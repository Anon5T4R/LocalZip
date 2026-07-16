import { useMemo } from "react";
import { formatBytes, formatDate, childrenOf } from "../lib/zpath";
import { localeTag, t } from "../lib/i18n";
import type { VNode } from "../lib/types";
import { useZip } from "../state/store";

/**
 * Tabela do conteúdo do arquivo na pasta interna atual: duplo clique navega
 * na pasta; seleção com clique/Ctrl/Shift; extração usa a seleção.
 */
export default function EntryTable() {
  const info = useZip((s) => s.info)!;
  const dir = useZip((s) => s.dir);
  const selection = useZip((s) => s.selection);
  const anchor = useZip((s) => s.anchor);
  const { setDir, setSelection } = useZip.getState();

  const nodes = useMemo(() => childrenOf(info.entries, dir), [info, dir]);
  const selected = new Set(selection);
  const files = info.entries.filter((e) => !e.isDir).length;
  const packed = formatBytes(info.archiveBytes);

  const click = (e: React.MouseEvent, node: VNode, index: number) => {
    e.stopPropagation();
    if (e.shiftKey && anchor !== null) {
      const [a, b] = [Math.min(anchor, index), Math.max(anchor, index)];
      setSelection(nodes.slice(a, b + 1).map((n) => n.path));
    } else if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected);
      if (next.has(node.path)) next.delete(node.path);
      else next.add(node.path);
      setSelection([...next], index);
    } else {
      setSelection([node.path], index);
    }
  };

  return (
    <div className="table-wrap" onClick={() => setSelection([], null)}>
      {info.bombSuspect && <div className="banner warn">{t("info.bomb")}</div>}
      {info.entries.some((e) => e.encrypted) && (
        <div className="banner info">{t("info.encrypted")}</div>
      )}

      <div className="table-head">
        <div className="cell name">{t("col.name")}</div>
        <div className="cell size">{t("col.size")}</div>
        <div className="cell size">{t("col.packed")}</div>
        <div className="cell date">{t("col.modified")}</div>
      </div>

      <div className="table-body">
        {nodes.length === 0 && <div className="table-empty">{t("list.empty")}</div>}
        {nodes.map((node, i) => (
          <div
            key={node.path}
            className={`row ${selected.has(node.path) ? "selected" : ""}`}
            onClick={(e) => click(e, node, i)}
            onDoubleClick={() => {
              if (node.isDir) setDir(node.path);
            }}
            title={node.path}
          >
            <div className="cell name">
              <span className="entry-icon">{node.isDir ? "📁" : "📄"}</span>
              <span className="entry-name">{node.name}</span>
              {node.encrypted && (
                <span className="prot-badge" title={t("table.protected")}>
                  🔒
                </span>
              )}
            </div>
            <div className="cell size">
              {node.isDir && node.size === 0 ? "—" : formatBytes(node.size)}
            </div>
            <div className="cell size">
              {info.format === "zip" && node.compressed > 0 ? formatBytes(node.compressed) : "—"}
            </div>
            <div className="cell date">{formatDate(node.modifiedMs, localeTag())}</div>
          </div>
        ))}
      </div>

      <div className="table-foot">
        <span>{t("table.items", { n: nodes.length })}</span>
        <span className="foot-fill" />
        <span>
          {t("info.summary", {
            files,
            size: formatBytes(info.totalSize),
            packed,
          })}
        </span>
      </div>
    </div>
  );
}
