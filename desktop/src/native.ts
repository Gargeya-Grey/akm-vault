import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { errorKind } from "./errors";
import { beginOutcome, completedWhileLocked, reportCompletion, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { acknowledgeNotices, receiveWarnings } from "./operationNotices";
import { acceptNativeStatus, invalidateNativeView, nativeNoticeStamp, nativeOwner } from "./nativeSession";
import type { AuditReport, BackupInfo, HistoryVersion, KeyView, RecycledKey, ReminderStatus, Status, WarningReport } from "./types";

/**
 * The one boundary with native code. Only the files that `readGuard.test.ts` names may import this
 * module: the actions in api.ts, the read mechanism in nativeRead.ts, the session poll in App.tsx
 * and the count notifier. A view reads native state only through `useNativeRead`.
 */

const mutations = new Set(["vault_create", "vault_upgrade", "key_add", "key_update", "key_remove", "key_rotate", "key_restore", "key_purge", "env_import", "env_save", "env_launch_terminal", "audit_recover"]);
// A request that the user can still decline in a native window answers null when they do.
// The disclosing commands are on the list too: a page can read its session ID, so native asks the
// user before it shows or copies a value (the bulk commands always, the single-key commands once a
// session is over its budget). A decline gives no value and writes nothing to the clipboard.
const declinable = new Set(["env_import", "env_save", "env_launch_terminal", "key_reveal", "key_copy", "env_export", "env_copy", "audit_recover"]);
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

/** A call of a native command with the page's session rules. `operation` is the ID of a native outcome record. */
export async function invoke<T>(command: string, args?: Record<string, unknown>, expectedOwner?: string, operation?: string): Promise<T> {
  const generation = sessionGeneration();
  const bootstrap = command === "vault_status" && nativeOwner() === null;
  const notices = nativeNoticeStamp();
  // A terminal that opened cannot be taken back, so its receipt exists before the request leaves.
  const outcome = command === "env_import" ? beginOutcome("import", generation, operation)
    : command === "env_launch_terminal" ? beginOutcome("terminal", generation, operation)
    : command === "env_save" ? beginOutcome("export", generation) : undefined;
  const cancelled = () => ({ kind: "locked", message: "The session changed. Unlock the vault again." });
  let knownSuccess = false;
  try {
    const result = await nativeInvoke<T>(command, withSession(command, args));
    const declined = declinable.has(command) && result === null;
    knownSuccess = !declined;
    if (declined && outcome !== undefined) settleOutcome(outcome, "cancelled");
    if (operation !== undefined) acknowledgeRecord(operation);
    if (generation !== sessionGeneration()) {
      if (mutations.has(command) && !declined) throw completedWhileLocked();
      throw cancelled();
    }
    if (statusReplies.has(command)) acceptNativeStatus(result as Status, expectedOwner);
    if (outcome !== undefined) settleOutcome(outcome, "completed");
    return result;
  } catch (error) {
    if (operation !== undefined && errorKind(error) !== "unknown") acknowledgeRecord(operation);
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

/** A call that bypasses the session rules: a discard that names its own session, and acknowledgements. */
export function rawInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return nativeInvoke<T>(command, args);
}

// A reply that came from the native command is the outcome, so its native record is redundant.
// A transport failure leaves the record in place: it may be the only truth about the request.
function acknowledgeRecord(operation: string) {
  acknowledgeNotices([operation]);
  void dismissNatively([operation]);
}

async function dismissNatively(ids: string[]) {
  try { await nativeInvoke<void>("vault_dismiss_warnings", { ids }); } catch { /* the record stays and is offered again */ }
}

/** The reads of native state. Only the files that may import this module call them. */
export const reads = {
  status: () => invoke<Status>("vault_status"),
  warnings: async () => {
    const report = await nativeInvoke<WarningReport>("vault_warnings");
    // This channel contains app warnings and fixed, origin-owned receipts, never protected vault results.
    const folded = receiveWarnings(report);
    if (folded.length > 0) void dismissNatively(folded);
  },
  keys: () => invoke<KeyView[]>("keys_list"),
  recycled: () => invoke<RecycledKey[]>("recycled_list"),
  audit: (limit = 300) => invoke<AuditReport>("audit_read", { limit }),
  // Times and the names of changed fields. A value never comes with them.
  history: (id: string) => invoke<HistoryVersion[]>("key_history", { id }),
  backup: () => invoke<BackupInfo>("backup_info"),
  remindersStatus: () => invoke<ReminderStatus>("reminders_status"),
};
