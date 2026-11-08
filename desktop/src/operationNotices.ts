import type { ApplicationNotice, OperationNotice, WarningReport } from "./types";
import { settleNativeOutcome, WARNING_EVENT } from "./sessionBoundary";
import type { Outcome } from "./sessionBoundary";

const IMPORT_OUTCOMES: Partial<Record<OperationNotice["kind"], Exclude<Outcome, "pending">>> = {
  importApplied: "completed",
  importSessionEnded: "completedWhileLocked",
  importFailed: "reportedError",
  importDeclined: "cancelled",
};

// Fixed-payload operation notices and separate app-global warnings, never protected vault results.
// Native retention handles webview reload; this cache handles replies arriving between React root mounts.
const pending = new Map<string, OperationNotice>();
const applicationWarnings = new Map<string, ApplicationNotice>();
const dismissed = new Set<string>();

export function retainedOperationNotices() { return [...pending.values()]; }
export function retainedApplicationWarnings() { return [...applicationWarnings.values()]; }

// Returns the ids of native import records that settled a waiting receipt. The caller acknowledges them natively.
export function receiveWarnings(report: WarningReport): string[] {
  const folded: string[] = [];
  for (const { id, originSession, kind } of report.operations) {
    if (dismissed.has(id)) continue;
    const outcome = IMPORT_OUTCOMES[kind];
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
