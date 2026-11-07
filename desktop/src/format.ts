import { isPast, statusOf } from "./schedule";
import type { KeyView } from "./types";

export function formatDate(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    // A date-only expiry is the last second of its date in UTC (docs/design/unit3.md).
    // Formatting in the local zone would show the next day east of UTC.
    timeZone: "UTC",
  }).format(date);
}

export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export type KeyTone = "ok" | "warn";

/** The state of a key at `now` (milliseconds). Callers pass a clock that refreshes (see `useNow`). */
export function keyState(key: KeyView, now: number): { label: string; tone: KeyTone } {
  const { expiry, rotation } = statusOf(key, now);
  if (isPast(expiry)) return { label: "Expired", tone: "warn" };
  if (isPast(rotation)) return { label: "Rotation overdue", tone: "warn" };
  if (expiry?.state === "in" && expiry.days <= 30) return { label: "Expires soon", tone: "warn" };
  if (rotation?.state === "in" && rotation.days <= 14) return { label: "Rotate soon", tone: "warn" };
  if (key.rotationDays !== null && rotation === null) {
    // AKM has no secret-change time, so it cannot say that the key is on schedule.
    return { label: "Rotation unknown", tone: "warn" };
  }
  return { label: "OK", tone: "ok" };
}

export function needsAttention(key: KeyView, now: number): boolean {
  return keyState(key, now).tone === "warn";
}

export function isExpired(key: KeyView, now: number): boolean {
  return isPast(statusOf(key, now).expiry);
}

export function expiryLabel(key: KeyView): string {
  if (!key.expiresAt) return "—";
  return formatDate(key.expiresAt);
}

/** The time of the last secret change, or "unknown". AKM never invents a date. */
export function secretChangeLabel(key: KeyView): string {
  if (key.secretChange === "known" && key.secretChangedAt) return formatDateTime(key.secretChangedAt);
  return "unknown";
}

export function rotationLabel(key: KeyView, now: number): string {
  const { rotation } = statusOf(key, now);
  if (rotation === null) return key.rotationDays === null ? "—" : "unknown";
  if (rotation.state === "now") return "due now";
  if (rotation.state === "overdue") return "overdue";
  return `in ${plural(rotation.days, "day")}`;
}
