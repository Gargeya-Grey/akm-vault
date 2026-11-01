import type { KeyView } from "./types";

export function formatDate(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    // Expiry dates are date-only values stored at midnight UTC. Formatting in
    // the local zone would show the previous day west of UTC.
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

export function daysUntil(iso: string): number {
  const target = new Date(iso).getTime();
  const now = Date.now();
  return Math.ceil((target - now) / 86_400_000);
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

export function keyState(key: KeyView): { label: string; tone: KeyTone } {
  if (key.expired) return { label: "Expired", tone: "warn" };
  if (key.rotationDueIn !== null && key.rotationDueIn < 0) {
    return { label: "Rotation overdue", tone: "warn" };
  }
  if (key.expiresAt && daysUntil(key.expiresAt) <= 30) {
    return { label: "Expires soon", tone: "warn" };
  }
  if (key.rotationDueIn !== null && key.rotationDueIn <= 14) {
    return { label: "Rotate soon", tone: "warn" };
  }
  return { label: "OK", tone: "ok" };
}

export function needsAttention(key: KeyView): boolean {
  return keyState(key).tone === "warn";
}

export function expiryLabel(key: KeyView): string {
  if (!key.expiresAt) return "—";
  return formatDate(key.expiresAt);
}

export function rotationLabel(key: KeyView): string {
  if (key.rotationDueIn === null) return "—";
  if (key.rotationDueIn < 0) return "overdue";
  if (key.rotationDueIn === 0) return "due today";
  return `in ${key.rotationDueIn} days`;
}
