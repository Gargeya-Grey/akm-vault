import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { beginOutcome, completedWhileLocked, reportCompletion, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { acknowledgeNotices, receiveWarnings } from "./operationNotices";
import { acceptNativeStatus, adoptReservation, beginNativeOpen, confirmNativeLock, endNativeOpen, invalidateNativeView, nativeCancellation, nativeNoticeStamp, nativeOwner, releaseNativeOwner } from "./nativeSession";
import type {
  AuditReport,
  BackupInfo,
  HistoryVersion,
  ImportReport,
  JournalRecovery,
  KeyDraft,
  KeyView,
  RecycledKey,
  ReminderStatus,
  RotationReceipt,
  Status,
  WarningReport,
} from "./types";

const mutations = new Set(["vault_create", "vault_upgrade", "key_add", "key_update", "key_remove", "key_rotate", "key_restore", "key_purge", "env_import", "env_save", "env_launch_terminal", "audit_recover"]);
// A request that the user can still decline in a native window answers null when they do.
// The disclosing commands are on the list too: a page can read its session ID, so native asks the
// user before it shows or copies a value (the bulk commands always, the single-key commands once a
// session is over its budget). A decline gives no value and writes nothing to the clipboard.
const declinable = new Set(["env_import", "env_save", "env_launch_terminal", "key_reveal", "key_copy", "env_export", "env_copy", "audit_recover"]);

function declined(what: string) {
  return { kind: "declined", message: `Cancelled. ${what}` };
}
const statusReplies = new Set(["vault_status", "vault_unlock", "vault_create", "vault_upgrade", "vault_reload"]);

// Every command that reads or changes the open vault, or renews its idle deadline, names the
// session in which the page made the request. Native refuses a request for a session that is not
// the current one, so a request that arrives late cannot act on a vault that opened after it. The
// commands that start a session, probe which one is current, or concern the whole application do
// not name one.
const sessionScoped = new Set(["vault_activity", "vault_upgrade", "vault_reload", "keys_list", "key_reveal", "key_copy", "key_add", "key_update", "key_remove", "key_rotate", "recycled_list", "key_restore", "key_purge", "audit_read", "audit_recover", "env_export", "env_copy", "env_save", "env_launch_terminal", "key_history", "backup_info"]);

function withSession(command: string, args?: Record<string, unknown>) {
  return sessionScoped.has(command) ? { ...args, sessionId: nativeOwner() ?? "" } : args;
}

async function invoke<T>(command: string, args?: Record<string, unknown>, expectedOwner?: string, operation?: string): Promise<T> {
  const generation = sessionGeneration();
  const bootstrap = command === "vault_status" && nativeOwner() === null;
  const notices = nativeNoticeStamp();
  const outcome = command === "env_import" ? beginOutcome("import", generation, operation)
    : command === "env_save" ? beginOutcome("export", generation) : undefined;
  const cancelled = () => ({ kind: "locked", message: "The session changed. Unlock the vault again." });
  let knownSuccess = false;
  try {
    const result = await nativeInvoke<T>(command, withSession(command, args));
    const declined = declinable.has(command) && result === null;
    knownSuccess = !declined;
    if (declined && outcome !== undefined) settleOutcome(outcome, "cancelled");
    if (operation !== undefined) acknowledgeImportRecord(operation);
    if (generation !== sessionGeneration()) {
      if (mutations.has(command) && !declined) throw completedWhileLocked();
      throw cancelled();
    }
    if (statusReplies.has(command)) acceptNativeStatus(result as Status, expectedOwner);
    if (outcome !== undefined) settleOutcome(outcome, "completed");
    // The page can expire and drop this reply, and a window that holds keys must still be on record.
    if (command === "env_launch_terminal" && !declined) settleOutcome(beginOutcome("terminal", generation), "completed");
    return result;
  } catch (error) {
    if (operation !== undefined && errorKind(error) !== "unknown") acknowledgeImportRecord(operation);
    if (errorKind(error) === "completedWhileLocked" || (knownSuccess && mutations.has(command))) {
      if (outcome !== undefined) settleOutcome(outcome, "completedWhileLocked");
      // A terminal that opened cannot be taken back, so its notice must say that it holds keys.
      else if (command === "env_launch_terminal") settleOutcome(beginOutcome("terminal", generation), "completedWhileLocked");
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

// A reply that came from the native command is the outcome, so its native record is redundant.
// A transport failure leaves the record in place: it may be the only truth about the request.
function acknowledgeImportRecord(operation: string) {
  acknowledgeNotices([operation]);
  void dismissNatively([operation]);
}

async function dismissNatively(ids: string[]) {
  try { await nativeInvoke<void>("vault_dismiss_warnings", { ids }); } catch { /* the record stays and is offered again */ }
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

async function lockSession(sessionId: string | null): Promise<void> {
  await invoke<void>("vault_lock", { sessionId });
  confirmNativeLock(sessionId);
}

export const api = {
  status: () => invoke<Status>("vault_status"),
  unlock: (path: string, password: string) =>
    open("vault_unlock", { path, password }),
  create: (path: string, password: string, name: string | null) =>
    open("vault_create", { path, password, name }),
  lock: () => lockSession(nativeOwner()),
  lockCurrent: async () => {
    // Fail-closed path: ask which session is current, then lock exactly that one.
    const current = await invoke<Status>("vault_status");
    if (current.unlocked) await api.lock();
  },
  activity: () => invoke<void>("vault_activity"),
  // The document hid and the page discarded its views, in the session it names. Native expires
  // that session's side of them too. The call can arrive after the session ended: native then
  // does nothing and answers success. A native refusal other than "locked" means that the
  // expiry is not certain, so the vault locks. That lock names the same session and never asks
  // which session is current, so a newer vault is not locked for an older one's failure.
  discardView: async (sessionId: string) => {
    try {
      await nativeInvoke<void>("view_discard", { sessionId });
    } catch (error) {
      if (errorKind(error) === "locked") return;
      try { await lockSession(sessionId); } catch { /* the native fail-closed paths already warn */ }
    }
  },
  upgrade: () => invoke<Status>("vault_upgrade"),
  reload: () => invoke<Status>("vault_reload"),
  warnings: async () => {
    const report = await nativeInvoke<WarningReport>("vault_warnings");
    // This channel contains app warnings and fixed, origin-owned receipts, never protected vault results.
    const folded = receiveWarnings(report);
    if (folded.length > 0) void dismissNatively(folded);
  },
  dismissWarnings: async (ids: string[]) => {
    // ID-only acknowledgment is application-wide too; a newer vault must not suppress a completed dismissal.
    await nativeInvoke<void>("vault_dismiss_warnings", { ids });
    acknowledgeNotices(ids);
  },

  keys: () => invoke<KeyView[]>("keys_list"),
  // A decline in the native window rejects with kind "declined": no value was shown.
  reveal: async (id: string) => {
    const value = await invoke<string | null>("key_reveal", { id });
    if (value === null) throw declined("No value was shown.");
    return value;
  },
  copy: async (id: string) => {
    const seconds = await invoke<number | null>("key_copy", { id });
    if (seconds === null) throw declined("Nothing was copied.");
    return seconds;
  },
  add: (draft: KeyDraft) => invoke<KeyView>("key_add", { draft }),
  update: (id: string, draft: KeyDraft) =>
    invoke<KeyView>("key_update", { id, draft }),
  remove: (id: string) => invoke<void>("key_remove", { id }),
  rotate: (id: string, secret: string) => invoke<RotationReceipt>("key_rotate", { id, secret }),
  // Times and the names of changed fields. A value never comes with them.
  history: (id: string) => invoke<HistoryVersion[]>("key_history", { id }),
  recycled: () => invoke<RecycledKey[]>("recycled_list"),
  restore: (id: string) => invoke<KeyView>("key_restore", { id }),
  purge: (id: string) => invoke<void>("key_purge", { id }),

  audit: (limit = 300) => invoke<AuditReport>("audit_read", { limit }),
  // Starts a new journal after damage. Native asks the user in its own window and keeps the damaged file. null means that the user declined there.
  recoverJournal: () => invoke<JournalRecovery | null>("audit_recover"),
  // Native shows the scope and the names and asks first. null means that the user declined there.
  exportEnv: (project: string | null, environment: string | null) =>
    invoke<string | null>("env_export", { project, environment, all: false }),
  copyEnv: (project: string | null, environment: string | null) =>
    invoke<number | null>("env_copy", { project, environment, all: false }),
  saveEnv: (
    project: string | null,
    environment: string | null,
  ) => invoke<string | null>("env_save", { project, environment, all: false }),
  // The page names a scope. Native code asks for the folder, shows the names, and starts the system command
  // prompt. The reply is the number of keys the terminal holds, or null when the user cancelled there.
  launchTerminal: (project: string, environment: string) =>
    invoke<number | null>("env_launch_terminal", { project, environment }),
  // One request. A native window lists the names and the scope; null means that the user declined there.
  importEnv: (
    text: string,
    project: string,
    environment: string,
  ): Promise<ImportReport | null> => {
    const sessionId = nativeOwner();
    if (sessionId === null) return Promise.reject({ kind: "locked", message: "The session changed. Unlock the vault again." });
    const operationId = crypto.randomUUID();
    return invoke<ImportReport | null>("env_import", { sessionId, operationId, text, project, environment }, undefined, operationId);
  },
  backup: () => invoke<BackupInfo>("backup_info"),

  // The notification text is built natively from this count. The page cannot send any other text. The session names the vault the count was made in; native refuses it if that session is no longer the current one.
  remindersStatus: () => invoke<ReminderStatus>("reminders_status"),
  remindersSet: (enabled: boolean) => invoke<ReminderStatus>("reminders_set", { enabled }),
  remindersNotify: (count: number, sessionId: string) => invoke<ReminderStatus>("reminders_notify", { sessionId, count }),
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
