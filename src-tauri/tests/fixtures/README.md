# Fixtures de RAR (arquivos de teste)

Estes `.rar` são **arquivos de verdade, gerados pelo WinRAR original** — não foram
produzidos pelo crate que estamos testando. Isso é de propósito: se o fixture viesse
do mesmo código que lê, um bug na leitura e um bug na escrita se cancelariam e o
teste passaria mentindo.

Origem: corpus de fixtures do projeto [`rars`](https://github.com/bitplane/rars)
(`tests/fixtures/`, versão 0.4.4), licenciado **MIT OR Apache-2.0**. O corpus do
`rars` por sua vez vem do repo de pesquisa de formato
[`rar-research`](https://github.com/bitplane/rar-research).

| Arquivo aqui | Origem no `rars` | O que exercita |
|---|---|---|
| `rar5_stored.rar` | `rar50/stored.rar` | RAR5 sem compressão |
| `rar5_compactado.rar` | `rar50/m3_default.rar` | RAR5 método 3 (padrão do WinRAR), 64 KB |
| `rar5_varios.rar` | `rar50/multifile.rar` | RAR5 com 3 membros |
| `rar5_senha.rar` | `rar50/password_aes.rar` | RAR5 AES (senha `password`) |
| `rar5_volumes.part{1,2,3}.rar` | `rar50/multivol.part{1,2,3}.rar` | RAR5 dividido em 3 volumes |
| `rar4_compactado.rar` | `rar15_40/rar300/compressed_text_rar300.rar` | RAR 3.x, Unpack29 |
| `rar4_volumes.rar` + `.r00` | `rar15_40/rar300/multivol_oldnaming_rar300.*` | RAR 3.x volumes na numeração antiga (`.r00`) |

**Como o teste sabe que a extração está certa:** cada membro carrega o CRC-32 (ou
BLAKE2sp) que o *WinRAR* calculou do arquivo original. Se a descompressão errar um
bit, o CRC não fecha. O `rars` valida isso internamente e devolve erro — os testes
conferem tanto o conteúdo esperado quanto essa validação.

Fixtures de zip/7z/tar não moram aqui: os testes os **geram** na hora (o formato de
escrita é nosso ou de crate já exercitado na ida e na volta).
