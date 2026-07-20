//! Guarda dos fixtures de RAR.
//!
//! Os `.rar` em `tests/fixtures/` vieram do WinRAR de verdade (ver o README de
//! lá) — se algum sumir ou for trocado por um arquivo gerado pelo crate que a
//! gente testa, os testes de extração passariam mentindo. Aqui só se confere
//! que os arquivos estão no lugar e têm a assinatura da família que prometem.
//!
//! Este alvo de integração também é o que torna legal o
//! `cargo::rustc-link-arg-tests` do `build.rs` (sem nenhum `[[test]]`, o cargo
//! recusa a instrução) — ver o comentário lá.

use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// RAR 1.5–4.x: `Rar!\x1a\x07\x00`. RAR 5+: `Rar!\x1a\x07\x01\x00`.
const RAR4: &[u8] = b"Rar!\x1a\x07\x00";
const RAR5: &[u8] = b"Rar!\x1a\x07\x01\x00";

#[test]
fn fixtures_de_rar_existem_e_sao_da_familia_certa() {
    let esperado: &[(&str, &[u8])] = &[
        ("rar5_stored.rar", RAR5),
        ("rar5_compactado.rar", RAR5),
        ("rar5_varios.rar", RAR5),
        ("rar5_senha.rar", RAR5),
        ("rar5_volumes.part1.rar", RAR5),
        ("rar5_volumes.part2.rar", RAR5),
        ("rar5_volumes.part3.rar", RAR5),
        ("rar4_compactado.rar", RAR4),
        ("rar4_volumes.rar", RAR4),
        ("rar4_volumes.r00", RAR4),
    ];
    for (nome, assinatura) in esperado {
        let p = fixture(nome);
        let bytes = std::fs::read(&p)
            .unwrap_or_else(|e| panic!("fixture sumiu: {} ({e})", p.display()));
        assert!(
            bytes.starts_with(assinatura),
            "{nome}: assinatura não bate — o fixture foi trocado?"
        );
    }
}
