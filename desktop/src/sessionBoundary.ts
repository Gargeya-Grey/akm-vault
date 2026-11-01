export const COMPLETION_EVENT = "akm-operation-completed";
export const SESSION_LOSS_EVENT = "akm-completed-session-lost";
export const WARNING_EVENT = "akm-operation-notices";
export const COMPLETION_MESSAGE = "An operation completed, but its originating session is no longer available. Do not repeat it. Open the relevant vault or check its export to inspect the saved result.";

export type Operation = "import" | "export" | "operation";
export type Outcome = "pending" | "completed" | "reportedError" | "cancelled" | "unconfirmed" | "completedWhileLocked";
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

export function retainedCompletions() { return [...receipts.values()].map((receipt) => ({ ...receipt })); }
export function acknowledgeCompletions(ids: number[]) {
  for (const id of ids) {
    if (receipts.get(id)?.outcome !== "pending") receipts.delete(id);
  }
  // An acknowledgment updates presentation, never locks a session.
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
}

export function sessionGeneration() { return generation; }
export function invalidateSession() { generation += 1; }

export function completedWhileLocked() {
  return { kind: "completedWhileLocked", message: COMPLETION_MESSAGE };
}

export function beginOutcome(operation: "import" | "export", origin: number) {
  const id = ++nextReceipt;
  receipts.set(id, { id, generation: origin, operation, outcome: "pending" });
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
  return id;
}

export function settleOutcome(id: number, outcome: Exclude<Outcome, "pending">) {
  const pending = receipts.get(id);
  if (!pending || pending.outcome !== "pending") return;
  receipts.set(id, { ...pending, outcome });
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
  if (outcome === "completedWhileLocked") signalSessionLoss(pending.generation);
}

function signalSessionLoss(origin: number) {
  // This is a live signal, not a replayable outcome-state update.
  window.dispatchEvent(new CustomEvent(SESSION_LOSS_EVENT, { detail: { generation: origin } }));
}

export function reportCompletion(origin = sessionGeneration()) {
  const receipt: OperationOutcome = { id: ++nextReceipt, generation: origin, operation: "operation", outcome: "completedWhileLocked" };
  receipts.set(receipt.id, receipt);
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
  signalSessionLoss(origin);
}
