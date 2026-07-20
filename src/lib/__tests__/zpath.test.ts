import { describe, expect, it } from "vitest";
import { childrenOf, crumbsOf, formatBytes, isSupportedArchive, parentDir } from "../zpath";
import type { AEntry } from "../types";

function e(path: string, opts?: Partial<AEntry>): AEntry {
  return {
    path,
    isDir: false,
    size: 10,
    compressed: 5,
    modifiedMs: 1000,
    encrypted: false,
    ...opts,
  };
}

describe("childrenOf", () => {
  const entries = [
    e("readme.md"),
    e("docs/a.txt"),
    e("docs/sub/b.txt", { size: 30 }),
    e("docs", { isDir: true, size: 0 }),
    e("img/logo.png", { size: 100 }),
  ];

  it("raiz: pastas primeiro, arquivo direto e pastas implícitas", () => {
    const out = childrenOf(entries, "");
    expect(out.map((n) => n.name)).toEqual(["docs", "img", "readme.md"]);
    const docs = out[0];
    expect(docs.isDir).toBe(true);
    expect(docs.size).toBe(40); // a.txt(10) + sub/b.txt(30)
    const img = out[1];
    expect(img.isDir).toBe(true); // implícita (só existe img/logo.png)
    expect(img.size).toBe(100);
  });

  it("dentro de docs: arquivo direto + subpasta", () => {
    const out = childrenOf(entries, "docs");
    expect(out.map((n) => n.name)).toEqual(["sub", "a.txt"]);
    expect(out[0].size).toBe(30);
  });

});

describe("crumbsOf / parentDir", () => {
  it("segmentos acumulados", () => {
    expect(crumbsOf("")).toEqual([]);
    expect(crumbsOf("a/b/c")).toEqual([
      { name: "a", path: "a" },
      { name: "b", path: "a/b" },
      { name: "c", path: "a/b/c" },
    ]);
  });

  it("pai interno", () => {
    expect(parentDir("a/b/c")).toBe("a/b");
    expect(parentDir("a")).toBe("");
  });
});

describe("isSupportedArchive / formatBytes", () => {
  // Este teste ficou PARA TRÁS da implementação: afirmava `.7z === false`, mas o
  // 7z entrou na v0.3 (extração via `sevenz-rust2`, puro Rust) e os alvos
  // xz/bz2/zst entraram na v0.2 — nenhum deles tinha cobertura. O CI ficou
  // vermelho desde então. Feature nova sem teste atualizado é teste que vira
  // alarme falso e depois é ignorado.
  it("extensões suportadas (v0.1 zip/tar, v0.2 xz/bz2/zst, v0.3 7z)", () => {
    expect(isSupportedArchive("x.ZIP")).toBe(true);
    expect(isSupportedArchive("x.tar")).toBe(true);
    expect(isSupportedArchive("x.tar.gz")).toBe(true);
    expect(isSupportedArchive("x.tgz")).toBe(true);
    expect(isSupportedArchive("x.tar.xz")).toBe(true);
    expect(isSupportedArchive("x.txz")).toBe(true);
    expect(isSupportedArchive("x.tar.bz2")).toBe(true);
    expect(isSupportedArchive("x.tbz2")).toBe(true);
    expect(isSupportedArchive("x.tbz")).toBe(true);
    expect(isSupportedArchive("x.tar.zst")).toBe(true);
    expect(isSupportedArchive("x.tzst")).toBe(true);
    expect(isSupportedArchive("x.7z")).toBe(true);
  });

  it("rar: extensão normal e as duas numerações de volume", () => {
    // Extrair RAR passou a existir (crate puro-Rust, MIT/Apache); CRIAR RAR
    // continua fora de escopo — é formato proprietário.
    expect(isSupportedArchive("x.rar")).toBe(true);
    expect(isSupportedArchive("x.part1.rar")).toBe(true);
    // Numeração antiga: o volume nem termina em ".rar".
    expect(isSupportedArchive("x.r00")).toBe(true);
    expect(isSupportedArchive("x.r14")).toBe(true);
    expect(isSupportedArchive("x.r1")).toBe(false); // 1 dígito não é volume
  });

  it("volumes de corte cru (.001) herdam o formato do nome de baixo", () => {
    expect(isSupportedArchive("x.zip.001")).toBe(true);
    expect(isSupportedArchive("x.7z.002")).toBe(true);
    expect(isSupportedArchive("x.tar.gz.017")).toBe(true);
    // O sufixo sozinho não salva um formato que não abrimos.
    expect(isSupportedArchive("x.txt.001")).toBe(false);
    expect(isSupportedArchive("x.zip.01")).toBe(false); // 2 dígitos não é volume
  });

  it("zip multi-disco abre pra poder EXPLICAR que não dá", () => {
    // Melhor abrir e mostrar a mensagem certa do que fingir que não é arquivo.
    expect(isSupportedArchive("x.z01")).toBe(true);
  });

  it("NÃO suportadas", () => {
    expect(isSupportedArchive("x.txt")).toBe(false);
    expect(isSupportedArchive("x.gz")).toBe(false); // .gz solto não é arquivo-contêiner
  });

  it("bytes legíveis", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
});
