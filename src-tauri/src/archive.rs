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
use sevenz_rust2::{ArchiveReader, Password};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    SevenZ,
}

/// Abre o fluxo de leitura de um tar já com o decodificador certo.
fn tar_reader(file: fs::File, format: Format) -> Box<dyn Read> {
    match format {
        Format::Tar => Box::new(file),
        Format::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        Format::TarXz => Box::new(xz2::read::XzDecoder::new(file)),
        Format::TarBz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        Format::TarZst => Box::new(zstd::stream::read::Decoder::new(file).expect("zstd")),
        Format::Zip | Format::SevenZ => unreachable!(),
    }
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
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Ok(Format::TarXz)
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") || lower.ends_with(".tbz") {
        Ok(Format::TarBz2)
    } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        Ok(Format::TarZst)
    } else if lower.ends_with(".tar") {
        Ok(Format::Tar)
    } else if lower.ends_with(".7z") {
        Ok(Format::SevenZ)
    } else {
        Err("formato não suportado (zip, 7z, tar, tar.gz/xz/bz2/zst; rar na v0.4)".into())
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
        Format::SevenZ => {
            // Lê só o cabeçalho (metadados de todos os arquivos), sem descompactar.
            let reader = ArchiveReader::open(path, Password::empty()).map_err(|e| e.to_string())?;
            for f in &reader.archive().files {
                let inner = norm_inner(&f.name);
                if inner.is_empty() {
                    continue;
                }
                if !f.is_directory {
                    total_size += f.size;
                }
                entries.push(AEntry {
                    path: inner,
                    is_dir: f.is_directory,
                    size: f.size,
                    compressed: 0,
                    modified_ms: 0,
                    encrypted: false,
                });
            }
        }
        _ => {
            let file = fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
            let mut ar = tar::Archive::new(tar_reader(file, format));
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
    /// `None` nos testes (sem runtime Tauri) — aí nada é emitido.
    app: Option<&'a AppHandle>,
    op_id: u64,
    done_files: u64,
    total_files: u64,
    done_bytes: u64,
    total_bytes: u64,
    last: Instant,
}

impl<'a> Reporter<'a> {
    fn new(app: Option<&'a AppHandle>, op_id: u64, total_files: u64, total_bytes: u64) -> Self {
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
        let Some(app) = self.app else { return };
        let _ = app.emit(
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
    password: Option<String>,
) {
    let result =
        extract_inner(Some(app), op_id, &cancel, &archive, &dest, &filter, password.as_deref());
    emit_done(app, op_id, result, cancel.load(Ordering::Relaxed));
}

fn extract_inner(
    app: Option<&AppHandle>,
    op_id: u64,
    cancel: &AtomicBool,
    archive: &str,
    dest: &str,
    filter: &Option<Vec<String>>,
    password: Option<&str>,
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
                // Decide pelo cabeçalho CRU, sem abrir o conteúdo: chamar
                // `by_index_decrypt` em pastas/entradas não-cifradas estourava
                // "senha incorreta" mesmo com a senha certa (bug do teste
                // real — zips de outras ferramentas marcam a flag de cifra na
                // entrada de pasta, que tem 0 bytes e nem header de cifra tem).
                let (inner, is_dir, encrypted) = {
                    let f = za.by_index_raw(i).map_err(|e| e.to_string())?;
                    (norm_inner(f.name()), f.is_dir(), f.encrypted())
                };
                if inner.is_empty() || !selected(&inner, filter) {
                    continue;
                }
                let target = safe_join(&dest_dir, &inner)?;
                if is_dir {
                    fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                    continue;
                }
                // Só entradas CIFRADAS passam pelo decrypt; o resto abre normal.
                let mut f = match (encrypted, password) {
                    (false, _) => za.by_index(i).map_err(|e| e.to_string())?,
                    (true, None) => return Err("NEED_PASSWORD".into()),
                    (true, Some(pw)) => match za.by_index_decrypt(i, pw.as_bytes()) {
                        Ok(f) => f,
                        Err(zip::result::ZipError::InvalidPassword) => {
                            return Err("WRONG_PASSWORD".into())
                        }
                        Err(e) => return Err(e.to_string()),
                    },
                };
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
                    // Erro de leitura/CRC em entrada cifrada ≈ senha errada que
                    // passou no check fraco do ZipCrypto (1 byte, 1/256).
                    let n = f.read(&mut buf).map_err(|e| {
                        if encrypted { "WRONG_PASSWORD".to_string() } else { e.to_string() }
                    })?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                    rep.bytes(n as u64, &inner);
                }
                rep.file_done(&inner);
            }
        }
        Format::SevenZ => {
            let pw = password.map(Password::from).unwrap_or_else(Password::empty);
            let mut reader = ArchiveReader::open(archive, pw).map_err(|e| e.to_string())?;
            let (mut total_files, mut total_bytes) = (0u64, 0u64);
            for f in &reader.archive().files {
                let inner = norm_inner(&f.name);
                if !inner.is_empty() && !f.is_directory && selected(&inner, filter) {
                    total_files += 1;
                    total_bytes += f.size;
                }
            }
            let mut rep = Reporter::new(app, op_id, total_files, total_bytes);
            // 7z costuma ser sólido: `for_each_entries` decodifica em sequência.
            let r = reader.for_each_entries(|entry, rd| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(sevenz_rust2::Error::Other("canceled".into()));
                }
                let inner = norm_inner(&entry.name);
                if inner.is_empty() || !selected(&inner, filter) {
                    return Ok(true);
                }
                let target =
                    safe_join(&dest_dir, &inner).map_err(|e| sevenz_rust2::Error::Other(e.into()))?;
                if entry.is_directory {
                    fs::create_dir_all(&target)?;
                    return Ok(true);
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = fs::File::create(&target)?;
                let mut buf = vec![0u8; 512 * 1024];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(out);
                        let _ = fs::remove_file(&target);
                        return Err(sevenz_rust2::Error::Other("canceled".into()));
                    }
                    let n = rd.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n])?;
                    rep.bytes(n as u64, &inner);
                }
                rep.file_done(&inner);
                Ok(true)
            });
            if cancel.load(Ordering::Relaxed) {
                return Err("canceled".into());
            }
            match r {
                Ok(_) => {}
                Err(sevenz_rust2::Error::PasswordRequired) => return Err("NEED_PASSWORD".into()),
                Err(sevenz_rust2::Error::MaybeBadPassword(_)) => {
                    // Com senha fornecida, "talvez senha ruim" = senha errada.
                    return Err(
                        if password.is_some() { "WRONG_PASSWORD" } else { "NEED_PASSWORD" }.into()
                    );
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        _ => {
            // Passo 1: totais (streaming — lê os headers de novo na extração).
            let (mut total_files, mut total_bytes) = (0u64, 0u64);
            {
                let file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
                let mut ar = tar::Archive::new(tar_reader(file, format));
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
            let mut ar = tar::Archive::new(tar_reader(file, format));
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
    password: Option<String>,
) {
    let result = create_inner(app, op_id, &cancel, &dest, &format, &sources, password.as_deref());
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
    password: Option<&str>,
) -> Result<Option<String>, String> {
    if sources.is_empty() {
        return Err("nada pra compactar".into());
    }
    let (files, total_bytes) = walk_sources(sources)?;
    let mut rep = Reporter::new(Some(app), op_id, files.len() as u64, total_bytes);
    let out = fs::File::create(dest).map_err(|e| format!("{dest}: {e}"))?;

    match format {
        "zip" => {
            let mut zw = zip::ZipWriter::new(out);
            // Sem anotar o tipo: `with_aes_encryption` amarra o lifetime da
            // senha, então o `SimpleFileOptions` ('static) não serviria.
            let mut options = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .large_file(true);
            // Senha = AES-256 (o suporte padrão do WinRAR/7-Zip pra zip cifrado).
            if let Some(pw) = password.filter(|p| !p.is_empty()) {
                options = options.with_aes_encryption(zip::AesMode::Aes256, pw);
            }
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

// ---------- testar integridade ----------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityResult {
    pub ok: bool,
    pub tested: u64,
    /// Nome do primeiro item com erro (vazio se tudo ok).
    pub bad: String,
    pub error: Option<String>,
}

/// Lê TODO o conteúdo de cada item — o zip valida o CRC de cada arquivo na
/// leitura; nos tar, ler até o fim detecta truncamento/corrupção do stream.
pub fn test_integrity(archive: &str, password: Option<&str>) -> IntegrityResult {
    match test_inner(archive, password) {
        Ok(tested) => IntegrityResult { ok: true, tested, bad: String::new(), error: None },
        Err((bad, error)) => IntegrityResult { ok: false, tested: 0, bad, error: Some(error) },
    }
}

fn test_inner(archive: &str, password: Option<&str>) -> Result<u64, (String, String)> {
    let format = detect_format(archive).map_err(|e| (String::new(), e))?;
    let mut tested = 0u64;
    let mut sink = [0u8; 256 * 1024];

    match format {
        Format::Zip => {
            let file = fs::File::open(archive).map_err(|e| (String::new(), e.to_string()))?;
            let mut za = zip::ZipArchive::new(file).map_err(|e| (String::new(), e.to_string()))?;
            for i in 0..za.len() {
                // Mesmo cuidado da extração: pastas nunca abrem conteúdo e só
                // entradas cifradas passam pelo decrypt.
                let (name, is_dir, encrypted) = {
                    let f = za.by_index_raw(i).map_err(|e| (String::new(), e.to_string()))?;
                    (norm_inner(f.name()), f.is_dir(), f.encrypted())
                };
                if is_dir {
                    continue;
                }
                let mut f = match (encrypted, password) {
                    (false, _) => za.by_index(i).map_err(|e| (name.clone(), e.to_string()))?,
                    (true, None) => return Err((name, "NEED_PASSWORD".into())),
                    (true, Some(pw)) => match za.by_index_decrypt(i, pw.as_bytes()) {
                        Ok(f) => f,
                        Err(zip::result::ZipError::InvalidPassword) => {
                            return Err((name, "WRONG_PASSWORD".into()))
                        }
                        Err(e) => return Err((name, e.to_string())),
                    },
                };
                loop {
                    match f.read(&mut sink) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e) => {
                            let msg =
                                if encrypted { "WRONG_PASSWORD".to_string() } else { e.to_string() };
                            return Err((name, msg));
                        }
                    }
                }
                tested += 1;
            }
        }
        Format::SevenZ => {
            let pw = password.map(Password::from).unwrap_or_else(Password::empty);
            let mut reader =
                ArchiveReader::open(archive, pw).map_err(|e| (String::new(), e.to_string()))?;
            let count = reader
                .archive()
                .files
                .iter()
                .filter(|f| !f.is_directory && f.has_stream)
                .count() as u64;
            reader
                .for_each_entries(|_entry, rd| {
                    let mut buf = [0u8; 256 * 1024];
                    loop {
                        match rd.read(&mut buf) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(e) => return Err(sevenz_rust2::Error::Other(e.to_string().into())),
                        }
                    }
                    Ok(true)
                })
                .map_err(|e| (String::new(), e.to_string()))?;
            tested = count;
        }
        _ => {
            let file = fs::File::open(archive).map_err(|e| (String::new(), e.to_string()))?;
            let mut ar = tar::Archive::new(tar_reader(file, format));
            let entries = ar.entries().map_err(|e| (String::new(), e.to_string()))?;
            for entry in entries {
                let mut entry = entry.map_err(|e| (String::new(), e.to_string()))?;
                let name = entry
                    .path()
                    .map(|p| norm_inner(&p.to_string_lossy()))
                    .unwrap_or_default();
                loop {
                    match entry.read(&mut sink) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e) => return Err((name, e.to_string())),
                    }
                }
                tested += 1;
            }
        }
    }
    Ok(tested)
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
    fn zip_com_senha_roundtrip_e_integridade() {
        // Cria um zip AES-256, testa a integridade com a senha certa e recusa
        // a senha errada.
        let base = std::env::temp_dir().join("localzip-test-pw");
        let _ = fs::remove_dir_all(&base);
        fs::write(base.join("../localzip-pw-src.txt"), b"segredo").ok();
        let zip_path = base.join("p.zip");
        fs::create_dir_all(&base).unwrap();
        {
            let out = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            let opt = zip::write::FileOptions::<()>::default()
                .with_aes_encryption(zip::AesMode::Aes256, "abc123");
            zw.start_file("s.txt", opt).unwrap();
            zw.write_all(b"segredo").unwrap();
            zw.finish().unwrap();
        }
        let info = open_archive(zip_path.to_str().unwrap()).unwrap();
        assert!(info.entries.iter().any(|e| e.encrypted));

        let ok = test_integrity(zip_path.to_str().unwrap(), Some("abc123"));
        assert!(ok.ok, "senha certa deveria validar");
        let bad = test_integrity(zip_path.to_str().unwrap(), Some("errada"));
        assert!(!bad.ok, "senha errada deveria falhar");

        let _ = fs::remove_dir_all(&base);
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

    #[test]
    fn zip_aes_com_pastas_extrai_com_senha() {
        // Reproduz o cenário do bug reportado: zip AES-256 COM PASTAS
        // (entrada de diretório não-cifrada + arquivos cifrados). Senha certa
        // TEM que extrair; errada/faltando viram códigos claros.
        let base = std::env::temp_dir().join("localzip-test-aes-pastas");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("p.zip");
        {
            let out = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            // Mesmas opções do create_inner (Deflated + large_file + AES-256).
            let opt = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .large_file(true)
                .with_aes_encryption(zip::AesMode::Aes256, "abc123");
            zw.add_directory("pasta/sub", zip::write::SimpleFileOptions::default()).unwrap();
            zw.start_file("pasta/sub/arquivo.txt", opt).unwrap();
            zw.write_all(b"aes com pastas").unwrap();
            zw.start_file("raiz.txt", opt).unwrap();
            zw.write_all(b"raiz").unwrap();
            zw.finish().unwrap();
        }
        let zip_s = zip_path.to_str().unwrap();
        let cancel = AtomicBool::new(false);

        // Senha certa: extrai tudo, com a estrutura de pastas.
        let dest = base.join("ok");
        extract_inner(None, 1, &cancel, zip_s, dest.to_str().unwrap(), &None, Some("abc123"))
            .expect("senha certa deveria extrair");
        assert_eq!(fs::read(dest.join("pasta/sub/arquivo.txt")).unwrap(), b"aes com pastas");
        assert_eq!(fs::read(dest.join("raiz.txt")).unwrap(), b"raiz");

        // Senha errada e sem senha: códigos claros pro front.
        let dest2 = base.join("err");
        let err = extract_inner(None, 2, &cancel, zip_s, dest2.to_str().unwrap(), &None, Some("errada"))
            .unwrap_err();
        assert_eq!(err, "WRONG_PASSWORD");
        let err = extract_inner(None, 3, &cancel, zip_s, dest2.to_str().unwrap(), &None, None)
            .unwrap_err();
        assert_eq!(err, "NEED_PASSWORD");

        // Testar integridade segue os mesmos caminhos.
        assert!(test_integrity(zip_s, Some("abc123")).ok);
        assert!(!test_integrity(zip_s, Some("errada")).ok);

        let _ = fs::remove_dir_all(&base);
    }

    // ---- ZipCrypto de verdade, gerado à mão (a escrita ZipCrypto é
    // `pub(crate)` no crate zip, então o teste monta os bytes do arquivo) ----

    fn crc32_byte(crc: u32, b: u8) -> u32 {
        let mut c = (crc ^ b as u32) & 0xff;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        (crc >> 8) ^ c
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffff_u32;
        for &b in data {
            c = crc32_byte(c, b);
        }
        c ^ 0xffff_ffff
    }

    /// Estado de chaves do ZipCrypto (PKZIP legado).
    struct ZcKeys(u32, u32, u32);
    impl ZcKeys {
        fn derive(password: &[u8]) -> Self {
            let mut k = ZcKeys(0x12345678, 0x23456789, 0x34567890);
            for &b in password {
                k.update(b);
            }
            k
        }
        fn update(&mut self, b: u8) {
            self.0 = crc32_byte(self.0, b);
            self.1 = self.1.wrapping_add(self.0 & 0xff).wrapping_mul(0x0808_8405).wrapping_add(1);
            self.2 = crc32_byte(self.2, (self.1 >> 24) as u8);
        }
        fn encrypt(&mut self, p: u8) -> u8 {
            let t = (self.2 as u16) | 3;
            let ks = (t.wrapping_mul(t ^ 1) >> 8) as u8;
            self.update(p);
            p ^ ks
        }
    }

    /// Cifra `plain` no formato ZipCrypto: 12 bytes de header (o último é o
    /// byte alto do CRC — validador PkzipCrc32) + dados, tudo cifrado.
    fn zipcrypto_encrypt(password: &[u8], crc: u32, plain: &[u8]) -> Vec<u8> {
        let mut k = ZcKeys::derive(password);
        let mut header = [0u8; 12];
        header[11] = (crc >> 24) as u8;
        header.iter().chain(plain.iter()).map(|&p| k.encrypt(p)).collect()
    }

    struct RawEntry<'a> {
        name: &'a str,
        /// bit 0 = cifrado (ZipCrypto).
        flags: u16,
        crc: u32,
        /// Bytes já no formato final (cifrados se for o caso); método STORED.
        data: Vec<u8>,
        uncomp: u32,
    }

    /// Monta um zip mínimo (STORED) byte a byte: local headers + central dir + EOCD.
    fn build_raw_zip(entries: &[RawEntry]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut offsets = Vec::new();
        for e in entries {
            offsets.push(out.len() as u32);
            out.extend_from_slice(&0x04034b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // versão mínima
            out.extend_from_slice(&e.flags.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // método STORED
            out.extend_from_slice(&0u16.to_le_bytes()); // hora DOS
            out.extend_from_slice(&0x21u16.to_le_bytes()); // data DOS (1980-01-01)
            out.extend_from_slice(&e.crc.to_le_bytes());
            out.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&e.uncomp.to_le_bytes());
            out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(e.name.as_bytes());
            out.extend_from_slice(&e.data);
        }
        let cd_start = out.len() as u32;
        for (e, off) in entries.iter().zip(&offsets) {
            out.extend_from_slice(&0x02014b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // made by
            out.extend_from_slice(&20u16.to_le_bytes()); // needed
            out.extend_from_slice(&e.flags.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // método
            out.extend_from_slice(&0u16.to_le_bytes()); // hora
            out.extend_from_slice(&0x21u16.to_le_bytes()); // data
            out.extend_from_slice(&e.crc.to_le_bytes());
            out.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&e.uncomp.to_le_bytes());
            out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(&0u16.to_le_bytes()); // comentário
            out.extend_from_slice(&0u16.to_le_bytes()); // disco
            out.extend_from_slice(&0u16.to_le_bytes()); // attrs internos
            let ext: u32 = if e.name.ends_with('/') { 0x10 } else { 0 };
            out.extend_from_slice(&ext.to_le_bytes());
            out.extend_from_slice(&off.to_le_bytes());
            out.extend_from_slice(e.name.as_bytes());
        }
        let cd_size = out.len() as u32 - cd_start;
        out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn zip_zipcrypto_extrai_e_pasta_com_flag_de_cifra_nao_estoura() {
        // ZipCrypto (a cifra legada que 7-Zip/WinRAR usam por padrão em "zip
        // com senha") + o caso patológico do bug: entrada de PASTA com a flag
        // de cifra ligada e 0 bytes de conteúdo. Antes, o by_index_decrypt
        // nessa pasta estourava "senha incorreta" mesmo com a senha certa.
        let pw = b"senha123";
        let plain: &[u8] = b"conteudo zipcrypto";
        let crc = crc32(plain);
        let entries = [
            RawEntry { name: "pasta/", flags: 1, crc: 0, data: Vec::new(), uncomp: 0 },
            RawEntry {
                name: "pasta/segredo.txt",
                flags: 1,
                crc,
                data: zipcrypto_encrypt(pw, crc, plain),
                uncomp: plain.len() as u32,
            },
            RawEntry {
                name: "aberto.txt",
                flags: 0,
                crc: crc32(b"sem senha"),
                data: b"sem senha".to_vec(),
                uncomp: 9,
            },
        ];
        let base = std::env::temp_dir().join("localzip-test-zipcrypto");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("zc.zip");
        fs::write(&zip_path, build_raw_zip(&entries)).unwrap();
        let zip_s = zip_path.to_str().unwrap();
        let cancel = AtomicBool::new(false);

        // Listagem marca o que é cifrado.
        let info = open_archive(zip_s).unwrap();
        assert!(info.entries.iter().any(|e| e.path == "pasta/segredo.txt" && e.encrypted));
        assert!(info.entries.iter().any(|e| e.path == "aberto.txt" && !e.encrypted));

        // Senha certa: extrai (ZipCrypto é suportado pelo crate zip 2.4).
        let dest = base.join("ok");
        extract_inner(None, 1, &cancel, zip_s, dest.to_str().unwrap(), &None, Some("senha123"))
            .expect("ZipCrypto com senha certa deveria extrair");
        assert_eq!(fs::read(dest.join("pasta/segredo.txt")).unwrap(), plain);
        assert_eq!(fs::read(dest.join("aberto.txt")).unwrap(), b"sem senha");

        // Senha errada e sem senha: códigos claros.
        let dest2 = base.join("err");
        let err = extract_inner(None, 2, &cancel, zip_s, dest2.to_str().unwrap(), &None, Some("errada"))
            .unwrap_err();
        assert_eq!(err, "WRONG_PASSWORD");
        let err = extract_inner(None, 3, &cancel, zip_s, dest2.to_str().unwrap(), &None, None)
            .unwrap_err();
        assert_eq!(err, "NEED_PASSWORD");

        // Testar integridade: pasta com flag não conta nem estoura.
        let r = test_integrity(zip_s, Some("senha123"));
        assert!(r.ok, "{:?}", r.error);
        assert_eq!(r.tested, 2);
        assert!(!test_integrity(zip_s, Some("errada")).ok);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sevenz_abre_lista_e_valida() {
        // Cria um .7z de verdade (via o próprio crate), lista o índice sem
        // extrair e valida a integridade lendo/decodificando todo o conteúdo.
        let base = std::env::temp_dir().join("localzip-test-7z");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("src/sub")).unwrap();
        fs::write(base.join("src/a.txt"), b"conteudo a").unwrap();
        fs::write(base.join("src/sub/b.txt"), b"bbbb").unwrap();
        let z = base.join("t.7z");
        sevenz_rust2::compress_to_path(base.join("src"), &z).unwrap();

        let info = open_archive(z.to_str().unwrap()).unwrap();
        assert!(matches!(info.format, Format::SevenZ));
        assert!(info.entries.iter().any(|e| e.path.ends_with("a.txt")));
        assert!(info.entries.iter().any(|e| e.path.ends_with("b.txt")));

        let r = test_integrity(z.to_str().unwrap(), None);
        assert!(r.ok, "7z íntegro deveria validar: {:?}", r.error);
        assert!(r.tested >= 2);

        let _ = fs::remove_dir_all(&base);
    }
}
