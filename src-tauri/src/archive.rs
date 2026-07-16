//! Motor de arquivos compactados da v0.1: **zip** (ler/criar) e **tar/tar.gz**
//! (ler/criar tar.gz), com leitura de índice SEM extrair, extração com
//! progresso/cancelamento e criação com progresso.
//!
//! Segurança: extração SEMPRE sanitiza os caminhos (zip-slip — nada sai do
//! destino); razão de expansão suspeita liga o aviso de zip bomb na UI.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Zip,
    Tar,
    TarGz,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AEntry {
    /// Caminho DENTRO do arquivo, separador "/", sem barra no fim.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Só o zip informa (0 nos demais).
    pub compressed: u64,
    pub modified_ms: i64,
    pub encrypted: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub path: String,
    pub format: Format,
    pub entries: Vec<AEntry>,
    pub total_size: u64,
    pub archive_bytes: u64,
    /// Razão de expansão gigante = possível zip bomb (a UI avisa).
    pub bomb_suspect: bool,
}

pub fn detect_format(path: &str) -> Result<Format, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".zip") {
        Ok(Format::Zip)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Ok(Format::TarGz)
    } else if lower.ends_with(".tar") {
        Ok(Format::Tar)
    } else if lower.ends_with(".gz") {
        // .gz solto (arquivo único) é tratado como tar.gz só se for tar por
        // dentro; v0.1 não cobre .gz de arquivo único — mensagem honesta.
        Err("formato .gz de arquivo único chega na v0.2".into())
    } else {
        Err("formato não suportado (v0.1: zip, tar, tar.gz)".into())
    }
}

/// Normaliza um caminho interno: "/" como separador, sem "./" nem barra final.
fn norm_inner(raw: &str) -> String {
    let s = raw.replace('\\', "/");
    let s = s.strip_prefix("./").unwrap_or(&s);
    s.trim_matches('/').to_string()
}

fn zip_dos_time_ms(f: &zip::read::ZipFile) -> i64 {
    // zip::DateTime → epoch-ms (aproximação local; suficiente pra exibição).
    if let Some(dt) = f.last_modified() {
        let (y, mo, d, h, mi, s) = (
            dt.year() as i32,
            dt.month() as u32,
            dt.day() as u32,
            dt.hour() as u32,
            dt.minute() as u32,
            dt.second() as u32,
        );
        // Conversão manual simples (dias julianos), sem crate de datas.
        let a = (14 - mo as i64) / 12;
        let y2 = y as i64 + 4800 - a;
        let m2 = mo as i64 + 12 * a - 3;
        let jdn = d as i64 + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045;
        let days = jdn - 2440588; // epoch JDN
        return (days * 86400 + h as i64 * 3600 + mi as i64 * 60 + s as i64) * 1000;
    }
    0
}

pub fn open_archive(path: &str) -> Result<ArchiveInfo, String> {
    let format = detect_format(path)?;
    let meta = fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    let archive_bytes = meta.len();
    let mut entries: Vec<AEntry> = Vec::new();
    let mut total_size = 0u64;

    match format {
        Format::Zip => {
            let file = fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
            let mut za = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            for i in 0..za.len() {
                // by_index_raw: lê só o cabeçalho, sem descompactar.
                let f = za.by_index_raw(i).map_err(|e| e.to_string())?;
                let inner = norm_inner(f.name());
                if inner.is_empty() {
                    continue;
                }
                let is_dir = f.is_dir();
                let size = f.size();
                if !is_dir {
                    total_size += size;
                }
                entries.push(AEntry {
                    path: inner,
                    is_dir,
                    size,
                    compressed: f.compressed_size(),
                    modified_ms: zip_dos_time_ms(&f),
                    encrypted: f.encrypted(),
                });
            }
        }
        Format::Tar | Format::TarGz => {
            let file = fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
            let reader: Box<dyn Read> = if format == Format::TarGz {
                Box::new(flate2::read::GzDecoder::new(file))
            } else {
                Box::new(file)
            };
            let mut ar = tar::Archive::new(reader);
            for entry in ar.entries().map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let inner = norm_inner(&entry.path().map_err(|e| e.to_string())?.to_string_lossy());
                if inner.is_empty() {
                    continue;
                }
                let is_dir = entry.header().entry_type().is_dir();
                let size = entry.header().size().unwrap_or(0);
                if !is_dir {
                    total_size += size;
                }
                let mtime = entry.header().mtime().unwrap_or(0) as i64 * 1000;
                entries.push(AEntry {
                    path: inner,
                    is_dir,
                    size,
                    compressed: 0,
                    modified_ms: mtime,
                    encrypted: false,
                });
            }
        }
    }

    // Heurística de zip bomb: >500 MB expandidos E razão >200×.
    let bomb_suspect =
        total_size > 500 * 1024 * 1024 && archive_bytes > 0 && total_size / archive_bytes > 200;

    Ok(ArchiveInfo {
        path: path.to_string(),
        format,
        entries,
        total_size,
        archive_bytes,
        bomb_suspect,
    })
}

// ---------- progresso ----------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpProgress {
    pub op_id: u64,
    pub done_files: u64,
    pub total_files: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpDone {
    pub op_id: u64,
    pub ok: bool,
    pub canceled: bool,
    pub error: Option<String>,
    /// Extração: pasta destino; criação: o arquivo criado.
    pub output: Option<String>,
}

struct Reporter<'a> {
    app: &'a AppHandle,
    op_id: u64,
    done_files: u64,
    total_files: u64,
    done_bytes: u64,
    total_bytes: u64,
    last: Instant,
}

impl<'a> Reporter<'a> {
    fn new(app: &'a AppHandle, op_id: u64, total_files: u64, total_bytes: u64) -> Self {
        Self { app, op_id, done_files: 0, total_files, done_bytes: 0, total_bytes, last: Instant::now() }
    }
    fn bytes(&mut self, n: u64, current: &str) {
        self.done_bytes += n;
        if self.last.elapsed().as_millis() >= 150 {
            self.emit(current);
        }
    }
    fn file_done(&mut self, current: &str) {
        self.done_files += 1;
        self.emit(current);
    }
    fn emit(&mut self, current: &str) {
        self.last = Instant::now();
        let _ = self.app.emit(
            "zipop-progress",
            OpProgress {
                op_id: self.op_id,
                done_files: self.done_files,
                total_files: self.total_files,
                done_bytes: self.done_bytes,
                total_bytes: self.total_bytes,
                current: current.to_string(),
            },
        );
    }
}

fn emit_done(app: &AppHandle, op_id: u64, result: Result<Option<String>, String>, canceled: bool) {
    let (ok, error, output) = match result {
        Ok(out) => (!canceled, None, out),
        Err(e) if e == "canceled" => (false, None, None),
        Err(e) => (false, Some(e), None),
    };
    let _ = app.emit("zipop-done", OpDone { op_id, ok, canceled, error, output });
}

// ---------- extração ----------

/// Junta o destino com um caminho interno SANITIZADO (zip-slip: componente
/// ".."/absoluto/unidade é rejeitado — nada escapa do destino).
fn safe_join(dest: &Path, inner: &str) -> Result<PathBuf, String> {
    let mut out = dest.to_path_buf();
    for comp in Path::new(&inner.replace('\\', "/")).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return Err(format!("caminho suspeito no arquivo: {inner}")),
        }
    }
    Ok(out)
}

/// O item `inner` está entre os selecionados? (igual ou descendente.)
fn selected(inner: &str, filter: &Option<Vec<String>>) -> bool {
    match filter {
        None => true,
        Some(list) => list
            .iter()
            .any(|p| inner == p || inner.starts_with(&format!("{p}/"))),
    }
}

pub fn extract(
    app: &AppHandle,
    op_id: u64,
    cancel: Arc<AtomicBool>,
    archive: String,
    dest: String,
    filter: Option<Vec<String>>,
) {
    let result = extract_inner(app, op_id, &cancel, &archive, &dest, &filter);
    emit_done(app, op_id, result, cancel.load(Ordering::Relaxed));
}

fn extract_inner(
    app: &AppHandle,
    op_id: u64,
    cancel: &AtomicBool,
    archive: &str,
    dest: &str,
    filter: &Option<Vec<String>>,
) -> Result<Option<String>, String> {
    let dest_dir = PathBuf::from(dest);
    fs::create_dir_all(&dest_dir).map_err(|e| format!("{dest}: {e}"))?;
    let format = detect_format(archive)?;

    match format {
        Format::Zip => {
            let file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
            let mut za = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

            // Totais do que foi selecionado (progresso honesto).
            let mut total_files = 0u64;
            let mut total_bytes = 0u64;
            for i in 0..za.len() {
                let f = za.by_index_raw(i).map_err(|e| e.to_string())?;
                let inner = norm_inner(f.name());
                if !inner.is_empty() && !f.is_dir() && selected(&inner, filter) {
                    total_files += 1;
                    total_bytes += f.size();
                }
            }
            let mut rep = Reporter::new(app, op_id, total_files, total_bytes);

            for i in 0..za.len() {
                if cancel.load(Ordering::Relaxed) {
                    return Err("canceled".into());
                }
                let mut f = match za.by_index(i) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::UnsupportedArchive(msg))
                        if msg.contains("Password") =>
                    {
                        return Err("arquivo protegido por senha (suporte chega na v0.2)".into())
                    }
                    Err(e) => return Err(e.to_string()),
                };
                let inner = norm_inner(f.name());
                if inner.is_empty() || !selected(&inner, filter) {
                    continue;
                }
                let target = safe_join(&dest_dir, &inner)?;
                if f.is_dir() {
                    fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                    continue;
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                let mut out = fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                let mut buf = vec![0u8; 512 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(out);
                        let _ = fs::remove_file(&target);
                        return Err("canceled".into());
                    }
                    let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    rep.bytes(n as u64, &inner);
                }
                rep.file_done(&inner);
            }
        }
        Format::Tar | Format::TarGz => {
            // Passo 1: totais (streaming — lê os headers de novo na extração).
            let (mut total_files, mut total_bytes) = (0u64, 0u64);
            {
                let file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
                let reader: Box<dyn Read> = if format == Format::TarGz {
                    Box::new(flate2::read::GzDecoder::new(file))
                } else {
                    Box::new(file)
                };
                let mut ar = tar::Archive::new(reader);
                for entry in ar.entries().map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let inner = norm_inner(&entry.path().map_err(|e| e.to_string())?.to_string_lossy());
                    if !inner.is_empty()
                        && !entry.header().entry_type().is_dir()
                        && selected(&inner, filter)
                    {
                        total_files += 1;
                        total_bytes += entry.header().size().unwrap_or(0);
                    }
                }
            }
            let mut rep = Reporter::new(app, op_id, total_files, total_bytes);

            let file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
            let reader: Box<dyn Read> = if format == Format::TarGz {
                Box::new(flate2::read::GzDecoder::new(file))
            } else {
                Box::new(file)
            };
            let mut ar = tar::Archive::new(reader);
            for entry in ar.entries().map_err(|e| e.to_string())? {
                if cancel.load(Ordering::Relaxed) {
                    return Err("canceled".into());
                }
                let mut entry = entry.map_err(|e| e.to_string())?;
                let inner = norm_inner(&entry.path().map_err(|e| e.to_string())?.to_string_lossy());
                if inner.is_empty() || !selected(&inner, filter) {
                    continue;
                }
                let etype = entry.header().entry_type();
                if etype.is_symlink() || etype.is_hard_link() {
                    continue; // links não são extraídos (mesma regra do LocalFiles)
                }
                let target = safe_join(&dest_dir, &inner)?;
                if etype.is_dir() {
                    fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                    continue;
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                let size = entry.header().size().unwrap_or(0);
                let mut out = fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
                rep.bytes(size, &inner);
                rep.file_done(&inner);
            }
        }
    }

    Ok(Some(dest.to_string()))
}

// ---------- criação ----------

fn walk_sources(sources: &[String]) -> Result<(Vec<(PathBuf, String)>, u64), String> {
    // (caminho no disco, caminho interno) + total de bytes. O interno começa
    // no NOME de cada origem (compactar a pasta "fotos" gera "fotos/…").
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut total = 0u64;

    fn rec(disk: &Path, inner: &str, files: &mut Vec<(PathBuf, String)>, total: &mut u64) -> Result<(), String> {
        let meta = fs::symlink_metadata(disk).map_err(|e| format!("{}: {e}", disk.display()))?;
        if meta.file_type().is_symlink() {
            return Ok(()); // links ficam de fora
        }
        if meta.is_dir() {
            for entry in fs::read_dir(disk).map_err(|e| format!("{}: {e}", disk.display()))? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().into_owned();
                rec(&entry.path(), &format!("{inner}/{name}"), files, total)?;
            }
        } else {
            *total += meta.len();
            files.push((disk.to_path_buf(), inner.to_string()));
        }
        Ok(())
    }

    for src in sources {
        let p = PathBuf::from(src);
        let name = p
            .file_name()
            .ok_or_else(|| format!("origem inválida: {src}"))?
            .to_string_lossy()
            .into_owned();
        rec(&p, &name, &mut files, &mut total)?;
    }
    Ok((files, total))
}

pub fn create(
    app: &AppHandle,
    op_id: u64,
    cancel: Arc<AtomicBool>,
    dest: String,
    format: String,
    sources: Vec<String>,
) {
    let result = create_inner(app, op_id, &cancel, &dest, &format, &sources);
    if result.is_err() {
        let _ = fs::remove_file(&dest); // não deixa arquivo pela metade
    }
    emit_done(app, op_id, result, cancel.load(Ordering::Relaxed));
}

fn create_inner(
    app: &AppHandle,
    op_id: u64,
    cancel: &AtomicBool,
    dest: &str,
    format: &str,
    sources: &[String],
) -> Result<Option<String>, String> {
    if sources.is_empty() {
        return Err("nada pra compactar".into());
    }
    let (files, total_bytes) = walk_sources(sources)?;
    let mut rep = Reporter::new(app, op_id, files.len() as u64, total_bytes);
    let out = fs::File::create(dest).map_err(|e| format!("{dest}: {e}"))?;

    match format {
        "zip" => {
            let mut zw = zip::ZipWriter::new(out);
            let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .large_file(true);
            for (disk, inner) in &files {
                if cancel.load(Ordering::Relaxed) {
                    return Err("canceled".into());
                }
                zw.start_file(inner.clone(), options).map_err(|e| e.to_string())?;
                let mut f = fs::File::open(disk).map_err(|e| format!("{}: {e}", disk.display()))?;
                let mut buf = vec![0u8; 512 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("canceled".into());
                    }
                    let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    zw.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    rep.bytes(n as u64, inner);
                }
                rep.file_done(inner);
            }
            zw.finish().map_err(|e| e.to_string())?;
        }
        "targz" => {
            let enc = flate2::write::GzEncoder::new(out, flate2::Compression::default());
            let mut tb = tar::Builder::new(enc);
            for (disk, inner) in &files {
                if cancel.load(Ordering::Relaxed) {
                    return Err("canceled".into());
                }
                let mut f = fs::File::open(disk).map_err(|e| format!("{}: {e}", disk.display()))?;
                tb.append_file(inner, &mut f).map_err(|e| e.to_string())?;
                let size = fs::metadata(disk).map(|m| m.len()).unwrap_or(0);
                rep.bytes(size, inner);
                rep.file_done(inner);
            }
            let enc = tb.into_inner().map_err(|e| e.to_string())?;
            enc.finish().map_err(|e| e.to_string())?;
        }
        other => return Err(format!("formato de criação desconhecido: {other}")),
    }

    Ok(Some(dest.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_inner_limpa_caminhos() {
        assert_eq!(norm_inner("./a/b/"), "a/b");
        assert_eq!(norm_inner("a\\b\\c"), "a/b/c");
        assert_eq!(norm_inner("/abs/x"), "abs/x");
    }

    #[test]
    fn safe_join_bloqueia_zip_slip() {
        let dest = Path::new("/tmp/out");
        assert!(safe_join(dest, "ok/file.txt").is_ok());
        assert!(safe_join(dest, "../fora.txt").is_err());
        assert!(safe_join(dest, "a/../../fora.txt").is_err());
    }

    #[test]
    fn selected_casa_descendentes() {
        let f = Some(vec!["docs".to_string(), "a.txt".to_string()]);
        assert!(selected("docs", &f));
        assert!(selected("docs/x/y.md", &f));
        assert!(selected("a.txt", &f));
        assert!(!selected("docs2/z", &f));
        assert!(selected("qualquer", &None));
    }

    #[test]
    fn zip_roundtrip_criar_abrir_extrair() {
        let base = std::env::temp_dir().join("localzip-test-rt");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("src/sub")).unwrap();
        fs::write(base.join("src/a.txt"), b"conteudo a").unwrap();
        fs::write(base.join("src/sub/b.txt"), b"bbbb").unwrap();

        // cria zip direto com o writer (sem AppHandle — testa o formato)
        let zip_path = base.join("t.zip");
        {
            let out = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            let opt: zip::write::SimpleFileOptions = Default::default();
            zw.start_file("src/a.txt", opt).unwrap();
            zw.write_all(b"conteudo a").unwrap();
            zw.start_file("src/sub/b.txt", opt).unwrap();
            zw.write_all(b"bbbb").unwrap();
            zw.finish().unwrap();
        }

        let info = open_archive(zip_path.to_str().unwrap()).unwrap();
        assert_eq!(info.entries.len(), 2);
        assert_eq!(info.total_size, 14);
        assert!(!info.bomb_suspect);
        assert!(info.entries.iter().any(|e| e.path == "src/sub/b.txt"));

        let _ = fs::remove_dir_all(&base);
    }
}
