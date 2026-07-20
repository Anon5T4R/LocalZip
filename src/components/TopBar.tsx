import { open, save } from "@tauri-apps/plugin-dialog";
import * as backend from "../lib/backend";
import { t } from "../lib/i18n";
import { crumbsOf, isSupportedArchive } from "../lib/zpath";
import { EDITABLE_FORMATS } from "../lib/types";
import { useUi } from "../state/ui";
import { useZip } from "../state/store";

/** Abre o seletor de arquivo compactado e carrega no viewer. */
export async function pickAndOpen() {
  const picked = await open({
    multiple: false,
    filters: [
      {
        name: "Arquivos compactados",
        // `001`/`002`… = volumes de corte cru; `r00`… = volume RAR antigo.
        extensions: [
          "zip", "rar", "7z", "tar", "gz", "tgz", "xz", "txz", "bz2", "tbz2", "tbz", "zst", "tzst",
          "z01", "001", "002", "003", "r00", "r01",
        ],
      },
    ],
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

/** Extrai tudo ou a seleção (pergunta a pasta destino; senha se cifrado). */
export async function extractTo(paths: string[] | null) {
  const info = useZip.getState().info;
  const dest = await open({ directory: true, title: t("extract.chooseDest") });
  if (typeof dest !== "string") return;
  // Zip cifrado: pede a senha antes (a extração falharia com NEED_PASSWORD).
  const needsPw = info?.entries.some((e) => e.encrypted) ?? false;
  if (needsPw) {
    useUi.getState().setPasswordAsk({ dest, paths });
    return;
  }
  await useZip.getState().startExtract(dest, paths);
}

/** Testa a integridade do arquivo aberto (lê tudo, valida CRC/stream). */
export async function testIntegrity() {
  const info = useZip.getState().info;
  if (!info) return;
  const ui = useUi.getState();
  // Cifrado: pede a senha só pra testar (extração falharia sem ela).
  let password: string | null = null;
  if (info.entries.some((e) => e.encrypted)) {
    password = window.prompt(t("password.title")) || "";
    if (!password) return;
  }
  ui.pushToast("info", t("test.running"));
  try {
    const r = await backend.testIntegrity(info.path, password);
    if (r.ok) ui.pushToast("ok", t("test.ok", { n: r.tested }));
    else {
      // Códigos do backend viram mensagem traduzida.
      const error =
        r.error === "WRONG_PASSWORD"
          ? t("password.wrong")
          : r.error === "NEED_PASSWORD"
            ? t("password.needed")
            : (r.error ?? "");
      ui.pushToast("error", t("test.bad", { name: r.bad || "?", error }));
    }
  } catch (e) {
    ui.pushToast("error", t("test.bad", { name: "?", error: String(e) }));
  }
}

/** Escolhe arquivos do disco e acrescenta ao zip aberto (sem re-extrair). */
export async function addToArchive() {
  const info = useZip.getState().info;
  if (!info) return;
  const picked = await open({ multiple: true, title: t("top.add") });
  const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
  if (paths.length === 0) return;
  // Zip cifrado: as entradas NOVAS também têm que ir cifradas, senão o arquivo
  // fica meio protegido e meio não — o que ninguém espera.
  let password: string | null = null;
  if (info.entries.some((e) => e.encrypted)) {
    password = window.prompt(t("password.title")) || "";
    if (!password) return;
  }
  await useZip.getState().startUpdate(paths, [], password);
}

/** Tira do zip os itens selecionados (reconstrói sem descompactar o resto). */
export async function removeFromArchive(paths: string[]) {
  if (paths.length === 0) return;
  if (!window.confirm(t("update.confirmRemove", { n: paths.length }))) return;
  await useZip.getState().startUpdate([], paths, null);
}

/** Pergunta o destino do arquivo novo e dispara a compactação. */
export async function saveAndCreate(format: "zip" | "targz", sources: string[], password = "") {
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
    await useZip.getState().startCreate(dest, format, sources, password);
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
  // Só o zip (e não um zip dividido em volumes) aceita adicionar/remover.
  const editable =
    !!info && EDITABLE_FORMATS.includes(info.format) && !/\.\d{3,}$/.test(info.path);

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
            {editable && (
              <>
                <button title={t("top.addTitle")} onClick={() => void addToArchive()}>
                  {t("top.add")}
                </button>
                <button
                  title={t("top.removeTitle")}
                  onClick={() => void removeFromArchive(selection)}
                  disabled={selection.length === 0}
                >
                  {t("top.remove")}
                </button>
              </>
            )}
            <button title={t("top.test")} onClick={() => void testIntegrity()}>
              {t("top.test")}
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
