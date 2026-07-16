import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../lib/i18n";
import { useUi } from "../state/ui";
import { saveAndCreate } from "./TopBar";

/**
 * Criar arquivo compactado: monta a lista de origens (arquivos/pastas ou
 * drag-and-drop, que o App injeta aqui) e escolhe o formato; o destino é
 * perguntado no "Criar…" (diálogo salvar).
 */
export default function CreateModal() {
  const sources = useUi((s) => s.createSources);
  const setCreateSources = useUi((s) => s.setCreateSources);
  const [format, setFormat] = useState<"zip" | "targz">("zip");

  if (sources === null) return null;

  const add = (paths: string[]) => {
    const next = [...sources];
    for (const p of paths) if (!next.includes(p)) next.push(p);
    setCreateSources(next);
  };

  const addFiles = async () => {
    const picked = await open({ multiple: true });
    if (Array.isArray(picked)) add(picked);
    else if (typeof picked === "string") add([picked]);
  };

  const addFolder = async () => {
    const picked = await open({ directory: true });
    if (typeof picked === "string") add([picked]);
  };

  return (
    <div className="modal-backdrop" onClick={() => setCreateSources(null)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{t("create.title")}</h2>

        <div className="create-row">
          <span className="muted">{t("create.sources")}</span>
          <div className="create-add">
            <button onClick={() => void addFiles()}>{t("create.addFiles")}</button>
            <button onClick={() => void addFolder()}>{t("create.addFolder")}</button>
          </div>
        </div>

        <div className="create-list">
          {sources.length === 0 && <div className="muted small">{t("create.empty")}</div>}
          {sources.map((s) => (
            <div key={s} className="create-item">
              <span className="create-path" title={s}>
                {s.split(/[\\/]/).pop()}
              </span>
              <button
                className="create-remove"
                onClick={() => setCreateSources(sources.filter((x) => x !== s))}
              >
                {t("create.remove")}
              </button>
            </div>
          ))}
        </div>

        <div className="create-row">
          <span className="muted">{t("create.format")}</span>
          <div className="segmented">
            <button className={format === "zip" ? "active" : ""} onClick={() => setFormat("zip")}>
              .zip
            </button>
            <button
              className={format === "targz" ? "active" : ""}
              onClick={() => setFormat("targz")}
            >
              .tar.gz
            </button>
          </div>
        </div>

        <div className="modal-actions">
          <button onClick={() => setCreateSources(null)}>{t("dlg.cancel")}</button>
          <button
            className="primary"
            disabled={sources.length === 0}
            onClick={() => void saveAndCreate(format, sources)}
          >
            {t("create.go")}
          </button>
        </div>
      </div>
    </div>
  );
}
