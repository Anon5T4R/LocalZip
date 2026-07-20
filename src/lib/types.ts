/** Espelho dos structs do Rust (serde camelCase). */

export type ArchiveFormat =
  | "zip"
  | "rar"
  | "sevenz"
  | "tar"
  | "targz"
  | "tarxz"
  | "tarbz2"
  | "tarzst";

/** Formatos que a gente sabe ALTERAR (adicionar/remover) — o resto é só leitura. */
export const EDITABLE_FORMATS: ArchiveFormat[] = ["zip"];

export interface AEntry {
  /** Caminho DENTRO do arquivo ("/" como separador, sem barra no fim). */
  path: string;
  isDir: boolean;
  size: number;
  compressed: number;
  modifiedMs: number;
  encrypted: boolean;
}

export interface ArchiveInfo {
  path: string;
  format: ArchiveFormat;
  entries: AEntry[];
  totalSize: number;
  archiveBytes: number;
  bombSuspect: boolean;
}

export interface OpProgress {
  opId: number;
  doneFiles: number;
  totalFiles: number;
  doneBytes: number;
  totalBytes: number;
  current: string;
}

export interface OpDone {
  opId: number;
  ok: boolean;
  canceled: boolean;
  error: string | null;
  output: string | null;
}

export type OpKind = "extract" | "create" | "update";

export interface RunningOp {
  opId: number;
  kind: OpKind;
  progress: OpProgress | null;
}

/** Nó calculado da visão de pasta (dentro do arquivo). */
export interface VNode {
  name: string;
  /** Caminho interno completo. */
  path: string;
  isDir: boolean;
  size: number;
  compressed: number;
  modifiedMs: number;
  encrypted: boolean;
  /** Nº de itens diretos (só pastas). */
  children: number;
}
