import { invoke } from "@tauri-apps/api/core";
import type { ArchiveInfo } from "./types";

/** Rodando dentro do Tauri? (o smoke em navegador puro não tem a ponte.) */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function openArchive(path: string): Promise<ArchiveInfo> {
  return invoke("open_archive", { path });
}

/** `paths` = null extrai tudo. Progresso via `zipop-progress`/`zipop-done`. */
export function startExtract(
  archive: string,
  dest: string,
  paths: string[] | null,
  password?: string | null,
): Promise<number> {
  return invoke("start_extract", { archive, dest, paths, password: password ?? null });
}

export function startCreate(
  dest: string,
  format: "zip" | "targz",
  sources: string[],
  password?: string | null,
): Promise<number> {
  return invoke("start_create", { dest, format, sources, password: password ?? null });
}

/**
 * Adiciona (`add`, caminhos no disco) e/ou remove (`remove`, caminhos DENTRO do
 * arquivo) num zip existente, sem re-extrair o resto. Progresso pelos mesmos
 * eventos das outras operações.
 */
export function startUpdate(
  archive: string,
  add: string[],
  remove: string[],
  password?: string | null,
): Promise<number> {
  return invoke("start_update", { archive, add, remove, password: password ?? null });
}

export interface IntegrityResult {
  ok: boolean;
  tested: number;
  bad: string;
  error: string | null;
}

export function testIntegrity(archive: string, password?: string | null): Promise<IntegrityResult> {
  return invoke("test_integrity", { archive, password: password ?? null });
}

export function cancelOp(opId: number): Promise<void> {
  return invoke("cancel_op", { opId });
}

export function getStartupFile(): Promise<string | null> {
  return invoke("get_startup_file");
}

// ---------- bandeja e autostart ----------

/**
 * A intenção de "abrir com o sistema" mora no BACKEND (settings.json na pasta de
 * dados), não no registro do Windows: o registro é só o efeito, e um efeito que
 * envelhece sozinho quando o exe muda de lugar. Por isso não há localStorage
 * aqui — o valor exibido é sempre o que o backend reconciliou no boot.
 */
export function autostartGet(): Promise<boolean> {
  return invoke("autostart_get");
}

export function autostartSet(enabled: boolean): Promise<void> {
  return invoke("autostart_set", { enabled });
}

export function closeToTrayGet(): Promise<boolean> {
  return invoke("close_to_tray_get");
}

export function closeToTraySet(enabled: boolean): Promise<void> {
  return invoke("close_to_tray_set", { enabled });
}

/** Manda os rótulos traduzidos pro menu da bandeja (que nasce antes do front). */
export function trayLabelsSet(show: string, quit: string): Promise<void> {
  return invoke("tray_labels_set", { show, quit });
}
