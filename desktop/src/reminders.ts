import { keyState, plural } from "./format";
import type { KeyStateKind } from "./format";
import type { KeyView } from "./types";

/**
 * The keys that need attention at one moment, each under its main state (`keyState`). The summary
 * and the "need attention" count both come from here, so they cannot disagree with the Keys
 * screen.
 */
export interface Attention {
  expired: KeyView[];
  overdue: KeyView[];
  /** Expires within EXPIRES_SOON_DAYS, or rotation falls due within ROTATE_SOON_DAYS. */
  dueSoon: KeyView[];
  /** A rotation interval and no known secret-change time. */
  unknown: KeyView[];
  total: number;
}

const GROUP: Partial<Record<KeyStateKind, "expired" | "overdue" | "dueSoon" | "unknown">> = {
  expired: "expired",
  overdue: "overdue",
  expiresSoon: "dueSoon",
  rotateSoon: "dueSoon",
  unknown: "unknown",
};

export function attentionOf(keys: readonly KeyView[], now: number): Attention {
  const result: Attention = { expired: [], overdue: [], dueSoon: [], unknown: [], total: 0 };
  for (const key of keys) {
    const group = GROUP[keyState(key, now).kind];
    if (group === undefined) continue;
    result[group].push(key);
    result.total += 1;
  }
  for (const group of [result.expired, result.overdue, result.dueSoon, result.unknown]) {
    group.sort((a, b) => a.title.localeCompare(b.title));
  }
  return result;
}

export const SUMMARY_TITLES_PER_GROUP = 5;

export function summaryHeading(total: number): string {
  return `${plural(total, "key")} ${total === 1 ? "needs" : "need"} attention`;
}

/**
 * One line of the summary: the state, the count and up to five keys, each named by `nameOf` (its
 * drawn title and its short ID, `namedKey`). Names only, never a value.
 */
export function summaryLine(label: string, keys: readonly KeyView[], nameOf: (key: KeyView) => string): string {
  const shown = keys.slice(0, SUMMARY_TITLES_PER_GROUP).map(nameOf);
  const rest = keys.length - shown.length;
  return `${label} (${keys.length}): ${shown.join(", ")}${rest > 0 ? `, and ${rest} more` : ""}`;
}
