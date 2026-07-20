//! Motor de arquivos compactados da v0.1: **zip** (ler/criar) e **tar/tar.gz**
//! (ler/criar tar.gz), com leitura de índice SEM extrair, extração com
//! progresso/cancelamento e criação com progresso.
//!
//! Segurança: extração SEMPRE sanitiza os caminhos (zip-slip — nada sai do
//! destino); razão de expansão suspeita liga o aviso de zip bomb na UI.

use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use sevenz_rust2::{ArchiveReader, Password};
use tauri::{AppHandle, Emitter};

use crate::rar;
use crate::split::{self, SplitReader};

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
    Rar,
}

/// Abre o fluxo de leitura de um tar já com o decodificador certo.
fn tar_reader(file: SplitReader, format: Format) -> Box<dyn Read> {
    match format {
        Format::Tar => Box::new(file),
        Format::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        Format::TarXz => Box::new(xz2::read::XzDecoder::new(file)),
        Format::TarBz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        Format::TarZst => Box::new(zstd::stream::read::Decoder::new(file).expect("zstd")),
        Format::Zip | Format::SevenZ | Format::Rar => unreachable!(),
    }
}

/// Abre o arquivo pra leitura — um arquivo simples OU um conjunto de volumes de
/// corte cru (`.zip.001`, `.7z.002`, …) apresentado como UM fluxo só.
///
/// Antes disso, barra o zip multi-disco de verdade (`.z01`), que NÃO é corte
/// cru: emendar os volumes não resolve (os deslocamentos do diretório central
/// são relativos ao disco), e sem essa checagem o crate `zip` estoura um
/// "invalid Zip archive" que não ajuda ninguém.
fn open_reader(path: &str) -> Result<SplitReader, String> {
    if split::multi_disk_zip(Path::new(path)) {
        return Err("MULTI_DISK_ZIP".into());
    }
    SplitReader::open(path)
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
    // Volume de corte cru: o formato mora no nome SEM o sufixo numérico
    // (`foo.zip.001` é um zip; `foo.tar.gz.002` é um tar.gz).
    let base = split::volume_base_name(Path::new(path)).unwrap_or_else(|| path.to_string());
    let lower = base.to_lowercase();
    if lower.ends_with(".rar") || crate::rar::is_old_volume_name(&lower) {
        Ok(Format::Rar)
    } else if lower.ends_with(".zip") {
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
        Err("formato não suportado (zip, rar, 7z, tar, tar.gz/xz/bz2/zst)".into())
    }
}

/// Normaliza um caminho interno: "/" como separador, sem "./" nem barra final.
pub(crate) fn norm_inner(raw: &str) -> String {
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
    // Num conjunto de volumes, "o tamanho do arquivo" é a SOMA dos volumes —
    // usar só o `.001` faria a razão de expansão mentir e ligar o alarme de
    // zip bomb em qualquer arquivo dividido.
    let archive_bytes = match format {
        Format::Rar => rar::volume_set(Path::new(path))
            .iter()
            .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
            .sum(),
        _ => open_reader(path)?.total(),
    };
    let mut entries: Vec<AEntry> = Vec::new();
    let mut total_size = 0u64;

    match format {
        Format::Rar => {
            entries = rar::list(path, None)?;
            total_size = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        }
        Format::Zip => {
            let file = open_reader(path)?;
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
            let reader = ArchiveReader::new(open_reader(path)?, Password::empty())
                .map_err(|e| e.to_string())?;
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
            let file = open_reader(path)?;
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

/// O `AppHandle` é DONO (não emprestado) porque o `rar.rs` precisa mandar o
/// Reporter pra dentro de um `Box<dyn Write>` `'static` — ver `rar::CountingWriter`.
pub(crate) struct Reporter {
    /// `None` nos testes (sem runtime Tauri) — aí nada é emitido.
    app: Option<AppHandle>,
    op_id: u64,
    done_files: u64,
    total_files: u64,
    done_bytes: u64,
    total_bytes: u64,
    last: Instant,
}

impl Reporter {
    pub(crate) fn new(
        app: Option<AppHandle>,
        op_id: u64,
        total_files: u64,
        total_bytes: u64,
    ) -> Self {
        Self { app, op_id, done_files: 0, total_files, done_bytes: 0, total_bytes, last: Instant::now() }
    }
    pub(crate) fn bytes(&mut self, n: u64, current: &str) {
        self.done_bytes += n;
        if self.last.elapsed().as_millis() >= 150 {
            self.emit(current);
        }
    }
    pub(crate) fn file_done(&mut self, current: &str) {
        self.done_files += 1;
        self.emit(current);
    }
    fn emit(&mut self, current: &str) {
        self.last = Instant::now();
        let Some(app) = self.app.as_ref() else { return };
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
pub(crate) fn safe_join(dest: &Path, inner: &str) -> Result<PathBuf, String> {
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
pub(crate) fn selected(inner: &str, filter: &Option<Vec<String>>) -> bool {
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
    cancel: &Arc<AtomicBool>,
    archive: &str,
    dest: &str,
    filter: &Option<Vec<String>>,
    password: Option<&str>,
) -> Result<Option<String>, String> {
    let dest_dir = PathBuf::from(dest);
    fs::create_dir_all(&dest_dir).map_err(|e| format!("{dest}: {e}"))?;
    let format = detect_format(archive)?;

    match format {
        Format::Rar => {
            rar::extract(app, op_id, cancel, archive, &dest_dir, filter, password)?;
        }
        Format::Zip => {
            let file = open_reader(archive)?;
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
            let mut rep = Reporter::new(app.cloned(), op_id, total_files, total_bytes);

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
            let mut reader =
                ArchiveReader::new(open_reader(archive)?, pw).map_err(|e| e.to_string())?;
            let (mut total_files, mut total_bytes) = (0u64, 0u64);
            for f in &reader.archive().files {
                let inner = norm_inner(&f.name);
                if !inner.is_empty() && !f.is_directory && selected(&inner, filter) {
                    total_files += 1;
                    total_bytes += f.size;
                }
            }
            let mut rep = Reporter::new(app.cloned(), op_id, total_files, total_bytes);
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
                let file = open_reader(archive)?;
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
            let mut rep = Reporter::new(app.cloned(), op_id, total_files, total_bytes);

            let file = open_reader(archive)?;
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
    let mut rep = Reporter::new(Some(app.clone()), op_id, files.len() as u64, total_bytes);
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

// ---------- adicionar / remover num zip existente ----------

/// Adiciona e/ou remove itens de um zip **sem descompactar o que já estava lá**.
///
/// Dois caminhos, e a diferença importa:
///
/// * **Só adicionar, sem colisão de nome** → `ZipWriter::new_append`. O arquivo
///   é aberto pra escrita, o cursor vai pro fim dos dados existentes, os novos
///   entram e só o diretório central é reescrito. Os bytes antigos **não são
///   nem lidos**. É o caminho de custo O(novos).
/// * **Remover, ou sobrescrever um nome que já existe** → reconstrói num
///   temporário e troca no fim. Mesmo aqui nada é descompactado: cada entrada
///   preservada passa por `raw_copy_file`, que copia o fluxo JÁ COMPRIMIDO com
///   o CRC e o método originais. Custo O(bytes comprimidos), não O(bytes
///   descompactados) — e sem perda de qualidade nem re-cifragem.
///
/// A prova de que não re-comprime está no teste `atualizar_nao_recomprime`:
/// ele compara `compressed_size` e CRC de cada sobrevivente antes e depois.
pub fn update(
    app: &AppHandle,
    op_id: u64,
    cancel: Arc<AtomicBool>,
    archive: String,
    add: Vec<String>,
    remove: Vec<String>,
    password: Option<String>,
) {
    let result =
        update_inner(Some(app), op_id, &cancel, &archive, &add, &remove, password.as_deref());
    emit_done(app, op_id, result, cancel.load(Ordering::Relaxed));
}

fn update_inner(
    app: Option<&AppHandle>,
    op_id: u64,
    cancel: &Arc<AtomicBool>,
    archive: &str,
    add: &[String],
    remove: &[String],
    password: Option<&str>,
) -> Result<Option<String>, String> {
    if !matches!(detect_format(archive)?, Format::Zip) {
        return Err("UPDATE_ONLY_ZIP".into());
    }
    if split::volume_parts(Path::new(archive)).is_some() {
        // Escrever de volta num conjunto dividido exigiria re-picar os volumes;
        // o corte cru é só-leitura por decisão de escopo.
        return Err("UPDATE_NOT_ON_SPLIT".into());
    }
    if add.is_empty() && remove.is_empty() {
        return Ok(Some(archive.to_string()));
    }

    let (files, total_bytes) = if add.is_empty() {
        (Vec::new(), 0)
    } else {
        walk_sources(add)?
    };
    let novos: std::collections::HashSet<&str> = files.iter().map(|(_, i)| i.as_str()).collect();

    // Quem sai: o filtro de remoção OU um nome que os novos vão sobrescrever.
    let rm_filter = if remove.is_empty() { None } else { Some(remove.to_vec()) };
    let mut colide = false;
    let mut sobrevivem = 0u64;
    {
        let za_file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
        let mut za = zip::ZipArchive::new(za_file).map_err(|e| e.to_string())?;
        for i in 0..za.len() {
            let f = za.by_index_raw(i).map_err(|e| e.to_string())?;
            let inner = norm_inner(f.name());
            if novos.contains(inner.as_str()) {
                colide = true;
            } else if !selected(&inner, &rm_filter) || rm_filter.is_none() {
                sobrevivem += 1;
            }
        }
    }
    let rebuild = !remove.is_empty() || colide;

    let mut rep = Reporter::new(
        app.cloned(),
        op_id,
        files.len() as u64 + if rebuild { sobrevivem } else { 0 },
        total_bytes,
    );

    if !rebuild {
        // ---- caminho rápido: acrescenta no fim, sem tocar nos bytes antigos.
        let rw = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(archive)
            .map_err(|e| format!("{archive}: {e}"))?;
        let mut zw = zip::ZipWriter::new_append(rw).map_err(|e| e.to_string())?;
        for (disk, inner) in &files {
            if cancel.load(Ordering::Relaxed) {
                return Err("canceled".into());
            }
            copia_pra_dentro(&mut zw, disk, inner, zip_opts(password), cancel, &mut rep)?;
        }
        zw.finish().map_err(|e| e.to_string())?;
        return Ok(Some(archive.to_string()));
    }

    // ---- caminho reconstrução: raw_copy_file preserva os bytes comprimidos.
    let tmp = PathBuf::from(format!("{archive}.localzip-tmp"));
    let res = (|| -> Result<(), String> {
        let za_file = fs::File::open(archive).map_err(|e| format!("{archive}: {e}"))?;
        let mut za = zip::ZipArchive::new(za_file).map_err(|e| e.to_string())?;
        let out = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        let mut zw = zip::ZipWriter::new(out);
        for i in 0..za.len() {
            if cancel.load(Ordering::Relaxed) {
                return Err("canceled".into());
            }
            let f = za.by_index_raw(i).map_err(|e| e.to_string())?;
            let inner = norm_inner(f.name());
            if inner.is_empty()
                || novos.contains(inner.as_str())
                || (rm_filter.is_some() && selected(&inner, &rm_filter))
            {
                continue;
            }
            zw.raw_copy_file(f).map_err(|e| e.to_string())?;
            rep.file_done(&inner);
        }
        for (disk, inner) in &files {
            if cancel.load(Ordering::Relaxed) {
                return Err("canceled".into());
            }
            copia_pra_dentro(&mut zw, disk, inner, zip_opts(password), cancel, &mut rep)?;
        }
        zw.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = res {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // Só troca no fim: se algo estourar no meio, o original continua intacto.
    fs::rename(&tmp, archive).map_err(|e| format!("{archive}: {e}"))?;
    Ok(Some(archive.to_string()))
}

/// Mesmas opções da criação (Deflate + AES-256 quando há senha). Precisa ser
/// `fn` com lifetime explícito: o `FileOptions` guarda a senha emprestada.
fn zip_opts(password: Option<&str>) -> zip::write::FileOptions<'_, ()> {
    let mut o = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        o = o.with_aes_encryption(zip::AesMode::Aes256, pw);
    }
    o
}

fn copia_pra_dentro<W: Write + Seek>(
    zw: &mut zip::ZipWriter<W>,
    disk: &Path,
    inner: &str,
    opt: zip::write::FileOptions<'_, ()>,
    cancel: &AtomicBool,
    rep: &mut Reporter,
) -> Result<(), String> {
    zw.start_file(inner.to_string(), opt).map_err(|e| e.to_string())?;
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
    Ok(())
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
        Format::Rar => return rar::test_integrity(archive, password),
        Format::Zip => {
            let file = open_reader(archive).map_err(|e| (String::new(), e))?;
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
            let mut reader = ArchiveReader::new(
                open_reader(archive).map_err(|e| (String::new(), e))?,
                pw,
            )
            .map_err(|e| (String::new(), e.to_string()))?;
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
            let file = open_reader(archive).map_err(|e| (String::new(), e))?;
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
        let cancel = Arc::new(AtomicBool::new(false));

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
        let cancel = Arc::new(AtomicBool::new(false));

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

    // ---- volumes de corte cru (.001) e zip multi-disco (.z01) ----

    /// Pica um arquivo em volumes `.001`, `.002`, … como o 7-Zip faz.
    fn pica(src: &Path, pedaco: usize) -> Vec<PathBuf> {
        let dados = fs::read(src).unwrap();
        let mut out = Vec::new();
        for (i, c) in dados.chunks(pedaco).enumerate() {
            let p = src.with_file_name(format!(
                "{}.{:03}",
                src.file_name().unwrap().to_string_lossy(),
                i + 1
            ));
            fs::write(&p, c).unwrap();
            out.push(p);
        }
        out
    }

    #[test]
    fn zip_dividido_em_001_abre_lista_e_extrai() {
        // O caso do item B3b: um zip picado em volumes de corte cru tem que
        // abrir, listar e extrair como se fosse um arquivo só — sem emendar
        // nada em disco antes.
        let base = std::env::temp_dir().join("localzip-test-split-zip");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("grande.zip");
        // Conteúdo grande o bastante pra uma entrada ATRAVESSAR a fronteira do
        // volume — é aí que um SplitReader errado se denuncia.
        let recheio: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        {
            let out = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            let opt: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("a/grande.bin", opt).unwrap();
            zw.write_all(&recheio).unwrap();
            zw.start_file("b.txt", opt).unwrap();
            zw.write_all(b"pequeno").unwrap();
            zw.finish().unwrap();
        }
        let inteiro = fs::read(&zip_path).unwrap();
        let vols = pica(&zip_path, 64 * 1024);
        assert!(vols.len() > 3, "esperava vários volumes, veio {}", vols.len());
        fs::remove_file(&zip_path).unwrap(); // só os volumes sobram no disco

        let v1 = vols[0].to_str().unwrap();
        // Detecção de formato olha o nome SEM o sufixo: `.zip.001` é zip.
        assert!(matches!(detect_format(v1).unwrap(), Format::Zip));

        let info = open_archive(v1).unwrap();
        assert_eq!(info.entries.len(), 2, "listou pelos volumes emendados");
        // `archive_bytes` é a SOMA dos volumes, não o tamanho do `.001`.
        assert_eq!(info.archive_bytes, inteiro.len() as u64);
        assert!(!info.bomb_suspect);

        let cancel = Arc::new(AtomicBool::new(false));
        let dest = base.join("out");
        extract_inner(None, 1, &cancel, v1, dest.to_str().unwrap(), &None, None).unwrap();
        assert_eq!(fs::read(dest.join("a/grande.bin")).unwrap(), recheio);
        assert_eq!(fs::read(dest.join("b.txt")).unwrap(), b"pequeno");

        // Abrir por um volume do MEIO acha o conjunto inteiro do mesmo jeito.
        let info2 = open_archive(vols[2].to_str().unwrap()).unwrap();
        assert_eq!(info2.entries.len(), 2);

        // E a integridade (CRC de cada entrada) fecha lendo pelos volumes.
        assert!(test_integrity(v1, None).ok);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn zip_multi_disco_da_erro_claro_em_vez_de_erro_do_crate() {
        // `.z01` NÃO é corte cru: os deslocamentos do diretório central são
        // por disco. Sem a checagem, o crate zip cospe "invalid Zip archive".
        let base = std::env::temp_dir().join("localzip-test-multidisk");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let z = base.join("a.zip");
        {
            let out = fs::File::create(&z).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            zw.start_file("x.txt", zip::write::SimpleFileOptions::default()).unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        let zs = z.to_str().unwrap();
        // Sozinho abre normal.
        assert!(open_archive(zs).is_ok());
        // Com o irmão `.z01` do lado, vira erro NOSSO, com código próprio.
        fs::write(base.join("a.z01"), b"disco 1").unwrap();
        assert_eq!(open_archive(zs).err().unwrap(), "MULTI_DISK_ZIP");
        let cancel = Arc::new(AtomicBool::new(false));
        let d = base.join("out");
        assert_eq!(
            extract_inner(None, 1, &cancel, zs, d.to_str().unwrap(), &None, None).unwrap_err(),
            "MULTI_DISK_ZIP"
        );
        let _ = fs::remove_dir_all(&base);
    }

    // ---- adicionar / remover sem re-extrair ----

    /// Zip com um membro grande e compressível + alguns pequenos.
    fn zip_gordo(dir: &Path, mb: usize) -> (PathBuf, Vec<u8>) {
        fs::create_dir_all(dir).unwrap();
        // Compressível, mas não trivial: bloco pseudo-aleatório repetido.
        let bloco: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        let mut grande = Vec::with_capacity(mb * 1024 * 1024);
        while grande.len() < mb * 1024 * 1024 {
            grande.extend_from_slice(&bloco);
        }
        let z = dir.join("gordo.zip");
        let out = fs::File::create(&z).unwrap();
        let mut zw = zip::ZipWriter::new(out);
        let opt = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("dados/grande.bin", opt).unwrap();
        zw.write_all(&grande).unwrap();
        for i in 0..3 {
            zw.start_file(format!("docs/n{i}.txt"), opt).unwrap();
            zw.write_all(format!("nota {i}").as_bytes()).unwrap();
        }
        zw.finish().unwrap();
        (z, grande)
    }

    /// (nome, tamanho comprimido, crc) de cada entrada — a impressão digital
    /// que prova se os bytes comprimidos foram preservados ou refeitos.
    fn digital(z: &Path) -> Vec<(String, u64, u32)> {
        let mut za = zip::ZipArchive::new(fs::File::open(z).unwrap()).unwrap();
        (0..za.len())
            .map(|i| {
                let f = za.by_index_raw(i).unwrap();
                (f.name().to_string(), f.compressed_size(), f.crc32())
            })
            .collect()
    }

    #[test]
    fn atualizar_nao_recomprime_nem_re_extrai() {
        // A afirmação "não re-extrai" tem que ser MEDIDA, não prometida.
        // Duas provas independentes:
        //  1) estrutural: os bytes comprimidos e o CRC dos sobreviventes são
        //     idênticos byte a byte (impossível se tivesse recomprimido);
        //  2) tempo: a atualização é MUITO mais rápida que descompactar e
        //     recompactar o mesmo zip.
        let base = std::env::temp_dir().join("localzip-test-update");
        let _ = fs::remove_dir_all(&base);
        let (z, grande) = zip_gordo(&base, 32);
        let zs = z.to_str().unwrap().to_string();
        let antes = digital(&z);
        let big_antes = antes.iter().find(|(n, ..)| n == "dados/grande.bin").unwrap().clone();

        // Baseline honesto: o que custaria RE-EXTRAIR e recompactar tudo.
        let t0 = Instant::now();
        {
            let mut za = zip::ZipArchive::new(fs::File::open(&z).unwrap()).unwrap();
            let out = fs::File::create(base.join("refeito.zip")).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            let opt = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for i in 0..za.len() {
                let name = za.by_index_raw(i).unwrap().name().to_string();
                let mut f = za.by_index(i).unwrap();
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).unwrap();
                drop(f);
                zw.start_file(name, opt).unwrap();
                zw.write_all(&buf).unwrap();
            }
            zw.finish().unwrap();
        }
        let recompactar = t0.elapsed();

        // Um arquivo novo pra entrar.
        let novo = base.join("novo.txt");
        fs::write(&novo, b"entrou depois").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        // (a) SÓ ADICIONAR: caminho rápido (new_append) — nem lê os bytes antigos.
        let t1 = Instant::now();
        update_inner(
            None,
            1,
            &cancel,
            &zs,
            &[novo.to_string_lossy().into_owned()],
            &[],
            None,
        )
        .unwrap();
        let adicionar = t1.elapsed();

        let dep_add = digital(&z);
        assert!(dep_add.iter().any(|(n, ..)| n == "novo.txt"), "o novo entrou");
        assert_eq!(
            dep_add.iter().find(|(n, ..)| n == "dados/grande.bin").unwrap(),
            &big_antes,
            "o membro grande foi RECOMPRIMIDO ao adicionar (deveria ser intocado)"
        );

        // (b) REMOVER: reconstrói, mas por raw_copy_file (sem descompactar).
        let t2 = Instant::now();
        update_inner(None, 2, &cancel, &zs, &[], &["docs".to_string()], None).unwrap();
        let remover = t2.elapsed();

        let dep_rm = digital(&z);
        assert!(!dep_rm.iter().any(|(n, ..)| n.starts_with("docs/")), "docs/ saiu");
        assert!(dep_rm.iter().any(|(n, ..)| n == "novo.txt"), "novo.txt ficou");
        assert_eq!(
            dep_rm.iter().find(|(n, ..)| n == "dados/grande.bin").unwrap(),
            &big_antes,
            "remover RECOMPRIMIU o membro grande (deveria ser cópia crua)"
        );

        // O zip continua válido e o conteúdo continua correto de verdade.
        assert!(test_integrity(&zs, None).ok, "zip atualizado deveria estar íntegro");
        let dest = base.join("out");
        extract_inner(None, 3, &cancel, &zs, dest.to_str().unwrap(), &None, None).unwrap();
        assert_eq!(fs::read(dest.join("dados/grande.bin")).unwrap().len(), grande.len());
        assert_eq!(fs::read(dest.join("dados/grande.bin")).unwrap(), grande);
        assert_eq!(fs::read(dest.join("novo.txt")).unwrap(), b"entrou depois");

        eprintln!(
            "[MEDIDO] 32 MB: re-extrair+recompactar {recompactar:?} | adicionar {adicionar:?} | remover {remover:?}"
        );
        // Margem folgada de propósito: o que importa é a ordem de grandeza.
        assert!(
            adicionar * 4 < recompactar,
            "adicionar ({adicionar:?}) deveria ser MUITO mais barato que recompactar ({recompactar:?})"
        );
        assert!(
            remover * 4 < recompactar,
            "remover ({remover:?}) deveria ser MUITO mais barato que recompactar ({recompactar:?})"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn atualizar_sobrescreve_nome_repetido_e_recusa_formato_errado() {
        let base = std::env::temp_dir().join("localzip-test-update2");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let z = base.join("a.zip");
        {
            let out = fs::File::create(&z).unwrap();
            let mut zw = zip::ZipWriter::new(out);
            let opt = zip::write::SimpleFileOptions::default();
            zw.start_file("nota.txt", opt).unwrap();
            zw.write_all(b"versao velha").unwrap();
            zw.start_file("outro.txt", opt).unwrap();
            zw.write_all(b"fica").unwrap();
            zw.finish().unwrap();
        }
        let zs = z.to_str().unwrap().to_string();
        let cancel = Arc::new(AtomicBool::new(false));

        // Nome que já existe: não pode duplicar a entrada — a nova substitui.
        let novo = base.join("nota.txt");
        fs::write(&novo, b"versao nova").unwrap();
        update_inner(None, 1, &cancel, &zs, &[novo.to_string_lossy().into_owned()], &[], None)
            .unwrap();
        let d = digital(&z);
        assert_eq!(d.iter().filter(|(n, ..)| n == "nota.txt").count(), 1, "duplicou: {d:?}");
        let dest = base.join("out");
        extract_inner(None, 2, &cancel, &zs, dest.to_str().unwrap(), &None, None).unwrap();
        assert_eq!(fs::read(dest.join("nota.txt")).unwrap(), b"versao nova");
        assert_eq!(fs::read(dest.join("outro.txt")).unwrap(), b"fica");

        // Formato só-leitura: recusa com código claro em vez de estragar nada.
        let tgz = base.join("a.tar.gz");
        fs::write(&tgz, b"nao importa").unwrap();
        let err = update_inner(
            None,
            3,
            &cancel,
            tgz.to_str().unwrap(),
            &[novo.to_string_lossy().into_owned()],
            &[],
            None,
        )
        .unwrap_err();
        assert_eq!(err, "UPDATE_ONLY_ZIP");

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
