export const COMPLETION_EVENT = "akm-operation-completed";
export const SESSION_LOSS_EVENT = "akm-completed-session-lost";
export const WARNING_EVENT = "akm-operation-notices";
export const COMPLETION_MESSAGE = "An operation completed, but its originating session is no longer available. Do not repeat it. Open the relevant vault or check its export to inspect the saved result.";

export type Operation = "import" | "export" | "terminal" | "operation";
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
// Opaque native operation ids of dispatched requests, so a native outcome record can settle a receipt whose reply was lost.
const nativeOperations = new Map<string, number>();

export function retainedCompletions() { return [...receipts.values()].map((receipt) => ({ ...receipt })); }
export function acknowledgeCompletions(ids: number[]) {
  for (const id of ids) {
    if (receipts.get(id)?.outcome !== "pending") {
      receipts.delete(id);
      for (const [operation, receipt] of nativeOperations) if (receipt === id) nativeOperations.delete(operation);
    }
  }
  // An acknowledgment updates presentation, never locks a session.
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT));
}

export function sessionGeneration() { return generation; }
export function invalidateSession() { generation += 1; }

export function completedWhileLocked() {
  return { kind: "completedWhileLocked", message: COMPLETION_MESSAGE };
}

export function beginOutcome(operation: "import" | "export" | "terminal", origin: number, nativeOperation?: string) {
  const id = ++nextReceipt;
  receipts.set(id, { id, generation: origin, operation, outcome: "pending" });
  if (nativeOperation !== undefined) nativeOperations.set(nativeOperation, id);
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

// A native outcome record settles only a receipt that is still waiting for its reply.
export function settleNativeOutcome(nativeOperation: string, outcome: Exclude<Outcome, "pending">): boolean {
  const id = nativeOperations.get(nativeOperation);
  if (id === undefined || receipts.get(id)?.outcome !== "pending") return false;
  settleOutcome(id, outcome);
  return true;
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
