import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "../lib/i18n";
import { crumbsOf, isSupportedArchive } from "../lib/zpath";
import { useUi } from "../state/ui";
import { useZip } from "../state/store";

/** Abre o seletor de arquivo compactado e carrega no viewer. */
export async function pickAndOpen() {
  const picked = await open({
    multiple: false,
    filters: [{ name: "Arquivos compactados", extensions: ["zip", "tar", "gz", "tgz"] }],
  });
  if (typeof picked === "string") {
    if (!isSupportedArchive(picked)) {
      const name = picked.split(/[\\/]/).pop() ?? picked;
      useUi.getState().pushToast("error", t("toast.notArchive", { name }));
      return;
    }
    await useZip.getState().open(picked);
  }
}

/** Extrai tudo ou a seleção (pergunta a pasta destino). */
export async function extractTo(paths: string[] | null) {
  const dest = await open({ directory: true, title: t("extract.chooseDest") });
  if (typeof dest === "string") {
    await useZip.getState().startExtract(dest, paths);
  }
}

/** Pergunta o destino do arquivo novo e dispara a compactação. */
export async function saveAndCreate(format: "zip" | "targz", sources: string[]) {
  const first = sources[0]?.split(/[\\/]/).pop() ?? "arquivo";
  const base = sources.length === 1 ? first.replace(/\.[^.]+$/, "") : first;
  const ext = format === "zip" ? "zip" : "tar.gz";
  const dest = await save({
    title: t("create.saveTitle"),
    defaultPath: `${base}.${ext}`,
    filters: [{ name: ext, extensions: [format === "zip" ? "zip" : "gz"] }],
  });
  if (typeof dest === "string") {
    useUi.getState().setCreateSources(null);
    await useZip.getState().startCreate(dest, format, sources);
  }
}

/** Barra superior: abrir/criar, breadcrumb interno, extrair, config. */
export default function TopBar() {
  const info = useZip((s) => s.info);
  const dir = useZip((s) => s.dir);
  const selection = useZip((s) => s.selection);
  const { setDir, close } = useZip.getState();
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const setCreateSources = useUi((s) => s.setCreateSources);

  const crumbs = crumbsOf(dir);
  const archiveName = info?.path.split(/[\\/]/).pop();

  return (
    <div className="topbar">
      <button title={t("top.openTitle")} onClick={() => void pickAndOpen()}>
        {t("top.open")}
      </button>
      <button title={t("top.createTitle")} onClick={() => setCreateSources([])}>
        {t("top.create")}
      </button>

      {info && (
        <>
          <div className="breadcrumb">
            <button className="crumb archive-name" title={info.path} onClick={() => setDir("")}>
              🗜️ {archiveName}
            </button>
            {crumbs.map((c) => (
              <span key={c.path} className="crumb-wrap">
                <span className="crumb-sep">›</span>
                <button className="crumb" onClick={() => setDir(c.path)}>
                  {c.name}
                </button>
              </span>
            ))}
          </div>

          <div className="topbar-actions">
            <button
              className="primary"
              onClick={() => void extractTo(null)}
              disabled={info.entries.length === 0}
            >
              {t("top.extractAll")}
            </button>
            <button
              onClick={() => void extractTo(selection)}
              disabled={selection.length === 0}
            >
              {t("top.extractSel")}
            </button>
            <button title={t("top.close")} onClick={close}>
              ✕
            </button>
          </div>
        </>
      )}
      {!info && <div className="crumb-fill" />}

      <button title={t("top.settingsTitle")} onClick={() => setSettingsOpen(true)}>
        ⚙
      </button>
    </div>
  );
}
