import { create } from "zustand";

export type Theme =
  | "light"
  | "dark"
  | "system"
  | "nature"
  | "darkblue"
  | "calmgreen"
  | "pastelpink"
  | "punkprincess";

export interface Toast {
  id: number;
  kind: "info" | "error" | "ok";
  text: string;
}

/** Pedido de senha pra extrair um zip cifrado. */
export interface PasswordAsk {
  dest: string;
  paths: string[] | null;
}

interface UiState {
  theme: Theme;
  settingsOpen: boolean;
  /** Modal de criação (null = fechado); guarda as origens já escolhidas. */
  createSources: string[] | null;
  /** Modal de senha na extração (null = fechado). */
  passwordAsk: PasswordAsk | null;
  toasts: Toast[];

  setTheme: (t: Theme) => void;
  setSettingsOpen: (v: boolean) => void;
  setCreateSources: (v: string[] | null) => void;
  setPasswordAsk: (v: PasswordAsk | null) => void;
  pushToast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
}

const THEME_KEY = "localzip.theme";

export const THEMES: Theme[] = [
  "system",
  "light",
  "dark",
  "nature",
  "darkblue",
  "calmgreen",
  "pastelpink",
  "punkprincess",
];

function loadTheme(): Theme {
  const v = localStorage.getItem(THEME_KEY);
  return v && (THEMES as string[]).includes(v) ? (v as Theme) : "system";
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
  passwordAsk: null,
  toasts: [],

  setTheme: (theme) => {
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
    set({ theme });
  },
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setCreateSources: (createSources) => set({ createSources }),
  setPasswordAsk: (passwordAsk) => set({ passwordAsk }),
  pushToast: (kind, text) =>
    set((s) => ({ toasts: [...s.toasts, { id: nextToast++, kind, text }] })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
