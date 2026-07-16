import { t } from "../lib/i18n";
import { useUi } from "../state/ui";
import { pickAndOpen } from "./TopBar";

/** Tela inicial sem arquivo aberto: abrir, criar ou arrastar. */
export default function EmptyState() {
  const setCreateSources = useUi((s) => s.setCreateSources);

  return (
    <div className="empty-state">
      <div className="empty-icon">🗜️</div>
      <h1>{t("empty.title")}</h1>
      <p className="muted">{t("empty.sub")}</p>
      <div className="empty-actions">
        <button className="primary" onClick={() => void pickAndOpen()}>
          {t("empty.open")}
        </button>
        <button onClick={() => setCreateSources([])}>{t("empty.create")}</button>
      </div>
      <p className="muted small">{t("empty.formats")}</p>
    </div>
  );
}
