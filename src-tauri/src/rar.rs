//! RAR — LISTAGEM e EXTRAÇÃO. **Criar RAR está fora de escopo** (é formato
//! proprietário; só a WinRAR GmbH publica compactador).
//!
//! Crate: [`rars`] 0.4 — 100% Rust, licença **MIT OR Apache-2.0** (compatível
//! com a MIT do LocalZip), `unsafe_code = "forbid"` no próprio crate e nenhuma
//! dependência nativa. Por ser código-fonte compilado pelo cargo — e não
//! binário de terceiro baixado em tempo de build — não entra na regra de
//! espelho do `Local-runtimes`: é dependência igual ao `zip` e ao `sevenz-rust2`
//! que já estavam aqui.
//!
//! ## O que o `rars` NÃO faz sozinho (medido, não suposto)
//!
//! O comentário que abria o `split.rs` dizia que "o `rars` costura os volumes
//! sozinho". **Não costura.** A API é `extract_volumes_to(&[Archive], …)`: quem
//! chama tem que ACHAR os volumes no disco, abrir cada um e passar a fatia na
//! ordem. Por isso o [`volume_set`] aqui embaixo existe, e por isso ele conhece
//! as DUAS numerações de volume de RAR:
//!
//! * **Nova (RAR 3+ / RAR5):** `foo.part1.rar`, `foo.part2.rar`, … (a largura do
//!   número varia: `part1` e `part01` são ambos legais).
//! * **Antiga (RAR 2/3):** `foo.rar`, `foo.r00`, `foo.r01`, … — repare que o
//!   PRIMEIRO volume é `.rar` e os seguintes são `.rNN`, então a ordem não sai
//!   de um `sort()` ingênuo dos nomes.
//!
//! Nada disso é o "corte cru" `.001`/`.002` do `split.rs`: ali os volumes são um
//! arquivo picado com tesoura; aqui cada volume é um RAR completo com cabeçalho
//! próprio, e um membro pode começar num volume e terminar no outro.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rars::{Archive, ArchiveReadOptions, ArchiveReader, ExtractedEntryMeta};

use crate::archive::{norm_inner, safe_join, selected, AEntry, Reporter};

// ---------- descoberta de volumes ----------

/// `foo.part07.rar` → (`foo`, 7, 2). Só o padrão NOVO de volume.
fn part_suffix(path: &Path) -> Option<(PathBuf, usize, usize)> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_lowercase();
    let rest = lower.strip_suffix(".rar")?;
    let (stem, num) = rest.rsplit_once(".part")?;
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: usize = num.parse().ok()?;
    // Recorta do nome ORIGINAL (preserva maiúsculas do usuário).
    Some((path.with_file_name(&name[..stem.len()]), n, num.len()))
}

/// `foo.r00` → `foo.rar` (numeração antiga: o 1º volume é o `.rar`).
fn old_naming_head(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_lowercase();
    let (stem, num) = lower.rsplit_once(".r")?;
    if num.len() != 2 || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(path.with_file_name(format!("{}.rar", &name[..stem.len()])))
}

/// O nome é um volume RAR da numeração antiga (`foo.r00`)? Usado pela detecção
/// de formato: `.r07` não termina em `.rar`, mas é RAR do mesmo jeito.
pub fn is_old_volume_name(lower: &str) -> bool {
    match lower.rsplit_once(".r") {
        Some((stem, num)) => {
            !stem.is_empty() && num.len() == 2 && num.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// Os volumes de um conjunto RAR, EM ORDEM, a partir de qualquer um deles.
///
/// Um `.rar` que não é multivolume devolve um vetor de 1 — o chamador não
/// precisa saber a diferença.
pub fn volume_set(path: &Path) -> Vec<PathBuf> {
    // Entrou por um `.rNN`? Volta pra cabeça do conjunto e recomeça.
    if !path.to_string_lossy().to_lowercase().ends_with(".rar") {
        if let Some(head) = old_naming_head(path) {
            if head.is_file() {
                return volume_set(&head);
            }
        }
        return vec![path.to_path_buf()];
    }

    // Numeração nova: `foo.partN.rar`.
    if let Some((base, _, width)) = part_suffix(path) {
        let mut out = Vec::new();
        let mut i = 1usize;
        loop {
            let p = base.with_file_name(format!(
                "{}.part{:0width$}.rar",
                base.file_name().unwrap_or_default().to_string_lossy(),
                i
            ));
            if !p.is_file() {
                break;
            }
            out.push(p);
            i += 1;
        }
        if !out.is_empty() {
            return out;
        }
        return vec![path.to_path_buf()];
    }

    // Numeração antiga: `foo.rar` + `foo.r00`, `foo.r01`, …
    let stem = path.with_extension("");
    let stem_name = stem.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut out = vec![path.to_path_buf()];
    let mut i = 0usize;
    loop {
        let p = stem.with_file_name(format!("{stem_name}.r{i:02}"));
        if !p.is_file() {
            break;
        }
        out.push(p);
        i += 1;
    }
    out
}

// ---------- erros ----------

/// Desembrulha `AtEntry`/`AtArchiveOffset` até o erro de verdade.
fn root_cause(e: &rars::Error) -> &rars::Error {
    match e {
        rars::Error::AtEntry { source, .. } | rars::Error::AtArchiveOffset { source, .. } => {
            root_cause(source)
        }
        other => other,
    }
}

/// Traduz o erro do crate pros códigos que o front já entende.
fn classify(e: &rars::Error, password: Option<&str>) -> String {
    match root_cause(e) {
        rars::Error::NeedPassword => "NEED_PASSWORD".into(),
        // O crate não consegue distinguir senha errada de dado corrompido (a
        // checagem só falha no fim, no hash) — com senha na mão, o palpite útil
        // é "senha errada"; sem senha, é "faltou senha".
        rars::Error::WrongPasswordOrCorruptData => {
            if password.is_some() { "WRONG_PASSWORD".into() } else { "NEED_PASSWORD".into() }
        }
        rars::Error::Io(io) if io.message.contains("canceled") => "canceled".into(),
        other => other.to_string(),
    }
}

fn parse_volumes(vols: &[PathBuf], password: Option<&str>) -> Result<Vec<Archive>, String> {
    let opts = ArchiveReadOptions::with_optional_password(password.map(|p| p.as_bytes()));
    let mut out = Vec::with_capacity(vols.len());
    for v in vols {
        let a = ArchiveReader::read_path_with_options(v, opts)
            .map_err(|e| classify(&e, password))?;
        out.push(a);
    }
    Ok(out)
}

// ---------- listagem ----------

/// Índice do RAR (ou do conjunto de volumes) sem descompactar nada.
///
/// Um membro que atravessa a fronteira de volume aparece no cabeçalho de CADA
/// volume que ele toca; o `is_split_before` marca as continuações, e é por isso
/// que elas são puladas — senão um arquivo de 3 volumes viraria 3 entradas.
pub fn list(path: &str, password: Option<&str>) -> Result<Vec<AEntry>, String> {
    let vols = volume_set(Path::new(path));
    let archives = parse_volumes(&vols, password)?;
    let mut entries = Vec::new();
    for a in &archives {
        for m in a.members() {
            let meta = &m.meta;
            if meta.is_split_before {
                continue; // continuação do volume anterior, não é entrada nova
            }
            let inner = norm_inner(&meta.name_lossy());
            if inner.is_empty() {
                continue;
            }
            entries.push(AEntry {
                path: inner,
                is_dir: meta.is_directory,
                size: meta.unpacked_size,
                compressed: meta.packed_size,
                modified_ms: dos_time_ms(meta.file_time),
                encrypted: meta.is_encrypted,
            });
        }
    }
    Ok(entries)
}

/// Timestamp DOS/FAT (o que o RAR guarda) → epoch-ms.
fn dos_time_ms(t: Option<u32>) -> i64 {
    let Some(t) = t else { return 0 };
    if t == 0 {
        return 0;
    }
    let (y, mo, d) = (1980 + ((t >> 25) & 0x7f) as i64, ((t >> 21) & 0x0f) as i64, ((t >> 16) & 0x1f) as i64);
    let (h, mi, s) = (((t >> 11) & 0x1f) as i64, ((t >> 5) & 0x3f) as i64, ((t & 0x1f) * 2) as i64);
    if mo == 0 || d == 0 {
        return 0;
    }
    let a = (14 - mo) / 12;
    let y2 = y + 4800 - a;
    let m2 = mo + 12 * a - 3;
    let jdn = d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045;
    ((jdn - 2440588) * 86400 + h * 3600 + mi * 60 + s) * 1000
}

// ---------- extração ----------

/// Escritor que conta bytes e obedece o cancelamento.
///
/// Tem que ser `'static` (o `rars` pede `Box<dyn Write>`), então DONO de tudo:
/// o `Reporter` vem num `Arc<Mutex<…>>` e a flag de cancelar num `Arc`.
struct CountingWriter {
    inner: Box<dyn Write>,
    rep: Arc<Mutex<Reporter>>,
    cancel: Arc<AtomicBool>,
    name: String,
    bytes: Arc<AtomicU64>,
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "canceled"));
        }
        let n = self.inner.write(buf)?;
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        if let Ok(mut r) = self.rep.lock() {
            r.bytes(n as u64, &self.name);
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Extrai tudo (`filter` = `None`) ou só a seleção.
pub fn extract(
    app: Option<&tauri::AppHandle>,
    op_id: u64,
    cancel: &Arc<AtomicBool>,
    archive: &str,
    dest_dir: &Path,
    filter: &Option<Vec<String>>,
    password: Option<&str>,
) -> Result<(), String> {
    let vols = volume_set(Path::new(archive));
    let archives = parse_volumes(&vols, password)?;

    // Totais honestos do que foi selecionado (mesma regra da listagem).
    let (mut total_files, mut total_bytes) = (0u64, 0u64);
    for a in &archives {
        for m in a.members() {
            let meta = &m.meta;
            if meta.is_split_before || meta.is_directory {
                continue;
            }
            let inner = norm_inner(&meta.name_lossy());
            if !inner.is_empty() && selected(&inner, filter) {
                total_files += 1;
                total_bytes += meta.unpacked_size;
            }
        }
    }

    let rep = Arc::new(Mutex::new(Reporter::new(app.cloned(), op_id, total_files, total_bytes)));
    let counted = Arc::new(AtomicU64::new(0));
    // Erro do NOSSO lado (criar pasta, zip-slip): o `rars` só deixa devolver
    // `rars::Error`, então o motivo real fica guardado aqui.
    let own_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let open = |meta: &ExtractedEntryMeta| -> rars::Result<Box<dyn Write>> {
        if cancel.load(Ordering::Relaxed) {
            return Err(rars::Error::from(io::Error::new(
                io::ErrorKind::Interrupted,
                "canceled",
            )));
        }
        // Fecha o arquivo anterior no contador de arquivos.
        let inner = norm_inner(&meta.name_lossy());
        if inner.is_empty() || !selected(&inner, filter) {
            // Não selecionado: o fluxo PRECISA ser consumido mesmo assim (o RAR
            // é sequencial, e sólido ainda por cima), então vai pro ralo.
            return Ok(Box::new(io::sink()));
        }
        let target = match safe_join(dest_dir, &inner) {
            Ok(t) => t,
            Err(e) => {
                *own_err.lock().unwrap() = Some(e);
                return Err(rars::Error::InvalidHeader("caminho suspeito no arquivo"));
            }
        };
        let mk = |e: std::io::Error| {
            *own_err.lock().unwrap() = Some(format!("{}: {e}", target.display()));
            rars::Error::from(e)
        };
        if meta.is_directory {
            fs::create_dir_all(&target).map_err(mk)?;
            return Ok(Box::new(io::sink()));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(mk)?;
        }
        let f = fs::File::create(&target).map_err(mk)?;
        if let Ok(mut r) = rep.lock() {
            r.file_done(&inner);
        }
        Ok(Box::new(CountingWriter {
            inner: Box::new(io::BufWriter::new(f)),
            rep: rep.clone(),
            cancel: cancel.clone(),
            name: inner,
            bytes: counted.clone(),
        }))
    };

    let pw = password.map(|p| p.as_bytes());
    let r = if archives.len() > 1 {
        rars::extract_volumes_to(&archives, pw, open)
    } else {
        archives[0].extract_to(pw, open)
    };

    if let Err(e) = r {
        if cancel.load(Ordering::Relaxed) {
            return Err("canceled".into());
        }
        if let Some(own) = own_err.lock().unwrap().take() {
            return Err(own);
        }
        return Err(classify(&e, password));
    }
    Ok(())
}

/// Lê TUDO pro ralo: o `rars` confere CRC-32/BLAKE2sp de cada membro na
/// descompressão, então "leu até o fim sem erro" = íntegro.
pub fn test_integrity(archive: &str, password: Option<&str>) -> Result<u64, (String, String)> {
    let vols = volume_set(Path::new(archive));
    let archives = parse_volumes(&vols, password).map_err(|e| (String::new(), e))?;
    let mut tested = 0u64;
    for a in &archives {
        for m in a.members() {
            if !m.meta.is_split_before && !m.meta.is_directory {
                tested += 1;
            }
        }
    }
    let pw = password.map(|p| p.as_bytes());
    let open = |_: &ExtractedEntryMeta| -> rars::Result<Box<dyn Write>> { Ok(Box::new(io::sink())) };
    let r = if archives.len() > 1 {
        rars::extract_volumes_to(&archives, pw, open)
    } else {
        archives[0].extract_to(pw, open)
    };
    match r {
        Ok(()) => Ok(tested),
        Err(e) => {
            let name = match &e {
                rars::Error::AtEntry { name, .. } => String::from_utf8_lossy(name).into_owned(),
                _ => String::new(),
            };
            Err((name, classify(&e, password)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("localzip-rar-{name}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn extrai(fx: &str, pw: Option<&str>, dest: &Path) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        extract(None, 1, &cancel, fixture(fx).to_str().unwrap(), dest, &None, pw)
    }

    #[test]
    fn rar5_stored_e_compactado_extraem() {
        // Sem compressão e com o método 3 (o padrão do WinRAR) — o CRC gravado
        // pelo WinRAR é quem diz se a descompressão acertou.
        for fx in ["rar5_stored.rar", "rar5_compactado.rar"] {
            let d = tmp(fx);
            extrai(fx, None, &d).unwrap_or_else(|e| panic!("{fx}: {e}"));
            let achados: Vec<_> = fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).collect();
            assert!(!achados.is_empty(), "{fx} não gerou arquivo");
            let itens = list(fixture(fx).to_str().unwrap(), None).unwrap();
            assert!(!itens.is_empty(), "{fx} listou vazio");
            // O tamanho no disco tem que bater com o do cabeçalho.
            for e in itens.iter().filter(|e| !e.is_dir) {
                let f = d.join(&e.path);
                assert_eq!(fs::metadata(&f).unwrap().len(), e.size, "{fx} → {}", e.path);
            }
            let _ = fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn rar5_varios_membros() {
        let itens = list(fixture("rar5_varios.rar").to_str().unwrap(), None).unwrap();
        assert!(itens.iter().filter(|e| !e.is_dir).count() >= 3, "{itens:?}", itens = itens.len());
        let d = tmp("varios");
        extrai("rar5_varios.rar", None, &d).unwrap();
        for e in itens.iter().filter(|e| !e.is_dir) {
            assert_eq!(fs::metadata(d.join(&e.path)).unwrap().len(), e.size);
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rar5_com_senha() {
        let fx = "rar5_senha.rar";
        // Senha certa extrai.
        let d = tmp("senha-ok");
        extrai(fx, Some("password"), &d).unwrap_or_else(|e| panic!("senha certa: {e}"));
        assert!(fs::read_dir(&d).unwrap().next().is_some());
        // Sem senha / senha errada viram códigos claros pro front.
        let d2 = tmp("senha-err");
        let sem = extrai(fx, None, &d2).unwrap_err();
        assert_eq!(sem, "NEED_PASSWORD", "sem senha");
        let errada = extrai(fx, Some("naoehessa"), &d2).unwrap_err();
        assert_eq!(errada, "WRONG_PASSWORD", "senha errada");
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }

    #[test]
    fn rar4_compactado_extrai() {
        // RAR 3.x (Unpack29) — implementação completamente diferente do RAR5.
        let d = tmp("rar4");
        extrai("rar4_compactado.rar", None, &d).unwrap();
        let itens = list(fixture("rar4_compactado.rar").to_str().unwrap(), None).unwrap();
        for e in itens.iter().filter(|e| !e.is_dir) {
            assert_eq!(fs::metadata(d.join(&e.path)).unwrap().len(), e.size);
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn volume_set_acha_as_duas_numeracoes() {
        // Nova (`part1..part3`): entra por QUALQUER volume, sai na ordem.
        let v = volume_set(&fixture("rar5_volumes.part2.rar"));
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v[0].to_string_lossy().ends_with("part1.rar"));
        assert!(v[2].to_string_lossy().ends_with("part3.rar"));

        // Antiga (`.rar` + `.r00`): o `.rar` é o PRIMEIRO, não o último.
        let v = volume_set(&fixture("rar4_volumes.rar"));
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].to_string_lossy().ends_with("rar4_volumes.rar"));
        assert!(v[1].to_string_lossy().ends_with(".r00"));
        // Entrar pelo `.r00` acha o mesmo conjunto.
        assert_eq!(volume_set(&fixture("rar4_volumes.r00")), v);

        // RAR de um volume só continua sendo um vetor de 1.
        assert_eq!(volume_set(&fixture("rar5_stored.rar")).len(), 1);
    }

    #[test]
    fn multivolume_costura_membro_partido() {
        // O ponto do teste: um membro que ATRAVESSA volumes tem que sair
        // inteiro, e aparecer UMA vez na listagem (não uma por volume).
        for (fx, n) in [("rar5_volumes.part1.rar", 3), ("rar4_volumes.rar", 2)] {
            let vols = volume_set(&fixture(fx));
            assert_eq!(vols.len(), n, "{fx}");
            let itens = list(fixture(fx).to_str().unwrap(), None).unwrap();
            let arquivos: Vec<_> = itens.iter().filter(|e| !e.is_dir).collect();
            assert!(!arquivos.is_empty(), "{fx} listou vazio");
            let nomes: std::collections::HashSet<_> =
                arquivos.iter().map(|e| e.path.clone()).collect();
            assert_eq!(nomes.len(), arquivos.len(), "{fx}: membro partido duplicou na lista");

            let d = tmp(&format!("mv-{n}"));
            extrai(fx, None, &d).unwrap_or_else(|e| panic!("{fx}: {e}"));
            for e in arquivos {
                let f = d.join(&e.path);
                assert_eq!(fs::metadata(&f).unwrap().len(), e.size, "{fx} → {}", e.path);
            }
            let _ = fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn integridade_confere_e_senha_errada_falha() {
        let ok = test_integrity(fixture("rar5_compactado.rar").to_str().unwrap(), None);
        assert!(ok.is_ok(), "{ok:?}");
        let bad = test_integrity(fixture("rar5_senha.rar").to_str().unwrap(), Some("errada"));
        assert!(bad.is_err(), "senha errada deveria falhar");
    }
}
