import { useLayoutEffect, useRef } from "react";

/**
 * The element that a chain of dialogs started from. A dialog that opens from a control inside
 * another dialog (Key Detail to Rotate secret) inherits that dialog's anchor, because the first
 * dialog and its controls are gone by the time the chain ends.
 */
const anchors = new WeakMap<Element, HTMLElement | null>();

/** Where focus lands when the element that opened a dialog no longer exists. One per view. */
const FALLBACK = "[data-focus-fallback]";

function currentOpener(): HTMLElement | null {
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const parent = active?.closest("[role=\"dialog\"]");
  if (parent && anchors.has(parent)) return anchors.get(parent) ?? null;
  return active === document.body ? null : active;
}

function focusLost(): boolean {
  const active = document.activeElement;
  return !active || active === document.body || !active.isConnected;
}

export function useDialogFocus<T extends HTMLElement>(open = true) {
  const ref = useRef<T>(null);
  const wasOpen = useRef(false);
  const opener = useRef<HTMLElement | null>(null);
  if (open && !wasOpen.current) opener.current = currentOpener();
  wasOpen.current = open;
  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!open || !dialog) return;
    const previous = opener.current;
    anchors.set(dialog, previous);
    const controls = () => [...dialog.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled):not([type="hidden"]), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])',
    )].filter((element) => !element.closest("[hidden], [inert]") && getComputedStyle(element).display !== "none");
    const focusFirst = () => (controls()[0] ?? dialog).focus();
    if (!dialog.contains(document.activeElement)) focusFirst();
    const onTab = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const items = controls();
      const first = items[0];
      const last = items.at(-1);
      if (!first || !last) { event.preventDefault(); dialog.focus(); return; }
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
        event.preventDefault(); last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) {
        event.preventDefault(); first.focus();
      }
    };
    const onFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && !dialog.contains(event.target)) focusFirst();
    };
    document.addEventListener("keydown", onTab, true);
    document.addEventListener("focusin", onFocus);
    return () => {
      document.removeEventListener("keydown", onTab, true);
      document.removeEventListener("focusin", onFocus);
      if (previous?.isConnected) previous.focus();
      // The opener can vanish in the same commit (a deleted row, an upgrade that removes its own
      // button). Once the commit is done, focus that is lost goes to the view's fallback target.
      queueMicrotask(() => {
        if (focusLost()) document.querySelector<HTMLElement>(FALLBACK)?.focus();
      });
    };
  }, [open]);
  return ref;
}
