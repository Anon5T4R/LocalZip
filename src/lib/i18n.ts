import { useSyncExternalStore } from "react";

/**
 * i18n leve da UI (padrão da suíte, ver docs/planos/padrao-apps.md). O dict
 * `pt` é a fonte da verdade das chaves; `en`/`es` como `Record<MessageKey,
 * string>` fazem o compilador recusar chave faltando ou sobrando.
 */

export type Locale = "pt" | "en" | "es";

export const LOCALE_LABELS: Record<Locale, string> = {
  pt: "Português",
  en: "English",
  es: "Español",
};

export const LOCALE_TAGS: Record<Locale, string> = {
  pt: "pt-BR",
  en: "en-US",
  es: "es",
};

const LOCALE_KEY = "localzip.locale";

const pt = {
  // Estado vazio
  "empty.title": "Nenhum arquivo aberto",
  "empty.sub": "Abra um arquivo compactado ou crie um novo. Dá pra arrastar pra cá também.",
  "empty.open": "Abrir arquivo…",
  "empty.create": "Criar arquivo…",
  "empty.formats": "zip (senha AES) · 7z (extração) · tar · tar.gz/xz/bz2/zst (rar na v0.4)",

  // TopBar
  "top.open": "Abrir…",
  "top.openTitle": "Abrir arquivo compactado (Ctrl+O)",
  "top.create": "Criar…",
  "top.createTitle": "Criar arquivo compactado novo (Ctrl+N)",
  "top.extractAll": "Extrair tudo",
  "top.extractSel": "Extrair seleção",
  "top.test": "Testar",
  "top.close": "Fechar arquivo",
  "top.settingsTitle": "Configurações",
  "top.up": "Pasta acima (Backspace)",
  "top.root": "(raiz)",

  // Tabela
  "col.name": "Nome",
  "col.size": "Tamanho",
  "col.packed": "Compactado",
  "col.modified": "Modificado em",
  "table.items": "{n} itens",
  "table.protected": "protegido",
  "list.empty": "Pasta vazia dentro do arquivo",

  // Info do arquivo
  "info.summary": "{files} arquivos · {size} ({packed} compactado)",
  "info.bomb":
    "⚠️ Este arquivo expande MUITO além do tamanho compactado (possível zip bomb). Extraia só se confiar na origem.",
  "info.encrypted": "Este arquivo tem itens protegidos por senha — vou pedir a senha ao extrair.",

  // Senha (v0.2)
  "password.title": "Senha do arquivo",
  "password.sub": "Este arquivo está protegido. Digite a senha pra extrair.",
  "password.needed": "Este arquivo pede senha.",
  "password.wrong": "Senha incorreta — confira e tente de novo.",

  // Testar integridade (v0.2)
  "test.running": "Testando integridade…",
  "test.ok": "Íntegro — {n} itens conferidos",
  "test.bad": "Falha em “{name}”: {error}",

  // Criar com senha (v0.2)
  "create.password": "Senha (opcional)",
  "create.passwordHint": "cifra AES-256",

  // Diálogos de extração
  "extract.chooseDest": "Escolha a pasta de destino",
  "extract.done": "Extraído em {dest}",
  "extract.openDest": "Abrir pasta",

  // Criar
  "create.title": "Criar arquivo compactado",
  "create.sources": "O que vai dentro",
  "create.addFiles": "Adicionar arquivos…",
  "create.addFolder": "Adicionar pasta…",
  "create.remove": "remover",
  "create.empty": "Nada adicionado ainda — use os botões acima ou arraste itens pra janela.",
  "create.format": "Formato",
  "create.go": "Criar…",
  "create.done": "Arquivo criado: {dest}",
  "create.saveTitle": "Salvar arquivo compactado como",

  // Operações
  "ops.extracting": "Extraindo… {done} de {total}",
  "ops.creating": "Compactando… {done} de {total}",
  "ops.files": "{done}/{total} arquivos",
  "ops.cancel": "Cancelar",
  "ops.canceled": "Operação cancelada",

  // Toasts / erros
  "toast.openFailed": "Não consegui abrir: {error}",
  "toast.opFailed": "Falha na operação: {error}",
  "toast.notArchive": "“{name}” não é um formato suportado (v0.1: zip, tar, tar.gz)",

  // Atalhos / diversos
  "dlg.cancel": "Cancelar",
  "dlg.ok": "OK",

  // Settings
  "settings.title": "Configurações",
  "settings.theme": "Tema",
  "settings.themeSystem": "Sistema",
  "settings.themeLight": "Claro",
  "settings.themeDark": "Escuro",
  "settings.themeNature": "Natureza",
  "settings.themeDarkBlue": "Azul escuro",
  "settings.themeCalmGreen": "Verde calmo",
  "settings.themePastelPink": "Rosa pastel",
  "settings.themePunkPrincess": "PunkPrincess",
  "settings.language": "Idioma",
  "settings.about":
    " — compactador 100% offline: abra e navegue zip/tar/tar.gz sem extrair, extraia tudo ou só a seleção (com progresso) e crie zip/tar.gz. Extração sempre protegida contra zip-slip. Parte da suíte Local.",
} as const;

export type MessageKey = keyof typeof pt;

const en: Record<MessageKey, string> = {
  "empty.title": "No archive open",
  "empty.sub": "Open an archive or create a new one. You can also drag one here.",
  "empty.open": "Open archive…",
  "empty.create": "Create archive…",
  "empty.formats": "zip (AES password) · 7z (extract) · tar · tar.gz/xz/bz2/zst (rar in v0.4)",

  "top.open": "Open…",
  "top.openTitle": "Open archive (Ctrl+O)",
  "top.create": "New…",
  "top.createTitle": "Create a new archive (Ctrl+N)",
  "top.extractAll": "Extract all",
  "top.extractSel": "Extract selection",
  "top.test": "Test",
  "top.close": "Close archive",
  "top.settingsTitle": "Settings",
  "top.up": "Up one folder (Backspace)",
  "top.root": "(root)",

  "col.name": "Name",
  "col.size": "Size",
  "col.packed": "Packed",
  "col.modified": "Modified",
  "table.items": "{n} items",
  "table.protected": "protected",
  "list.empty": "Empty folder inside the archive",

  "info.summary": "{files} files · {size} ({packed} packed)",
  "info.bomb":
    "⚠️ This archive expands FAR beyond its packed size (possible zip bomb). Only extract if you trust the source.",
  "info.encrypted": "This archive has password-protected items — I'll ask for the password when extracting.",

  "password.title": "Archive password",
  "password.sub": "This archive is protected. Enter the password to extract.",
  "password.needed": "This archive needs a password.",
  "password.wrong": "Wrong password — check it and try again.",

  "test.running": "Testing integrity…",
  "test.ok": "Intact — {n} items checked",
  "test.bad": "Failed on “{name}”: {error}",

  "create.password": "Password (optional)",
  "create.passwordHint": "AES-256 encryption",

  "extract.chooseDest": "Choose the destination folder",
  "extract.done": "Extracted to {dest}",
  "extract.openDest": "Open folder",

  "create.title": "Create archive",
  "create.sources": "What goes inside",
  "create.addFiles": "Add files…",
  "create.addFolder": "Add folder…",
  "create.remove": "remove",
  "create.empty": "Nothing added yet — use the buttons above or drag items onto the window.",
  "create.format": "Format",
  "create.go": "Create…",
  "create.done": "Archive created: {dest}",
  "create.saveTitle": "Save archive as",

  "ops.extracting": "Extracting… {done} of {total}",
  "ops.creating": "Packing… {done} of {total}",
  "ops.files": "{done}/{total} files",
  "ops.cancel": "Cancel",
  "ops.canceled": "Operation canceled",

  "toast.openFailed": "Couldn't open: {error}",
  "toast.opFailed": "Operation failed: {error}",
  "toast.notArchive": "“{name}” is not a supported format (v0.1: zip, tar, tar.gz)",

  "dlg.cancel": "Cancel",
  "dlg.ok": "OK",

  "settings.title": "Settings",
  "settings.theme": "Theme",
  "settings.themeSystem": "System",
  "settings.themeLight": "Light",
  "settings.themeDark": "Dark",
  "settings.themeNature": "Nature",
  "settings.themeDarkBlue": "Dark blue",
  "settings.themeCalmGreen": "Calm green",
  "settings.themePastelPink": "Pastel pink",
  "settings.themePunkPrincess": "PunkPrincess",
  "settings.language": "Language",
  "settings.about":
    " — 100% offline archiver: browse zip/tar/tar.gz without extracting, extract all or just the selection (with progress) and create zip/tar.gz. Extraction is always zip-slip protected. Part of the Local suite.",
};

const es: Record<MessageKey, string> = {
  "empty.title": "Ningún archivo abierto",
  "empty.sub": "Abre un archivo comprimido o crea uno nuevo. También puedes arrastrarlo aquí.",
  "empty.open": "Abrir archivo…",
  "empty.create": "Crear archivo…",
  "empty.formats": "zip (contraseña AES) · 7z (extracción) · tar · tar.gz/xz/bz2/zst (rar en v0.4)",

  "top.open": "Abrir…",
  "top.openTitle": "Abrir archivo comprimido (Ctrl+O)",
  "top.create": "Nuevo…",
  "top.createTitle": "Crear un archivo comprimido nuevo (Ctrl+N)",
  "top.extractAll": "Extraer todo",
  "top.extractSel": "Extraer selección",
  "top.test": "Probar",
  "top.close": "Cerrar archivo",
  "top.settingsTitle": "Configuración",
  "top.up": "Carpeta superior (Backspace)",
  "top.root": "(raíz)",

  "col.name": "Nombre",
  "col.size": "Tamaño",
  "col.packed": "Comprimido",
  "col.modified": "Modificado",
  "table.items": "{n} elementos",
  "table.protected": "protegido",
  "list.empty": "Carpeta vacía dentro del archivo",

  "info.summary": "{files} archivos · {size} ({packed} comprimido)",
  "info.bomb":
    "⚠️ Este archivo se expande MUCHO más allá de su tamaño comprimido (posible zip bomb). Extrae solo si confías en el origen.",
  "info.encrypted":
    "Este archivo tiene elementos protegidos con contraseña — pediré la contraseña al extraer.",

  "password.title": "Contraseña del archivo",
  "password.sub": "Este archivo está protegido. Escribe la contraseña para extraer.",
  "password.needed": "Este archivo pide contraseña.",
  "password.wrong": "Contraseña incorrecta: revísala e inténtalo de nuevo.",

  "test.running": "Probando integridad…",
  "test.ok": "Íntegro — {n} elementos comprobados",
  "test.bad": "Falló en “{name}”: {error}",

  "create.password": "Contraseña (opcional)",
  "create.passwordHint": "cifrado AES-256",

  "extract.chooseDest": "Elige la carpeta de destino",
  "extract.done": "Extraído en {dest}",
  "extract.openDest": "Abrir carpeta",

  "create.title": "Crear archivo comprimido",
  "create.sources": "Qué va dentro",
  "create.addFiles": "Añadir archivos…",
  "create.addFolder": "Añadir carpeta…",
  "create.remove": "quitar",
  "create.empty": "Nada añadido todavía — usa los botones de arriba o arrastra elementos a la ventana.",
  "create.format": "Formato",
  "create.go": "Crear…",
  "create.done": "Archivo creado: {dest}",
  "create.saveTitle": "Guardar archivo como",

  "ops.extracting": "Extrayendo… {done} de {total}",
  "ops.creating": "Comprimiendo… {done} de {total}",
  "ops.files": "{done}/{total} archivos",
  "ops.cancel": "Cancelar",
  "ops.canceled": "Operación cancelada",

  "toast.openFailed": "No se pudo abrir: {error}",
  "toast.opFailed": "Error en la operación: {error}",
  "toast.notArchive": "“{name}” no es un formato soportado (v0.1: zip, tar, tar.gz)",

  "dlg.cancel": "Cancelar",
  "dlg.ok": "OK",

  "settings.title": "Configuración",
  "settings.theme": "Tema",
  "settings.themeSystem": "Sistema",
  "settings.themeLight": "Claro",
  "settings.themeDark": "Oscuro",
  "settings.themeNature": "Naturaleza",
  "settings.themeDarkBlue": "Azul oscuro",
  "settings.themeCalmGreen": "Verde tranquilo",
  "settings.themePastelPink": "Rosa pastel",
  "settings.themePunkPrincess": "PunkPrincess",
  "settings.language": "Idioma",
  "settings.about":
    " — compresor 100% offline: navega zip/tar/tar.gz sin extraer, extrae todo o solo la selección (con progreso) y crea zip/tar.gz. Extracción siempre protegida contra zip-slip. Parte de la suite Local.",
};

const DICTS: Record<Locale, Record<MessageKey, string>> = { pt, en, es };

/** Palpite de locale pelo idioma do sistema (só no 1º uso). */
export function detectLocale(): Locale {
  const l = (typeof navigator !== "undefined" ? navigator.language : "pt").toLowerCase();
  if (l.startsWith("en")) return "en";
  if (l.startsWith("es")) return "es";
  return "pt";
}

function loadLocale(): Locale {
  const v = typeof localStorage !== "undefined" ? localStorage.getItem(LOCALE_KEY) : null;
  return v === "pt" || v === "en" || v === "es" ? v : detectLocale();
}

let current: Locale = loadLocale();
const listeners = new Set<() => void>();

export function getLocale(): Locale {
  return current;
}

export function localeTag(): string {
  return LOCALE_TAGS[current];
}

export function setLocale(locale: Locale) {
  if (locale === current) return;
  current = locale;
  try {
    localStorage.setItem(LOCALE_KEY, locale);
  } catch {
    /* localStorage indisponível */
  }
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, getLocale);
}

/** Traduz uma chave, interpolando placeholders `{param}`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  let msg: string = DICTS[current][key] ?? pt[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      msg = msg.split(`{${k}}`).join(String(v));
    }
  }
  return msg;
}
