export const COMPLETION_EVENT = "akm-operation-completed";
export const COMPLETION_MESSAGE = "An operation completed, but its originating session is no longer available. Do not repeat it. Open the relevant vault or check its export to inspect the saved result.";

let generation = 0;

export function sessionGeneration() { return generation; }
export function invalidateSession() { generation += 1; }

export function completedWhileLocked() {
  return { kind: "completedWhileLocked", message: COMPLETION_MESSAGE };
}

export function reportCompletion(origin = sessionGeneration()) {
  window.dispatchEvent(new CustomEvent(COMPLETION_EVENT, { detail: { generation: origin } }));
}
