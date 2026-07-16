import { create } from "zustand";

export type Theme = "light" | "dark" | "system";

export interface Toast {
  id: number;
  kind: "info" | "error" | "ok";
  text: string;
}

interface UiState {
  theme: Theme;
  settingsOpen: boolean;
  /** Modal de criação (null = fechado); guarda as origens já escolhidas. */
  createSources: string[] | null;
  toasts: Toast[];

  setTheme: (t: Theme) => void;
  setSettingsOpen: (v: boolean) => void;
  setCreateSources: (v: string[] | null) => void;
  pushToast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
}

const THEME_KEY = "localzip.theme";

function loadTheme(): Theme {
  const v = localStorage.getItem(THEME_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

/** Aplica o tema no <html data-theme> (resolvendo "system" pela mídia). */
export function applyTheme(theme: Theme) {
  const resolved =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme;
  document.documentElement.dataset.theme = resolved;
}

let nextToast = 1;

export const useUi = create<UiState>((set) => ({
  theme: loadTheme(),
  settingsOpen: false,
  createSources: null,
  toasts: [],

  setTheme: (theme) => {
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
    set({ theme });
  },
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setCreateSources: (createSources) => set({ createSources }),
  pushToast: (kind, text) =>
    set((s) => ({ toasts: [...s.toasts, { id: nextToast++, kind, text }] })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
