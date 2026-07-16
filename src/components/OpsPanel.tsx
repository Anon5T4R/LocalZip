import { cancelOp } from "../lib/backend";
import { formatBytes } from "../lib/zpath";
import { t } from "../lib/i18n";
import { useZip } from "../state/store";

/** Painel flutuante das operações em andamento (progresso + cancelar). */
export default function OpsPanel() {
  const ops = useZip((s) => s.ops);
  if (ops.length === 0) return null;

  return (
    <div className="ops-panel">
      {ops.map((op) => {
        const p = op.progress;
        const pct =
          p && p.totalBytes > 0
            ? Math.min(100, Math.round((p.doneBytes / p.totalBytes) * 100))
            : null;
        return (
          <div key={op.opId} className="op">
            <div className="op-line">
              <span className="op-text">
                {t(op.kind === "extract" ? "ops.extracting" : "ops.creating", {
                  done: p ? formatBytes(p.doneBytes) : "…",
                  total: p ? formatBytes(p.totalBytes) : "…",
                })}
              </span>
              <button className="op-cancel" onClick={() => void cancelOp(op.opId)}>
                {t("ops.cancel")}
              </button>
            </div>
            {p && (
              <div className="op-sub">
                {t("ops.files", { done: String(p.doneFiles), total: String(p.totalFiles) })}
                {p.current ? ` — ${p.current}` : ""}
              </div>
            )}
            <div className="op-bar">
              <div
                className={`op-fill ${pct === null ? "indeterminate" : ""}`}
                style={pct === null ? undefined : { width: `${pct}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
