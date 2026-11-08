import { keyState, plural } from "./format";
import type { KeyStateKind } from "./format";
import type { KeyView, ReminderStatus } from "./types";

/**
 * The keys that need attention at one moment, each under its main state (`keyState`). The summary,
 * the "need attention" count and the notification count all come from here, so they cannot
 * disagree with the Keys screen.
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

/** One line of the summary: the state, the count and up to five titles. Titles only, never a value. */
export function summaryLine(label: string, keys: readonly KeyView[]): string {
  const shown = keys.slice(0, SUMMARY_TITLES_PER_GROUP).map((key) => key.title);
  const rest = keys.length - shown.length;
  return `${label} (${keys.length}): ${shown.join(", ")}${rest > 0 ? `, and ${rest} more` : ""}`;
}

/** Whether the page should look at the keys for a notification at all. The native side checks again. */
export function canNotify(status: ReminderStatus | null | undefined): boolean {
  return Boolean(status && status.enabled && status.permission === "granted" && !status.blocked);
}

/**
 * What to do when the number of keys that need attention is `count`. `announced` is the highest
 * count that the user already knows about in this unlock session: the summary and the Keys screen
 * show it while the window is in front, and a notification said it while the window was behind.
 * A notification is sent only for a rise above `announced` while the window is not in front.
 */
export function nextAnnouncement(announced: number, count: number, foreground: boolean): { announced: number; notify: boolean } {
  if (count > announced) return { announced: count, notify: !foreground };
  return { announced: count, notify: false };
}

export const REMINDER_NOTE =
  "The notification says only how many keys need attention. It names no key, project or environment. AKM runs nothing in the background: it can show one only while AKM is open and unlocked.";

/** The words for each state of the setting. Windows decides whether a toast shows; AKM cannot see that. */
export function reminderText(status: ReminderStatus): string {
  if (!status.enabled) return "Off. AKM shows a summary after you unlock the vault. It sends no Windows notification.";
  if (status.permission !== "granted") {
    return "On, but Windows has not allowed notifications for AKM. AKM does not try again. Allow them in the Windows notification settings for AKM, then turn this setting off and on.";
  }
  if (status.delivery === "failed" || status.blocked) {
    return "On, but Windows did not accept the last notification. AKM does not try again in this run. Turn this setting off and on to try again.";
  }
  if (status.delivery === "sent") {
    return "On. The last notification was handed to Windows. AKM cannot see whether Windows showed it: Focus assist and the per-app notification switch decide that.";
  }
  return "On. AKM sends one count-only notification when more keys need attention while this window is in the background and the vault is unlocked.";
}
