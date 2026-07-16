import { create } from "zustand";
import * as backend from "../lib/backend";
import { t } from "../lib/i18n";
import type { ArchiveInfo, OpKind, RunningOp } from "../lib/types";
import { useUi } from "./ui";

/** Estado central: o arquivo aberto, a pasta interna atual, seleção e ops. */
interface ZipState {
  info: ArchiveInfo | null;
  /** Pasta interna atual ("" = raiz). */
  dir: string;
  selection: string[];
  anchor: number | null;
  ops: RunningOp[];

  open: (path: string) => Promise<void>;
  close: () => void;
  setDir: (dir: string) => void;
  setSelection: (paths: string[], anchor?: number | null) => void;
  startExtract: (dest: string, paths: string[] | null) => Promise<void>;
  startCreate: (dest: string, format: "zip" | "targz", sources: string[]) => Promise<void>;
  opProgress: (opId: number, p: RunningOp["progress"]) => void;
  opDone: (opId: number) => void;
}

export const useZip = create<ZipState>((set, get) => ({
  info: null,
  dir: "",
  selection: [],
  anchor: null,
  ops: [],

  open: async (path) => {
    try {
      const info = await backend.openArchive(path);
      set({ info, dir: "", selection: [], anchor: null });
    } catch (e) {
      useUi.getState().pushToast("error", t("toast.openFailed", { error: String(e) }));
    }
  },

  close: () => set({ info: null, dir: "", selection: [], anchor: null }),

  setDir: (dir) => set({ dir, selection: [], anchor: null }),

  setSelection: (selection, anchor) =>
    set((s) => ({ selection, anchor: anchor === undefined ? s.anchor : anchor })),

  startExtract: async (dest, paths) => {
    const info = get().info;
    if (!info) return;
    await startOp(set, "extract", () => backend.startExtract(info.path, dest, paths));
  },

  startCreate: async (dest, format, sources) => {
    await startOp(set, "create", () => backend.startCreate(dest, format, sources));
  },

  opProgress: (opId, progress) =>
    set((s) => ({ ops: s.ops.map((o) => (o.opId === opId ? { ...o, progress } : o)) })),

  opDone: (opId) => set((s) => ({ ops: s.ops.filter((o) => o.opId !== opId) })),
}));

async function startOp(
  set: (fn: (s: ZipState) => Partial<ZipState>) => void,
  kind: OpKind,
  invoke: () => Promise<number>,
) {
  try {
    const opId = await invoke();
    set((s) => ({ ops: [...s.ops, { opId, kind, progress: null }] }));
  } catch (e) {
    useUi.getState().pushToast("error", t("toast.opFailed", { error: String(e) }));
  }
}
