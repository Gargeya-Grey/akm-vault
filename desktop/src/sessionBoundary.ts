import { useSyncExternalStore } from "react";

export const COMPLETION_EVENT = "akm-operation-completed";
export const SESSION_LOSS_EVENT = "akm-completed-session-lost";
export const WARNING_EVENT = "akm-operation-notices";
export const COMPLETION_MESSAGE = "An operation completed, but its originating session is no longer available. Do not repeat it. Open the relevant vault or check its export to inspect the saved result.";

export type Operation = "import" | "export" | "terminal" | "operation";
// "lost": the request failed in transport, so the page cannot know what it did. Its native record,
// if one arrives, stays a notice of its own.
export type Outcome = "pending" | "completed" | "reportedError" | "cancelled" | "unconfirmed" | "completedWhileLocked" | "lost";
export interface OperationOutcome {
  id: number;
  generation: number;
  operation: Operation;
  outcome: Outcome;
}

let generation = 0;
let nextReceipt = 0;
// Only opaque identity/classification lives here. Never retain native input, results or errors.
const receipts = new Map<number, OperationOutcome>();
// Opaque native operation ids of dispatched requests, so a native outcome record can settle a receipt whose reply was lost.
const nativeOperations = new Map<string, number>();
/** Who settled a receipt: its reply, its native record, or a failure in transport. */
export type SettledBy = "reply" | "record" | "transport";
const settledBy = new Map<number, SettledBy>();
// One number per change of the receipts, so a view can render again when one changes.
let receiptsVersion = 0;
function announce() {
  receiptsVersion += 1;
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
}

export function retainedCompletions() { return [...receipts.values()].map((receipt) => ({ ...receipt })); }
export function acknowledgeCompletions(ids: number[]) {
  for (const id of ids) {
    if (receipts.get(id)?.outcome !== "pending") {
      receipts.delete(id);
      settledBy.delete(id);
      for (const [operation, receipt] of nativeOperations) if (receipt === id) nativeOperations.delete(operation);
    }
  }
  // An acknowledgment updates presentation, never locks a session.
  announce();
}

export function sessionGeneration() { return generation; }
export function invalidateSession() {
  generation += 1;
  stalePlaintext();
}

/** Every plaintext that the page holds goes stale on this event (U2-R1-04, contract 4). */
export const PLAINTEXT_EVENT = "akm-plaintext-stale";
// The generation of the vault data. A change of the data, when it leaves and when it ends, makes
// every plaintext that the page holds stale. `sealed.ts` binds each plaintext to it.
let data = 0;
let plaintext = 0;
function stalePlaintext() {
  plaintext += 1;
  window.dispatchEvent(new CustomEvent(PLAINTEXT_EVENT));
}
export function dataGeneration() { return data; }
export function changeData() {
  data += 1;
  stalePlaintext();
}
/** One number per event that can make a plaintext stale, so a view renders again on it. */
export function plaintextVersion() { return plaintext; }

export function completedWhileLocked() {
  return { kind: "completedWhileLocked", message: COMPLETION_MESSAGE };
}

export function beginOutcome(operation: "import" | "export" | "terminal", origin: number, nativeOperation?: string) {
  const id = ++nextReceipt;
  receipts.set(id, { id, generation: origin, operation, outcome: "pending" });
  if (nativeOperation !== undefined) nativeOperations.set(nativeOperation, id);
  announce();
  return id;
}

export function settleOutcome(id: number, outcome: Exclude<Outcome, "pending">, by: SettledBy = "reply") {
  const pending = receipts.get(id);
  if (!pending || pending.outcome !== "pending") return;
  receipts.set(id, { ...pending, outcome });
  settledBy.set(id, by);
  announce();
  if (outcome === "completedWhileLocked") signalSessionLoss(pending.generation);
}

/**
 * The one owner of the state of a dispatched operation (U2-R1-03). A form keeps no waiting flag
 * of its own: it reads the outcome of its receipt here, and who settled it.
 */
export function receiptState(id: number | null): { outcome: Outcome; by: SettledBy | null } | null {
  const receipt = id === null ? undefined : receipts.get(id);
  return receipt === undefined ? null : { outcome: receipt.outcome, by: settledBy.get(receipt.id) ?? null };
}

/** The newest receipt of `operation` that waits and belongs to the current session, or null. */
export function waitingReceipt(operation: "import" | "export" | "terminal"): number | null {
  let found: number | null = null;
  for (const receipt of receipts.values()) {
    if (receipt.operation === operation && receipt.outcome === "pending" && receipt.generation === generation) found = receipt.id;
  }
  return found;
}

function subscribe(listener: () => void) {
  window.addEventListener(COMPLETION_EVENT, listener);
  return () => window.removeEventListener(COMPLETION_EVENT, listener);
}

/** Renders the caller again whenever a receipt changes. */
export function useReceipts(): number {
  return useSyncExternalStore(subscribe, () => receiptsVersion);
}

export interface NativeOutcome {
  operation: "import" | "terminal";
  outcome: Exclude<Outcome, "pending">;
}

// A native outcome record settles only a receipt of its own operation that is still waiting for its reply.
export function settleNativeOutcome(nativeOperation: string, { operation, outcome }: NativeOutcome): boolean {
  const id = nativeOperations.get(nativeOperation);
  const receipt = id === undefined ? undefined : receipts.get(id);
  if (id === undefined || receipt?.outcome !== "pending" || receipt.operation !== operation) return false;
  // An import record settles what the reply would have settled, and the vault data changed too.
  if (operation === "import") changeData();
  settleOutcome(id, outcome, "record");
  return true;
}

function signalSessionLoss(origin: number) {
  // This is a live signal, not a replayable outcome-state update.
  window.dispatchEvent(new CustomEvent(SESSION_LOSS_EVENT, { detail: { generation: origin } }));
}

export function reportCompletion(origin = sessionGeneration()) {
  const receipt: OperationOutcome = { id: ++nextReceipt, generation: origin, operation: "operation", outcome: "completedWhileLocked" };
  receipts.set(receipt.id, receipt);
  announce();
  signalSessionLoss(origin);
}
