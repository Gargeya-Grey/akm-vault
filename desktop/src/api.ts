import { errorKind } from "./errors";
import { invoke, rawInvoke } from "./native";
import { adoptReservation, beginNativeOpen, confirmNativeLock, endNativeOpen, invalidateNativeView, nativeCancellation, nativeOwner, releaseNativeOwner } from "./nativeSession";
import { sessionGeneration } from "./sessionBoundary";
import { acknowledgeNotices } from "./operationNotices";
import type { Dispatched, ImportReport, JournalRecovery, KeyDraft, KeyView, ReminderStatus, RotationReceipt, Sealed, Status } from "./types";

export { errorKind, errorText } from "./errors";

/**
 * The actions that the page can ask native code for. Each one changes state, asks native for an
 * effect, or ends a session. The reads of native state are not here: they live in native.ts, and a
 * view reaches them only through `useNativeRead` (readGuard.test.ts).
 */

function declined(what: string) {
  return { kind: "declined", message: `Cancelled. ${what}` };
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
  confirmNativeLock(sessionId, true);
}

export const api = {
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
      await rawInvoke<void>("view_discard", { sessionId });
    } catch (error) {
      if (errorKind(error) === "locked") return;
      try { await lockSession(sessionId); } catch { /* the native fail-closed paths already warn */ }
    }
  },
  upgrade: () => invoke<Status>("vault_upgrade"),
  reload: () => invoke<Status>("vault_reload"),
  dismissWarnings: async (ids: string[]) => {
    // ID-only acknowledgment is application-wide too; a newer vault must not suppress a completed dismissal.
    await rawInvoke<void>("vault_dismiss_warnings", { ids });
    acknowledgeNotices(ids);
  },

  // A decline in the native window rejects with kind "declined": no value was shown. The value
  // arrives sealed: only `plaintext.tsx` draws it (U2-R1-04).
  reveal: async (id: string): Promise<Sealed> => {
    const value = await invoke<Sealed | null>("key_reveal", { id });
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
  restore: (id: string) => invoke<KeyView>("key_restore", { id }),
  purge: (id: string) => invoke<void>("key_purge", { id }),

  // Starts a new journal after damage. Native asks the user in its own window and keeps the damaged file. null means that the user declined there.
  recoverJournal: () => invoke<JournalRecovery | null>("audit_recover"),
  // Native shows the scope and the names and asks first. null means that the user declined there.
  exportEnv: (project: string | null, environment: string | null) =>
    invoke<Sealed | null>("env_export", { project, environment, all: false }),
  copyEnv: (project: string | null, environment: string | null) =>
    invoke<number | null>("env_copy", { project, environment, all: false }),
  saveEnv: (
    project: string | null,
    environment: string | null,
  ) => invoke<string | null>("env_save", { project, environment, all: false }),
  // The page names a scope. Native code asks for the folder, shows the names, and starts the system command
  // prompt. The reply is the number of keys the terminal holds, or null when the user cancelled there.
  // Native keeps a record under `operationId` until the page acknowledges it, so a lost reply still has an outcome.
  launchTerminal: (project: string, environment: string) => {
    const operationId = crypto.randomUUID();
    return invoke<number | null>("env_launch_terminal", { project, environment, operationId }, undefined, operationId);
  },
  // One request. A native window lists the names and the scope; null means that the user declined there.
  importEnv: (
    text: string,
    project: string,
    environment: string,
  ): Dispatched<ImportReport | null> => {
    const sessionId = nativeOwner();
    if (sessionId === null) return Object.assign(Promise.reject({ kind: "locked", message: "The session changed. Unlock the vault again." }), { receipt: null });
    const operationId = crypto.randomUUID();
    return invoke<ImportReport | null>("env_import", { sessionId, operationId, text, project, environment }, undefined, operationId);
  },

  // The notification text is built natively from this count. The page cannot send any other text. The session names the vault the count was made in; native refuses it if that session is no longer the current one.
  remindersSet: (enabled: boolean) => invoke<ReminderStatus>("reminders_set", { enabled }),
  remindersNotify: (count: number, sessionId: string) => invoke<ReminderStatus>("reminders_notify", { sessionId, count }),
};
