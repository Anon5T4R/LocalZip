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
