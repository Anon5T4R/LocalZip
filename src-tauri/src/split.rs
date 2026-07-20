//! Volumes divididos (split archives) — LEITURA.
//!
//! Duas famílias completamente diferentes moram sob o mesmo apelido "arquivo
//! dividido", e confundir as duas custa caro:
//!
//! 1. **Corte cru** (`foo.zip.001`, `foo.zip.002`, …, também `.7z.001`,
//!    `.tar.gz.001`): os volumes são a *sequência de bytes* de um arquivo
//!    normal, picada com tesoura. Emendar os volumes na ordem devolve o
//!    original byte a byte. É o que o 7-Zip gera em "Dividir em volumes".
//!    É essa família que este módulo resolve — e resolve **sem copiar nada**:
//!    o [`SplitReader`] apresenta os N arquivos como um fluxo `Read + Seek` só.
//!
//! 2. **Zip realmente multi-disco** (`foo.z01`, `foo.z02`, `foo.zip`): cada
//!    volume é um "disco" com numeração própria, e os deslocamentos gravados no
//!    diretório central são *relativos ao disco*. Emendar não basta — teria que
//!    reescrever o deslocamento de cada entrada. Não é suportado, e o
//!    [`multi_disk_zip`] detecta pra dar erro claro em vez de erro obscuro do
//!    crate. (Ver README.)
//!
//! RAR dividido (`.part01.rar` / `.r00`) é outra coisa ainda: o próprio formato
//! sabe que é multi-volume, e mora em `rar.rs`. (Correção de uma suposição que
//! estava escrita aqui: o `rars` **não** costura sozinho. A API dele é
//! `extract_volumes_to(&[Archive], …)` — quem chama tem que achar os volumes no
//! disco e passar na ordem. É o `rar::volume_set` que faz isso.)

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Sufixo numérico de corte cru: `.001`, `.002`, … (3+ dígitos).
fn numeric_suffix(path: &Path) -> Option<(PathBuf, usize, usize)> {
    let name = path.file_name()?.to_str()?;
    let (stem, num) = name.rsplit_once('.')?;
    if num.len() < 3 || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // `.tar.gz` não é volume; `.001` é. O corte é sempre só dígitos.
    let n: usize = num.parse().ok()?;
    Some((path.with_file_name(stem), n, num.len()))
}

/// Um `.zip` com irmãos `.z01`/`.z02` é zip multi-disco de verdade (família 2).
pub fn multi_disk_zip(path: &Path) -> bool {
    let Some(stem) = path.file_stem() else { return false };
    let first = path.with_file_name(format!("{}.z01", stem.to_string_lossy()));
    first.is_file()
}

/// Volumes de um conjunto de corte cru, em ordem, a partir de QUALQUER volume.
///
/// Devolve `None` quando o caminho não é volume — aí é arquivo simples e o
/// chamador segue o caminho normal.
pub fn volume_parts(path: &Path) -> Option<Vec<PathBuf>> {
    let (base, _, width) = numeric_suffix(path)?;
    let mut parts = Vec::new();
    // Numeração começa em 001 (7-Zip) — mas aceita 000 se existir.
    let start = if base.with_extension_num(0, width).is_file() { 0 } else { 1 };
    let mut i = start;
    loop {
        let p = base.with_extension_num(i, width);
        if !p.is_file() {
            break;
        }
        parts.push(p);
        i += 1;
    }
    if parts.len() < 2 {
        return None; // volume solto (ou só o .001): trata como arquivo comum
    }
    Some(parts)
}

/// Nome sem o sufixo de volume — é dele que sai o formato (`foo.zip.001` → `foo.zip`).
pub fn volume_base_name(path: &Path) -> Option<String> {
    let (base, _, _) = numeric_suffix(path)?;
    Some(base.to_string_lossy().into_owned())
}

trait WithExtensionNum {
    fn with_extension_num(&self, n: usize, width: usize) -> PathBuf;
}

impl WithExtensionNum for PathBuf {
    fn with_extension_num(&self, n: usize, width: usize) -> PathBuf {
        let name = self.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.with_file_name(format!("{name}.{n:0width$}"))
    }
}

/// Os volumes de um corte cru vistos como UM fluxo `Read + Seek`.
///
/// Nenhum byte é copiado pra lugar nenhum: cada leitura vai direto no volume
/// que contém aquele deslocamento. Um zip de 3 volumes abre igual a um zip
/// simples, e uma entrada que atravessa a fronteira entre dois volumes é lida
/// em duas idas ao disco, sem o chamador saber.
pub struct SplitReader {
    /// (arquivo, deslocamento global onde começa, tamanho)
    parts: Vec<(fs::File, u64, u64)>,
    total: u64,
    pos: u64,
}

impl SplitReader {
    /// Abre um arquivo simples (1 parte) ou o conjunto de volumes.
    pub fn open(path: &str) -> Result<Self, String> {
        let p = Path::new(path);
        match volume_parts(p) {
            Some(parts) => Self::open_parts(&parts),
            None => Self::open_parts(&[p.to_path_buf()]),
        }
    }

    pub fn open_parts(paths: &[PathBuf]) -> Result<Self, String> {
        let mut parts = Vec::with_capacity(paths.len());
        let mut off = 0u64;
        for p in paths {
            let f = fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let len = f.metadata().map_err(|e| e.to_string())?.len();
            parts.push((f, off, len));
            off += len;
        }
        Ok(Self { parts, total: off, pos: 0 })
    }

    /// Total de bytes do conjunto (soma dos volumes).
    pub fn total(&self) -> u64 {
        self.total
    }
}

impl Read for SplitReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.pos >= self.total {
            return Ok(0);
        }
        let pos = self.pos;
        let Some((f, start, len)) = self.parts.iter_mut().find(|(_, s, l)| pos >= *s && pos < *s + *l)
        else {
            return Ok(0);
        };
        // Corta a leitura na fronteira do volume: quem chamou faz outra volta.
        let inner = pos - *start;
        let take = ((*len - inner) as usize).min(buf.len());
        f.seek(SeekFrom::Start(inner))?;
        let n = f.read(&mut buf[..take])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for SplitReader {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let new = match from {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.total as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if new < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek negativo"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("localzip-split-{name}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn sufixo_numerico_so_pega_volume() {
        assert!(numeric_suffix(Path::new("a/foo.zip.001")).is_some());
        assert!(numeric_suffix(Path::new("a/foo.7z.012")).is_some());
        // `.tar.gz` e `.zip` não são volumes; `.01` (2 dígitos) também não.
        assert!(numeric_suffix(Path::new("a/foo.tar.gz")).is_none());
        assert!(numeric_suffix(Path::new("a/foo.zip")).is_none());
        assert!(numeric_suffix(Path::new("a/foo.zip.01")).is_none());
    }

    #[test]
    fn volume_base_name_tira_o_sufixo() {
        let b = volume_base_name(Path::new("/x/foo.zip.001")).unwrap();
        assert!(b.ends_with("foo.zip"), "{b}");
        assert!(volume_base_name(Path::new("/x/foo.zip")).is_none());
    }

    #[test]
    fn split_reader_emenda_e_navega() {
        let dir = tmp("emenda");
        let dados: Vec<u8> = (0..30_000u32).map(|i| (i % 251) as u8).collect();
        let mut parts = Vec::new();
        for (i, c) in dados.chunks(7_000).enumerate() {
            let p = dir.join(format!("d.bin.{:03}", i + 1));
            fs::write(&p, c).unwrap();
            parts.push(p);
        }
        assert_eq!(parts.len(), 5);

        // Abrir por QUALQUER volume acha o conjunto inteiro, na ordem.
        let achados = volume_parts(&parts[3]).unwrap();
        assert_eq!(achados, parts);

        let mut r = SplitReader::open(parts[0].to_str().unwrap()).unwrap();
        assert_eq!(r.total(), dados.len() as u64);
        let mut tudo = Vec::new();
        r.read_to_end(&mut tudo).unwrap();
        assert_eq!(tudo, dados, "leitura sequencial devolve o original");

        // Seek no meio de um volume e leitura ATRAVESSANDO a fronteira.
        r.seek(SeekFrom::Start(6_990)).unwrap();
        let mut buf = vec![0u8; 20];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, dados[6_990..7_010], "leitura cruza a fronteira");

        r.seek(SeekFrom::End(-5)).unwrap();
        let mut fim = Vec::new();
        r.read_to_end(&mut fim).unwrap();
        assert_eq!(fim, dados[dados.len() - 5..]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn volume_solto_nao_vira_conjunto() {
        let dir = tmp("solto");
        let p = dir.join("x.zip.001");
        fs::write(&p, b"so eu").unwrap();
        assert!(volume_parts(&p).is_none(), "um volume só é arquivo comum");
        // Mas o SplitReader abre mesmo assim (1 parte).
        let mut r = SplitReader::open(p.to_str().unwrap()).unwrap();
        let mut s = String::new();
        r.read_to_string(&mut s).unwrap();
        assert_eq!(s, "so eu");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_disk_zip_detecta_z01() {
        let dir = tmp("multidisk");
        let z = dir.join("a.zip");
        fs::write(&z, b"x").unwrap();
        assert!(!multi_disk_zip(&z));
        let mut f = fs::File::create(dir.join("a.z01")).unwrap();
        f.write_all(b"y").unwrap();
        assert!(multi_disk_zip(&z), "irmão .z01 = zip multi-disco de verdade");
        let _ = fs::remove_dir_all(&dir);
    }
}
