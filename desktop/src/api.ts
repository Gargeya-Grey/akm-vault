import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { completedWhileLocked, reportCompletion, sessionGeneration } from "./sessionBoundary";
import type {
  AuditReport,
  BackupInfo,
  ImportReport,
  ImportPreview,
  KeyDraft,
  KeyView,
  Status,
} from "./types";

const mutations = new Set(["vault_create", "vault_upgrade", "key_add", "key_update", "key_remove", "env_import_confirm", "env_save"]);

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const generation = sessionGeneration();
  try {
    const result = await nativeInvoke<T>(command, args);
    if (command !== "vault_lock" && command !== "vault_status" && generation !== sessionGeneration()) {
      if (mutations.has(command) && !(command === "env_save" && result === null)) throw completedWhileLocked();
      throw { kind: "locked", message: "The session changed. Unlock the vault again." };
    }
    return result;
  } catch (error) {
    if (errorKind(error) === "completedWhileLocked") reportCompletion(generation);
    throw error;
  }
}

export const api = {
  status: () => invoke<Status>("vault_status"),
  unlock: (path: string, password: string) =>
    invoke<Status>("vault_unlock", { path, password }),
  create: (path: string, password: string, name: string | null) =>
    invoke<Status>("vault_create", { path, password, name }),
  lock: () => invoke<void>("vault_lock"),
  activity: () => invoke<void>("vault_activity"),
  upgrade: () => invoke<Status>("vault_upgrade"),
  reload: () => invoke<Status>("vault_reload"),
  warnings: () => invoke<string[]>("vault_warnings"),

  keys: () => invoke<KeyView[]>("keys_list"),
  reveal: (id: string) => invoke<string>("key_reveal", { id }),
  copy: (id: string) => invoke<number>("key_copy", { id }),
  add: (draft: KeyDraft) => invoke<KeyView>("key_add", { draft }),
  update: (id: string, draft: KeyDraft) =>
    invoke<KeyView>("key_update", { id, draft }),
  remove: (id: string) => invoke<void>("key_remove", { id }),

  audit: (limit = 300) => invoke<AuditReport>("audit_read", { limit }),
  exportEnv: (project: string | null, environment: string | null) =>
    invoke<string>("env_export", { project, environment, all: false }),
  copyEnv: (project: string | null, environment: string | null) =>
    invoke<number>("env_copy", { project, environment, all: false }),
  saveEnv: (
    project: string | null,
    environment: string | null,
  ) => invoke<string | null>("env_save", { project, environment, all: false }),
  previewImport: (
    text: string,
    project: string | null,
    environment: string | null,
  ) => invoke<ImportPreview>("env_import_preview", { text, project, environment }),
  confirmImport: (token: string) => invoke<ImportReport>("env_import_confirm", { token }),
  backup: () => invoke<BackupInfo>("backup_info"),
};

export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string" && message.length > 0) return message;
  }
  return "Something went wrong. Try again.";
}

export function errorKind(error: unknown): string {
  if (error && typeof error === "object" && "kind" in error) {
    return String((error as { kind: unknown }).kind);
  }
  return "unknown";
}
