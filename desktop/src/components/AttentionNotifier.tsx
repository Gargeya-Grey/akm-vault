import { useEffect } from "react";
import { api } from "../api";
import { nativeOwner } from "../nativeSession";
import { attentionOf, canNotify, nextAnnouncement } from "../reminders";

/** Time between two looks. A hidden page may run slower; a look is cheap and reads no secret. */
export const LOOK_EVERY_MS = 30_000;

const inFront = () => document.visibilityState === "visible" && document.hasFocus();

/**
 * Sends the opt-in notification. Mounted only while the vault is unlocked, so a lock removes it
 * and its timer. It asks the native layer whether the setting is on and not blocked, reads the keys
 * only then, and sends a count. A look is not user activity: `reminders_status` and `keys_list`
 * do not extend the idle deadline (contract 6). It never reads or sends a title.
 */
export function AttentionNotifier() {
  useEffect(() => {
    let disposed = false;
    let looking = false;
    let announced = 0;
    const look = async () => {
      if (looking) return;
      looking = true;
      try {
        const session = nativeOwner();
        if (session === null) return;
        const status = await api.remindersStatus();
        if (disposed || !canNotify(status)) return;
        const keys = await api.keys();
        if (disposed) return;
        const count = attentionOf(keys, Date.now()).total;
        const step = nextAnnouncement(announced, count, inFront());
        announced = step.announced;
        if (step.notify) await api.remindersNotify(count, session);
      } catch {
        // A locked session, a missing native command or a refused notification: the native side keeps the state.
      } finally {
        looking = false;
      }
    };
    void look();
    const timer = window.setInterval(() => { void look(); }, LOOK_EVERY_MS);
    document.addEventListener("visibilitychange", look);
    window.addEventListener("focus", look);
    window.addEventListener("blur", look);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", look);
      window.removeEventListener("focus", look);
      window.removeEventListener("blur", look);
    };
  }, []);
  return null;
}
