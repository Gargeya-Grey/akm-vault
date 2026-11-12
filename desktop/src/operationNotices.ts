import type { ApplicationNotice, OperationNotice, WarningReport } from "./types";
import { settleNativeOutcome, WARNING_EVENT } from "./sessionBoundary";
import type { NativeOutcome } from "./sessionBoundary";

// The native record of a dispatched request, and the outcome it gives the receipt of that request.
// A record settles only a receipt of its own operation.
const NATIVE_OUTCOMES: Partial<Record<OperationNotice["kind"], NativeOutcome>> = {
  importApplied: { operation: "import", outcome: "completed" },
  importSessionEnded: { operation: "import", outcome: "completedWhileLocked" },
  importFailed: { operation: "import", outcome: "reportedError" },
  importDeclined: { operation: "import", outcome: "cancelled" },
  terminalStarted: { operation: "terminal", outcome: "completed" },
  terminalSessionEnded: { operation: "terminal", outcome: "completedWhileLocked" },
  terminalFailed: { operation: "terminal", outcome: "reportedError" },
  terminalDeclined: { operation: "terminal", outcome: "cancelled" },
};

// Fixed-payload operation notices and separate app-global warnings, never protected vault results.
// Native retention handles webview reload; this cache handles replies arriving between React root mounts.
const pending = new Map<string, OperationNotice>();
const applicationWarnings = new Map<string, ApplicationNotice>();
const dismissed = new Set<string>();

export function retainedOperationNotices() { return [...pending.values()]; }
export function retainedApplicationWarnings() { return [...applicationWarnings.values()]; }

// Returns the ids of native records that settled a waiting receipt. The caller acknowledges them natively.
export function receiveWarnings(report: WarningReport): string[] {
  const folded: string[] = [];
  for (const { id, originSession, kind } of report.operations) {
    if (dismissed.has(id)) continue;
    const outcome = NATIVE_OUTCOMES[kind];
    if (outcome !== undefined && settleNativeOutcome(id, outcome)) {
      dismissed.add(id);
      folded.push(id);
      continue;
    }
    pending.set(id, { id, originSession, kind });
  }
  for (const { id, message } of report.application) {
    if (!dismissed.has(id)) applicationWarnings.set(id, { id, message });
  }
  announce();
  return folded;
}

export function acknowledgeNotices(ids: string[]) {
  for (const id of ids) { dismissed.add(id); pending.delete(id); applicationWarnings.delete(id); }
  announce();
}

function announce() {
  window.dispatchEvent(new CustomEvent<WarningReport>(WARNING_EVENT, { detail: { application: retainedApplicationWarnings(), operations: retainedOperationNotices() } }));
}
