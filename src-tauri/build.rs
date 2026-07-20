fn main() {
    // O binário de teste do cargo não ganha manifesto Win32 (só o executável do
    // app ganha, pelo `tauri_build`). Sem manifesto o loader liga o comctl32
    // **v5**, que não exporta `TaskDialogIndirect` — e o `rfd` (dependência do
    // tauri-plugin-dialog) importa essa função. Resultado: o exe de teste nem
    // carrega, morre com STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139) antes de
    // rodar um único teste, sem imprimir nada.
    //
    // Isso é uma armadilha LATENTE, não um bug novo: a importação só aparece
    // quando o linker para de descartar o caminho de diálogo do `rfd`, e isso
    // muda sozinho conforme o crate cresce. Pedir a dependência de manifesto
    // aqui deixa o exe de teste igual ao do app.
    #[cfg(windows)]
    {
        // `-tests` NÃO alcança o harness de teste da lib (só alvos `[[test]]`
        // de integração) — e é justamente o da lib que quebra. Então vale pra
        // todo mundo…
        println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
        // …menos no exe do app, onde o `tauri_build` já embute um manifesto
        // pelo `resource.lib`: dois manifestos fazem o CVTRES abortar com
        // "recurso duplicado" (CVT1100 → LNK1123). Este vem depois e vence.
        println!("cargo::rustc-link-arg-bins=/MANIFEST:NO");
    }
    tauri_build::build()
}
