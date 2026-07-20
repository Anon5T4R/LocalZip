# LocalZip

Compactador de arquivos **100% offline** da suíte Local — o utilitário
universal de `.zip`/`.tar.gz` que faltava (7-Zip/WinRAR local, sem instalador
de terceiro).

## Recursos

**v0.5**
- **Extrair RAR** (RAR 1.5–4.x e RAR5, com senha, multivolume nas duas
  numerações: `.part1.rar` e `.rar`+`.r00`) — via crate **puro Rust**, sem
  `unrar.dll` nem binário de terceiro. **Criar RAR está fora de escopo**: o
  compactador é proprietário da WinRAR GmbH.
- **Volumes divididos** (`foo.zip.001`, `foo.7z.002`, …): abre, lista e extrai
  direto dos volumes, **sem emendar nada em disco** — os N arquivos são
  apresentados ao leitor como um fluxo `Read + Seek` só. Vale pra zip, 7z e tar.
- **Zip multi-disco de verdade** (`.z01`/`.z02`) é detectado e recusado **com
  explicação**, em vez do "invalid Zip archive" do crate. É outro formato: os
  deslocamentos do diretório central são relativos ao disco, então emendar os
  volumes não resolveria.
- **Adicionar e remover num zip existente sem re-extrair**. Adicionar usa
  acréscimo no fim (os bytes antigos não são nem lidos); remover reconstrói
  copiando **o fluxo já comprimido** de cada sobrevivente. Medido num zip de
  32 MB: re-extrair+recompactar 1122 ms · adicionar **1,9 ms** · remover
  **4,1 ms**.

**v0.2**
- **Mais formatos**: além de zip/tar/tar.gz, agora abre e extrai **`tar.xz`,
  `tar.bz2` e `tar.zst`**
- **Zip com senha (AES-256)**: extrai zips protegidos (pede a senha) e
  **cria** zips cifrados
- **Testar integridade**: lê tudo e valida o CRC de cada item (detecta
  corrupção/truncamento) — botão "Testar"

**v0.1**
- **Abrir e navegar** o conteúdo de `zip`, `tar` e `tar.gz/tgz` **sem extrair**
  (lê só o índice; pastas implícitas aparecem certinhas)
- **Extrair tudo ou só a seleção** pra pasta escolhida, com **progresso e
  cancelamento** — e abre a pasta no fim
- **Criar** `.zip` ou `.tar.gz` de arquivos e pastas (diálogo ou drag-and-drop)
- **Drag-and-drop**: soltar um arquivo suportado abre; soltar qualquer outra
  coisa vira origem de um arquivo novo
- **Segurança:** extração **sempre sanitizada contra zip-slip** (nada escapa do
  destino), links ignorados, **aviso de possível zip bomb** (razão de expansão
  suspeita) e aviso de itens protegidos por senha
- Tema claro/escuro/sistema · UI em **PT/EN/ES**

**Roadmap:** integração com o LocalFiles (zip inline).

## Stack

Tauri 2 + React 19 + Vite + TypeScript no front; Rust no back (`zip`, `tar`,
`flate2`, `sevenz-rust2`, `rars` — streaming com progresso, tudo no Rust). Sem
sidecar, sem rede, sem binário de terceiro: todas as dependências são crates
compilados pelo cargo.

Os `.rar` de teste em `src-tauri/tests/fixtures/` vieram do **WinRAR de
verdade** (via o corpus MIT/Apache do projeto `rars`), não do crate que a gente
testa — se o fixture saísse do mesmo código que lê, um bug na leitura e um na
escrita se cancelariam e o teste passaria mentindo. Detalhes no README de lá.

## Dev

```bash
npm install
npm run tauri dev   # porta 1460
```

Testes: `npm test` (front) e `cargo test` em `src-tauri/` (CI).

## Release

Tag `vX.Y.Z` → GitHub Actions builda NSIS (Windows) + AppImage (Linux) e
publica a Release. Parte da suíte [Local](https://github.com/Anon5T4R).

## Licença

MIT
