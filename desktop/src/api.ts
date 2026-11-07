import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { beginOutcome, completedWhileLocked, reportCompletion, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { acknowledgeNotices, receiveWarnings } from "./operationNotices";
import { acceptNativeStatus, adoptReservation, beginNativeOpen, confirmNativeLock, endNativeOpen, invalidateNativeView, nativeCancellation, nativeNoticeStamp, nativeOwner, releaseNativeOwner } from "./nativeSession";
import type {
  AuditReport,
  BackupInfo,
  ImportReport,
  ImportPreview,
  KeyDraft,
  KeyView,
  Status,
  WarningReport,
} from "./types";

const mutations = new Set(["vault_create", "vault_upgrade", "key_add", "key_update", "key_remove", "env_import_confirm", "env_save"]);
const statusReplies = new Set(["vault_status", "vault_unlock", "vault_create", "vault_upgrade", "vault_reload"]);

async function invoke<T>(command: string, args?: Record<string, unknown>, expectedOwner?: string): Promise<T> {
  const generation = sessionGeneration();
  const bootstrap = command === "vault_status" && nativeOwner() === null;
  const notices = nativeNoticeStamp();
  const outcome = command === "env_import_confirm" ? beginOutcome("import", generation)
    : command === "env_save" ? beginOutcome("export", generation) : undefined;
  const cancelled = () => ({ kind: "locked", message: "The session changed. Unlock the vault again." });
  let knownSuccess = false;
  try {
    const result = await nativeInvoke<T>(command, args);
    knownSuccess = !(command === "env_save" && result === null);
    if (command === "env_save" && result === null && outcome !== undefined) settleOutcome(outcome, "cancelled");
    if (generation !== sessionGeneration()) {
      if (mutations.has(command) && !(command === "env_save" && result === null)) throw completedWhileLocked();
      throw cancelled();
    }
    if (statusReplies.has(command)) acceptNativeStatus(result as Status, expectedOwner);
    if (outcome !== undefined) settleOutcome(outcome, "completed");
    return result;
  } catch (error) {
    if (errorKind(error) === "completedWhileLocked" || (knownSuccess && mutations.has(command))) {
      if (outcome !== undefined) settleOutcome(outcome, "completedWhileLocked");
      else reportCompletion(generation);
      throw completedWhileLocked(); // Do not forward native details, even in a known-commit error.
    }
    // A bootstrap error carries no owner, unlike a successful status. If a native
    // notice overtook it, its details cannot be attributed safely. A success for
    // a different, non-tombstoned owner is still accepted above, not blanket-dropped.
    if (bootstrap && generation === sessionGeneration() && notices !== nativeNoticeStamp()) invalidateNativeView();
    const obsolete = generation !== sessionGeneration();
    if (outcome !== undefined) settleOutcome(outcome, obsolete ? "unconfirmed" : "reportedError");
    if (obsolete) throw cancelled();
    throw error;
  }
}

async function open(command: "vault_unlock" | "vault_create", args: Record<string, unknown>): Promise<Status> {
  const token = beginNativeOpen();
  let reservation: string | null = null;
  try {
    const prepared = await invoke<string>("vault_prepare_open");
    if (token.generation !== sessionGeneration()) throw nativeCancellation();
    reservation = adoptReservation(prepared);
    return await invoke<Status>(command, { ...args, sessionId: reservation }, reservation);
  } catch (error) {
    if (token.generation === sessionGeneration()) {
      if (reservation !== null) releaseNativeOwner(reservation);
      if (["locked", "sessionUnavailable"].includes(errorKind(error))) {
        invalidateNativeView("The native session is unavailable. Check Windows session availability before opening the vault again.");
      }
    }
    throw error;
  } finally { endNativeOpen(token); }
}

export const api = {
  status: () => invoke<Status>("vault_status"),
  unlock: (path: string, password: string) =>
    open("vault_unlock", { path, password }),
  create: (path: string, password: string, name: string | null) =>
    open("vault_create", { path, password, name }),
  lock: async () => {
    const sessionId = nativeOwner();
    await invoke<void>("vault_lock", { sessionId });
    confirmNativeLock(sessionId);
  },
  lockCurrent: async () => {
    // Fail-closed path: ask which session is current, then lock exactly that one.
    const current = await invoke<Status>("vault_status");
    if (current.unlocked) await api.lock();
  },
  activity: () => invoke<void>("vault_activity"),
  upgrade: () => invoke<Status>("vault_upgrade"),
  reload: () => invoke<Status>("vault_reload"),
  warnings: async () => {
    const report = await nativeInvoke<WarningReport>("vault_warnings");
    // This channel contains app warnings and fixed, origin-owned receipts, never protected vault results.
    receiveWarnings(report);
  },
  dismissWarnings: async (ids: string[]) => {
    // ID-only acknowledgment is application-wide too; a newer vault must not suppress a completed dismissal.
    await nativeInvoke<void>("vault_dismiss_warnings", { ids });
    acknowledgeNotices(ids);
  },

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
