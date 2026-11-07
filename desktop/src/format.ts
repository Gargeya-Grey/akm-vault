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

/** A key is "soon" when its expiry is within this many days. */
export const EXPIRES_SOON_DAYS = 30;
/** A key is "soon" when its rotation falls due within this many days. */
export const ROTATE_SOON_DAYS = 14;

/**
 * The one state of a key that the list, the detail, the summary and the notification share.
 * The order is the order of precedence: a key that is expired and also overdue is "expired".
 */
export type KeyStateKind = "expired" | "overdue" | "expiresSoon" | "rotateSoon" | "unknown" | "ok";

const STATE_LABELS: Record<KeyStateKind, { label: string; tone: KeyTone }> = {
  expired: { label: "Expired", tone: "warn" },
  overdue: { label: "Rotation overdue", tone: "warn" },
  expiresSoon: { label: "Expires soon", tone: "warn" },
  rotateSoon: { label: "Rotate soon", tone: "warn" },
  // AKM has no secret-change time, so it cannot say that the key is on schedule.
  unknown: { label: "Rotation unknown", tone: "warn" },
  ok: { label: "OK", tone: "ok" },
};

function stateKind(key: KeyView, now: number): KeyStateKind {
  const { expiry, rotation } = statusOf(key, now);
  if (isPast(expiry)) return "expired";
  if (isPast(rotation)) return "overdue";
  if (expiry?.state === "in" && expiry.days <= EXPIRES_SOON_DAYS) return "expiresSoon";
  if (rotation?.state === "in" && rotation.days <= ROTATE_SOON_DAYS) return "rotateSoon";
  if (key.rotationDays !== null && rotation === null) return "unknown";
  return "ok";
}

/** The state of a key at `now` (milliseconds). Callers pass a clock that refreshes (see `useNow`). */
export function keyState(key: KeyView, now: number): { kind: KeyStateKind; label: string; tone: KeyTone } {
  const kind = stateKind(key, now);
  return { kind, ...STATE_LABELS[kind] };
}

export function needsAttention(key: KeyView, now: number): boolean {
  return keyState(key, now).tone === "warn";
}

export function isExpired(key: KeyView, now: number): boolean {
  return isPast(statusOf(key, now).expiry);
}

function days(count: number): string {
  return plural(count, "day");
}

/** What a screen reader hears about the expiry of a key. Whole days are for display only. */
export function expirySentence(key: KeyView, now: number): string {
  const { expiry } = statusOf(key, now);
  if (expiry === null) return "No expiry date";
  if (expiry.state === "in") return `Expires in ${days(expiry.days)}`;
  if (expiry.state === "now") return "Expires now";
  return expiry.days === 0 ? "Expired less than a day ago" : `Expired ${days(expiry.days)} ago`;
}

/** What a screen reader hears about the rotation of a key. An unknown change time is never "on schedule". */
export function rotationSentence(key: KeyView, now: number): string {
  if (key.rotationDays === null) return "No rotation interval";
  const { rotation } = statusOf(key, now);
  if (rotation === null) return "Rotation unknown, no known secret-change time";
  if (rotation.state === "in") return `Rotation due in ${days(rotation.days)}`;
  if (rotation.state === "now") return "Rotation due now";
  return rotation.days === 0 ? "Rotation overdue by less than a day" : `Rotation overdue by ${days(rotation.days)}`;
}

/** The accessible description of a key row: the expiry and the rotation status, in words. */
export function statusDescription(key: KeyView, now: number): string {
  return `${expirySentence(key, now)}. ${rotationSentence(key, now)}.`;
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
