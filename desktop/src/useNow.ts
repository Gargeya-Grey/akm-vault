import { useEffect, useState } from "react";
import { nextChangeAt } from "./schedule";
import type { KeyView } from "./types";

/** The longest wait between two looks at the wall clock. A clock change is noticed within this time. */
export const MAX_WAIT_MS = 60_000;
const MIN_WAIT_MS = 50;
const MARGIN_MS = 20;

/**
 * The current time, refreshed so that the expired and rotation states of `keys` never go stale
 * while the list stays open. It reads the wall clock only. It calls nothing native, so a refresh
 * is not user activity and never extends the idle deadline (contract 6). It looks again at the
 * next moment that a state can change, after the page becomes visible or focused again, and at
 * least once per `MAX_WAIT_MS`, which also catches a changed system clock.
 */
export function useNow(keys: readonly KeyView[]): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let timer: number | undefined;
    const arm = () => {
      window.clearTimeout(timer);
      timer = undefined;
      const current = Date.now();
      const next = nextChangeAt(keys, current);
      if (next === null) return;
      const wait = Math.min(Math.max(next - current + MARGIN_MS, MIN_WAIT_MS), MAX_WAIT_MS);
      timer = window.setTimeout(refresh, wait);
    };
    function refresh() {
      setNow(Date.now());
      arm();
    }
    const onVisibility = () => {
      if (!document.hidden) refresh();
    };
    refresh();
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("focus", refresh);
    };
  }, [keys]);

  return now;
}
