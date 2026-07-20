import type { AEntry, VNode } from "./types";

/**
 * Visão de pasta DENTRO do arquivo: a partir da lista achatada de entradas
 * ("a/b/c.txt"), calcula os filhos diretos de um diretório interno — inclui
 * pastas implícitas (zip nem sempre tem entrada própria pra pasta) e agrega
 * tamanho/contagem nas pastas.
 */
export function childrenOf(entries: AEntry[], dir: string): VNode[] {
  const prefix = dir === "" ? "" : dir + "/";
  const byName = new Map<string, VNode>();

  for (const e of entries) {
    if (!e.path.startsWith(prefix) || e.path === dir) continue;
    const rest = e.path.slice(prefix.length);
    if (rest === "") continue;
    const slash = rest.indexOf("/");
    const isDirect = slash < 0;
    const name = isDirect ? rest : rest.slice(0, slash);
    const full = prefix + name;

    const existing = byName.get(name);
    if (isDirect && !e.isDir) {
      // Arquivo direto deste nível.
      byName.set(name, {
        name,
        path: full,
        isDir: false,
        size: e.size,
        compressed: e.compressed,
        modifiedMs: e.modifiedMs,
        encrypted: e.encrypted,
        children: 0,
      });
    } else {
      // Pasta (explícita ou implícita): agrega.
      const node: VNode = existing ?? {
        name,
        path: full,
        isDir: true,
        size: 0,
        compressed: 0,
        modifiedMs: 0,
        encrypted: false,
        children: 0,
      };
      if (!node.isDir) continue; // arquivo e pasta com o mesmo nome: arquivo ganha
      if (!isDirect && !e.isDir) {
        node.size += e.size;
        node.compressed += e.compressed;
      }
      // Filho direto da pasta = resto com exatamente 1 nível a mais.
      if (!isDirect) {
        const deeper = rest.slice(slash + 1);
        if (!deeper.includes("/") && deeper !== "") node.children += 1;
      }
      if (e.modifiedMs > node.modifiedMs) node.modifiedMs = e.modifiedMs;
      byName.set(name, node);
    }
  }

  const out = [...byName.values()];
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  out.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return collator.compare(a.name, b.name);
  });
  return out;
}

/** Segmentos do breadcrumb interno: [{name, path}] (raiz = path ""). */
export function crumbsOf(dir: string): { name: string; path: string }[] {
  if (dir === "") return [];
  const parts = dir.split("/");
  const out: { name: string; path: string }[] = [];
  let acc = "";
  for (const part of parts) {
    acc = acc === "" ? part : `${acc}/${part}`;
    out.push({ name: part, path: acc });
  }
  return out;
}

export function parentDir(dir: string): string {
  const idx = dir.lastIndexOf("/");
  return idx < 0 ? "" : dir.slice(0, idx);
}

/** Bytes legíveis (unidades binárias, 1 casa). */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${units[u]}`;
}

export function formatDate(ms: number, localeTag: string): string {
  if (!ms) return "—";
  return new Intl.DateTimeFormat(localeTag, { dateStyle: "short", timeStyle: "short" }).format(
    new Date(ms),
  );
}

/**
 * O caminho parece um arquivo compactado que a gente abre?
 *
 * Cuidado com as três famílias de "arquivo dividido", que são diferentes:
 * `foo.zip.001` (corte cru, qualquer formato), `foo.part2.rar` (multivolume do
 * próprio RAR) e `foo.r07` (a numeração antiga do RAR, que nem termina em
 * `.rar`). O `.z01` do zip multi-disco entra de propósito: é melhor abrir e
 * explicar por que não dá do que fingir que o arquivo não existe.
 */
export function isSupportedArchive(path: string): boolean {
  let l = path.toLowerCase();
  // Sufixo de volume de corte cru (3+ dígitos) não muda o formato: tira e segue.
  l = l.replace(/\.\d{3,}$/, "");
  return (
    l.endsWith(".zip") ||
    l.endsWith(".z01") ||
    l.endsWith(".rar") ||
    /\.r\d{2}$/.test(l) ||
    l.endsWith(".7z") ||
    l.endsWith(".tar") ||
    /\.(tar\.gz|tgz|tar\.xz|txz|tar\.bz2|tbz2|tbz|tar\.zst|tzst)$/.test(l)
  );
}
