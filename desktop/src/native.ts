import { invoke as nativeInvoke } from "@tauri-apps/api/core";
import { errorText } from "./errors";
import { beginOutcome, completedWhileLocked, reportCompletion, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { acknowledgeNotices, receiveWarnings } from "./operationNotices";
import { acceptNativeStatus, invalidateNativeView, nativeNoticeStamp, nativeOwner } from "./nativeSession";
import { NATIVE_ERROR_KINDS, type AuditReport, type BackupInfo, type HistoryVersion, type KeyView, type NativeError, type NativeErrorKind, type RecycledKey, type ReminderStatus, type Status, type TransportFailure, type WarningReport, type Dispatched } from "./types";

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

const nativeKinds: ReadonlySet<string> = new Set(NATIVE_ERROR_KINDS);

/**
 * Where a rejection came from (U2-R1-07). A native error reply is a plain object with a string
 * message and a kind on the closed list in types.ts. Any other shape is a transport failure: no
 * native command sent it, so the page cannot know what the request did. Only `kind` and `message`
 * pass on, so nothing else that a rejection carries reaches the screen.
 */
function provenance(rejection: unknown): NativeError | TransportFailure {
  if (Object.prototype.toString.call(rejection) === "[object Object]") {
    const { kind, message } = rejection as Record<string, unknown>;
    if (typeof kind === "string" && nativeKinds.has(kind) && typeof message === "string") return { kind: kind as NativeErrorKind, message };
  }
  return { kind: "transport", message: errorText(rejection) };
}

/**
 * A call of a native command with the page's session rules. `operation` is the ID of a native
 * outcome record. The reply carries the ID of its receipt, or null when the command has none.
 */
export function invoke<T>(command: string, args?: Record<string, unknown>, expectedOwner?: string, operation?: string): Dispatched<T> {
  const generation = sessionGeneration();
  // A terminal that opened cannot be taken back, so its receipt exists before the request leaves.
  const outcome = command === "env_import" ? beginOutcome("import", generation, operation)
    : command === "env_launch_terminal" ? beginOutcome("terminal", generation, operation)
    : command === "env_save" ? beginOutcome("export", generation) : undefined;
  return Object.assign(call<T>(command, args, expectedOwner, operation, generation, outcome), { receipt: outcome ?? null });
}

async function call<T>(command: string, args: Record<string, unknown> | undefined, expectedOwner: string | undefined, operation: string | undefined, generation: number, outcome: number | undefined): Promise<T> {
  const bootstrap = command === "vault_status" && nativeOwner() === null;
  const notices = nativeNoticeStamp();
  const cancelled = () => ({ kind: "locked", message: "The session changed. Unlock the vault again." });
  const committedWhileLocked = () => {
    if (outcome !== undefined) settleOutcome(outcome, "completedWhileLocked");
    else reportCompletion(generation);
    return completedWhileLocked(); // Do not forward native details, even in a known-commit error.
  };
  const failed = (error: unknown, settled: "lost" | "reportedError") => {
    // A bootstrap error carries no owner, unlike a successful status. If a native
    // notice overtook it, its details cannot be attributed safely. A success for
    // a different, non-tombstoned owner is still accepted, not blanket-dropped.
    if (bootstrap && generation === sessionGeneration() && notices !== nativeNoticeStamp()) invalidateNativeView();
    const obsolete = generation !== sessionGeneration();
    if (outcome !== undefined) settleOutcome(outcome, obsolete ? "unconfirmed" : settled, settled === "lost" ? "transport" : "reply");
    return obsolete ? cancelled() : error;
  };
  let result: T;
  try {
    result = await nativeInvoke<T>(command, withSession(command, args));
  } catch (rejection) {
    const error = provenance(rejection);
    // A native reply is the outcome, so its record is redundant. A transport failure leaves the
    // record in place: it may be the only truth about the request.
    if (operation !== undefined && error.kind !== "transport") acknowledgeRecord(operation);
    if (error.kind === "completedWhileLocked") throw committedWhileLocked();
    throw failed(error, error.kind === "transport" ? "lost" : "reportedError");
  }
  const declined = declinable.has(command) && result === null;
  if (declined && outcome !== undefined) settleOutcome(outcome, "cancelled");
  if (operation !== undefined) acknowledgeRecord(operation);
  if (generation !== sessionGeneration()) {
    if (mutations.has(command) && !declined) throw committedWhileLocked();
    throw cancelled();
  }
  if (statusReplies.has(command)) {
    try {
      acceptNativeStatus(result as Status, expectedOwner);
    } catch (error) {
      // The vault already changed, so a refused status after a mutation is still a commit.
      if (mutations.has(command)) throw committedWhileLocked();
      throw failed(error, "reportedError");
    }
  }
  if (outcome !== undefined) settleOutcome(outcome, "completed");
  return result;
}

/** A call that bypasses the session rules: a discard that names its own session, and acknowledgements. */
export function rawInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return nativeInvoke<T>(command, args);
}

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
