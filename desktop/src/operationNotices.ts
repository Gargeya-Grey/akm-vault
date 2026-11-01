import type { ApplicationNotice, OperationNotice, WarningReport } from "./types";
import { WARNING_EVENT } from "./sessionBoundary";

// Fixed-payload operation notices and separate app-global warnings, never protected vault results.
// Native retention handles webview reload; this cache handles replies arriving between React root mounts.
const pending = new Map<string, OperationNotice>();
const applicationWarnings = new Map<string, ApplicationNotice>();
const dismissed = new Set<string>();

export function retainedOperationNotices() { return [...pending.values()]; }
export function retainedApplicationWarnings() { return [...applicationWarnings.values()]; }

export function receiveWarnings(report: WarningReport) {
  for (const { id, originSession, kind } of report.operations) {
    if (!dismissed.has(id)) pending.set(id, { id, originSession, kind });
  }
  for (const { id, message } of report.application) {
    if (!dismissed.has(id)) applicationWarnings.set(id, { id, message });
  }
  announce();
}

export function acknowledgeNotices(ids: string[]) {
  for (const id of ids) { dismissed.add(id); pending.delete(id); applicationWarnings.delete(id); }
  announce();
}

function announce() {
  window.dispatchEvent(new CustomEvent<WarningReport>(WARNING_EVENT, { detail: { application: retainedApplicationWarnings(), operations: retainedOperationNotices() } }));
}
