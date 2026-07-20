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
  "empty.formats": "zip (senha AES) · rar · 7z · tar · tar.gz/xz/bz2/zst · volumes .001",

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
  "toast.notArchive": "“{name}” não é um formato suportado (zip, rar, 7z, tar e variantes)",

  // Adicionar/remover num arquivo existente (v0.5)
  "top.add": "Adicionar…",
  "top.addTitle": "Adicionar arquivos a este zip (sem re-extrair o resto)",
  "top.remove": "Remover",
  "top.removeTitle": "Remover os itens selecionados do zip",
  "update.confirmRemove": "Remover {n} item(ns) do arquivo? O zip será reescrito.",
  "update.done": "Arquivo atualizado: {dest}",
  "update.onlyZip": "Adicionar/remover só funciona em zip — este formato é só leitura.",
  "update.notOnSplit": "Não dá pra alterar um arquivo dividido em volumes (só leitura).",

  // Erros de formato
  "err.multiDisk":
    "Este é um zip multi-disco de verdade (.z01/.z02), que guarda os deslocamentos por disco. Junte os volumes no WinRAR/7-Zip e abra o .zip resultante.",

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

  // Segundo plano (bandeja/autostart)
  "settings.background": "Segundo plano",
  "settings.closeToTray": "Fechar minimiza pra bandeja",
  "settings.closeToTrayHint":
    "O X esconde a janela em vez de sair; o app segue na bandeja. Uma extração em andamento NUNCA é interrompida pelo X — mesmo com esta opção desligada, a janela some e o trabalho continua.",
  "settings.autostart": "Abrir com o sistema",
  "settings.autostartHint":
    "Sobe junto com o login, direto na bandeja (sem roubar a tela). A escolha fica guardada no app e é reimposta a cada boot — se o LocalZip mudar de pasta, o atalho de inicialização é reescrito sozinho.",
  "settings.autostartDisabledByOs":
    "A inicialização foi desligada pelo Gerenciador de Tarefas do Windows. Reative por lá, ou marque aqui de novo.",
  "tray.show": "Mostrar/Ocultar",
  "tray.quit": "Sair",
  "toast.settingsFailed": "Não deu pra salvar a configuração: {error}",

  "settings.about":
    " — compactador 100% offline: abra e navegue zip/tar/tar.gz sem extrair, extraia tudo ou só a seleção (com progresso) e crie zip/tar.gz. Extração sempre protegida contra zip-slip. Parte da suíte Local.",
} as const;

export type MessageKey = keyof typeof pt;

const en: Record<MessageKey, string> = {
  "empty.title": "No archive open",
  "empty.sub": "Open an archive or create a new one. You can also drag one here.",
  "empty.open": "Open archive…",
  "empty.create": "Create archive…",
  "empty.formats": "zip (AES password) · rar · 7z · tar · tar.gz/xz/bz2/zst · .001 volumes",

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
  "toast.notArchive": "“{name}” is not a supported format (zip, rar, 7z, tar and variants)",

  // Adicionar/remover num arquivo existente (v0.5)
  "top.add": "Add…",
  "top.addTitle": "Add files to this zip (without re-extracting the rest)",
  "top.remove": "Remove",
  "top.removeTitle": "Remove the selected items from the zip",
  "update.confirmRemove": "Remove {n} item(s) from the archive? The zip will be rewritten.",
  "update.done": "Archive updated: {dest}",
  "update.onlyZip": "Add/remove only works on zip — this format is read-only.",
  "update.notOnSplit": "A split (multi-volume) archive can't be modified — read-only.",

  // Erros de formato
  "err.multiDisk":
    "This is a true multi-disk zip (.z01/.z02), which stores offsets per disk. Join the volumes in WinRAR/7-Zip and open the resulting .zip.",

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

  "settings.background": "Background",
  "settings.closeToTray": "Closing minimizes to the tray",
  "settings.closeToTrayHint":
    "The X hides the window instead of quitting; the app stays in the tray. An extraction in progress is NEVER interrupted by the X — even with this option off, the window disappears and the work carries on.",
  "settings.autostart": "Start with the system",
  "settings.autostartHint":
    "Starts at login, straight into the tray (without taking over the screen). The choice is stored in the app and reapplied on every boot — if LocalZip moves to another folder, the startup entry is rewritten by itself.",
  "settings.autostartDisabledByOs":
    "Startup was turned off in the Windows Task Manager. Re-enable it there, or tick this box again.",
  "tray.show": "Show/Hide",
  "tray.quit": "Quit",
  "toast.settingsFailed": "Could not save the setting: {error}",

  "settings.about":
    " — 100% offline archiver: browse zip/tar/tar.gz without extracting, extract all or just the selection (with progress) and create zip/tar.gz. Extraction is always zip-slip protected. Part of the Local suite.",
};

const es: Record<MessageKey, string> = {
  "empty.title": "Ningún archivo abierto",
  "empty.sub": "Abre un archivo comprimido o crea uno nuevo. También puedes arrastrarlo aquí.",
  "empty.open": "Abrir archivo…",
  "empty.create": "Crear archivo…",
  "empty.formats": "zip (contraseña AES) · rar · 7z · tar · tar.gz/xz/bz2/zst · volúmenes .001",

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
  "toast.notArchive": "“{name}” no es un formato soportado (zip, rar, 7z, tar y variantes)",

  // Adicionar/remover num arquivo existente (v0.5)
  "top.add": "Añadir…",
  "top.addTitle": "Añadir archivos a este zip (sin volver a extraer el resto)",
  "top.remove": "Quitar",
  "top.removeTitle": "Quitar del zip los elementos seleccionados",
  "update.confirmRemove": "¿Quitar {n} elemento(s) del archivo? El zip se reescribirá.",
  "update.done": "Archivo actualizado: {dest}",
  "update.onlyZip": "Añadir/quitar solo funciona en zip: este formato es de solo lectura.",
  "update.notOnSplit": "No se puede modificar un archivo dividido en volúmenes (solo lectura).",

  // Erros de formato
  "err.multiDisk":
    "Este es un zip multidisco real (.z01/.z02), que guarda los desplazamientos por disco. Une los volúmenes en WinRAR/7-Zip y abre el .zip resultante.",

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

  "settings.background": "Segundo plano",
  "settings.closeToTray": "Cerrar minimiza a la bandeja",
  "settings.closeToTrayHint":
    "La X oculta la ventana en vez de salir; la app sigue en la bandeja. Una extracción en curso NUNCA se interrumpe con la X — incluso con esta opción desactivada, la ventana desaparece y el trabajo continúa.",
  "settings.autostart": "Abrir con el sistema",
  "settings.autostartHint":
    "Arranca junto con el inicio de sesión, directo a la bandeja (sin apropiarse de la pantalla). La elección se guarda en la app y se reimpone en cada arranque — si LocalZip cambia de carpeta, la entrada de inicio se reescribe sola.",
  "settings.autostartDisabledByOs":
    "El inicio automático fue desactivado desde el Administrador de tareas de Windows. Reactívalo allí, o vuelve a marcar esta casilla.",
  "tray.show": "Mostrar/Ocultar",
  "tray.quit": "Salir",
  "toast.settingsFailed": "No se pudo guardar la configuración: {error}",

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

/**
 * Códigos de erro do backend → mensagem traduzida.
 *
 * O Rust devolve códigos estáveis (`NEED_PASSWORD`, `MULTI_DISK_ZIP`, …) em vez
 * de frases: assim a mensagem que o usuário lê é traduzida aqui, e um texto de
 * erro do sistema operacional (que não temos como traduzir) passa direto.
 */
export function tError(code: string | null | undefined): string {
  const raw = (code ?? "").replace(/^.*?Error:\s*/, "");
  switch (raw) {
    case "NEED_PASSWORD":
      return t("password.needed");
    case "WRONG_PASSWORD":
      return t("password.wrong");
    case "MULTI_DISK_ZIP":
      return t("err.multiDisk");
    case "UPDATE_ONLY_ZIP":
      return t("update.onlyZip");
    case "UPDATE_NOT_ON_SPLIT":
      return t("update.notOnSplit");
    default:
      return raw;
  }
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
