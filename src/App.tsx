import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openPath } from "@tauri-apps/plugin-opener";
import { getStartupFile, isTauri } from "./lib/backend";
import { t } from "./lib/i18n";
import { childrenOf, isSupportedArchive, parentDir } from "./lib/zpath";
import type { OpDone, OpProgress } from "./lib/types";
import CreateModal from "./components/CreateModal";
import EmptyState from "./components/EmptyState";
import EntryTable from "./components/EntryTable";
import OpsPanel from "./components/OpsPanel";
import PasswordModal from "./components/PasswordModal";
import SettingsModal from "./components/SettingsModal";
import Toasts from "./components/Toasts";
import TopBar, { pickAndOpen } from "./components/TopBar";
import { useUi } from "./state/ui";
import { useZip } from "./state/store";

export default function App() {
  // Boot: arquivo passado no launch (associação/abrir com).
  useEffect(() => {
    if (!isTauri) return;
    void getStartupFile()
      .then((f) => {
        if (f && isSupportedArchive(f)) void useZip.getState().open(f);
      })
      .catch(() => {});
  }, []);

  // Eventos: progresso/fim das operações, 2ª instância e drag-and-drop.
  useEffect(() => {
    if (!isTauri) return;
    const un1 = listen<OpProgress>("zipop-progress", (e) => {
      useZip.getState().opProgress(e.payload.opId, e.payload);
    });
    const un2 = listen<OpDone>("zipop-done", (e) => {
      const zip = useZip.getState();
      const ui = useUi.getState();
      const op = zip.ops.find((o) => o.opId === e.payload.opId);
      zip.opDone(e.payload.opId);
      if (e.payload.canceled) {
        ui.pushToast("info", t("ops.canceled"));
      } else if (!e.payload.ok && e.payload.error === "NEED_PASSWORD") {
        // Fallback (o extractTo já pede a senha antes quando detecta cifra).
        ui.pushToast("error", t("password.needed"));
      } else if (!e.payload.ok && e.payload.error) {
        ui.pushToast("error", t("toast.opFailed", { error: e.payload.error }));
      } else if (e.payload.ok && e.payload.output) {
        if (op?.kind === "extract") {
          ui.pushToast("ok", t("extract.done", { dest: e.payload.output }));
          void openPath(e.payload.output).catch(() => {}); // abre a pasta extraída
        } else {
          ui.pushToast("ok", t("create.done", { dest: e.payload.output }));
        }
      }
    });
    const un3 = listen<string>("open-file", (e) => {
      if (isSupportedArchive(e.payload)) void useZip.getState().open(e.payload);
    });
    // Drop do SO: arquivo suportado abre; qualquer outra coisa vira origem
    // do "criar arquivo" (modal já aberto ganha os itens).
    const un4 = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop" || event.payload.paths.length === 0) return;
      const paths = event.payload.paths;
      const ui = useUi.getState();
      if (ui.createSources !== null) {
        const next = [...ui.createSources];
        for (const p of paths) if (!next.includes(p)) next.push(p);
        ui.setCreateSources(next);
        return;
      }
      if (paths.length === 1 && isSupportedArchive(paths[0])) {
        void useZip.getState().open(paths[0]);
      } else {
        ui.setCreateSources(paths);
      }
    });
    return () => {
      for (const un of [un1, un2, un3, un4]) void un.then((f) => f());
    };
  }, []);

  // Atalhos: Ctrl+O abre, Ctrl+N cria, Ctrl+A seleciona, Backspace sobe, Esc.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable) return;
      const zip = useZip.getState();
      const ui = useUi.getState();
      const key = e.key.toLowerCase();

      if (e.ctrlKey && key === "o") {
        e.preventDefault();
        void pickAndOpen();
        return;
      }
      if (e.ctrlKey && key === "n") {
        e.preventDefault();
        ui.setCreateSources([]);
        return;
      }
      if (!zip.info) return;
      if (e.ctrlKey && key === "a") {
        e.preventDefault();
        zip.setSelection(childrenOf(zip.info.entries, zip.dir).map((n) => n.path));
        return;
      }
      if (e.key === "Backspace") {
        e.preventDefault();
        if (zip.dir !== "") zip.setDir(parentDir(zip.dir));
        return;
      }
      if (e.key === "Escape") zip.setSelection([], null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const info = useZip((s) => s.info);

  return (
    <div className="app">
      <TopBar />
      {info ? <EntryTable /> : <EmptyState />}
      <CreateModal />
      <PasswordModal />
      <SettingsModal />
      <OpsPanel />
      <Toasts />
    </div>
  );
}
