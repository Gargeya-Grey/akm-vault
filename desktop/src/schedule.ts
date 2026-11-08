import type { KeyView } from "./types";

// Mirrors crates/akm-core/src/schedule.rs. Both read crates/akm-core/tests/fixtures/time/cases.json,
// so the two sides cannot drift. Scheduling uses exact instants; whole days are for display only.

export const DAY_MS = 86_400_000;

export type Remaining =
  | { state: "in"; days: number }
  | { state: "now" }
  | { state: "overdue"; days: number };

/** How far a due instant is from now. `in` rounds up, so it is at least 1. `overdue` counts full days late. */
export function remainingOf(dueMs: number, nowMs: number): Remaining {
  const left = dueMs - nowMs;
  if (left === 0) return { state: "now" };
  if (left > 0) return { state: "in", days: Math.ceil(left / DAY_MS) };
  return { state: "overdue", days: Math.floor(-left / DAY_MS) };
}

export function isPast(remaining: Remaining | null): boolean {
  return remaining !== null && remaining.state !== "in";
}

/** A date-only expiry is the last second of that date in UTC, as in the CLI. */
export function dateOnlyExpiry(date: string): string {
  return `${date}T23:59:59Z`;
}

function instant(iso: string | null): number | null {
  if (!iso) return null;
  const value = new Date(iso).getTime();
  return Number.isNaN(value) ? null : value;
}

export interface Status {
  expiry: Remaining | null;
  rotation: Remaining | null;
}

/** The due times in a key view, as milliseconds. Rotation counts from a known change time only. */
export function dueTimes(key: KeyView): { expiresAt: number | null; rotationDueAt: number | null } {
  return { expiresAt: instant(key.expiresAt), rotationDueAt: instant(key.rotationDueAt) };
}

export function statusOf(key: KeyView, nowMs: number): Status {
  const { expiresAt, rotationDueAt } = dueTimes(key);
  return {
    expiry: expiresAt === null ? null : remainingOf(expiresAt, nowMs),
    rotation: rotationDueAt === null ? null : remainingOf(rotationDueAt, nowMs),
  };
}

/**
 * The next moment at which any shown state or day count of these keys can change, or null when
 * no key has a due time. A moment is the exact instant: when a day count drops, or when a due
 * time passes.
 */
export function nextChangeAt(keys: readonly KeyView[], nowMs: number): number | null {
  let next: number | null = null;
  const consider = (at: number) => {
    if (at > nowMs && (next === null || at < next)) next = at;
  };
  for (const key of keys) {
    const { expiresAt, rotationDueAt } = dueTimes(key);
    for (const due of [expiresAt, rotationDueAt]) {
      if (due === null) continue;
      const left = due - nowMs;
      if (left > 0) {
        const days = Math.ceil(left / DAY_MS);
        consider(due - (days - 1) * DAY_MS);
      } else if (left === 0) {
        consider(nowMs + 1);
      } else {
        consider(due + (Math.floor(-left / DAY_MS) + 1) * DAY_MS);
      }
    }
  }
  return next;
}
