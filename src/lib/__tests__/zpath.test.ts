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

  it("NÃO suportadas", () => {
    // RAR é decisão de escopo, não esquecimento: criar RAR está fora (formato
    // proprietário) e extrair depende de crate puro-Rust, ainda não avaliado.
    expect(isSupportedArchive("x.rar")).toBe(false);
    expect(isSupportedArchive("x.txt")).toBe(false);
    expect(isSupportedArchive("x.gz")).toBe(false); // .gz solto não é arquivo-contêiner
  });

  it("bytes legíveis", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
});
