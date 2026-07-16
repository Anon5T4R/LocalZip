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
): Promise<number> {
  return invoke("start_extract", { archive, dest, paths });
}

export function startCreate(
  dest: string,
  format: "zip" | "targz",
  sources: string[],
): Promise<number> {
  return invoke("start_create", { dest, format, sources });
}

export function cancelOp(opId: number): Promise<void> {
  return invoke("cancel_op", { opId });
}

export function getStartupFile(): Promise<string | null> {
  return invoke("get_startup_file");
}
