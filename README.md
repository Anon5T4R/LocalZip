# LocalZip

Compactador de arquivos **100% offline** da suíte Local — o utilitário
universal de `.zip`/`.tar.gz` que faltava (7-Zip/WinRAR local, sem instalador
de terceiro).

## Recursos

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

**Roadmap:** v0.3 = 7z, extração de RAR, volumes divididos, adicionar/remover
sem re-extrair, integração com o LocalFiles.

## Stack

Tauri 2 + React 19 + Vite + TypeScript no front; Rust no back (`zip`, `tar`,
`flate2` — streaming com progresso, tudo no Rust). Sem sidecar, sem rede.

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
