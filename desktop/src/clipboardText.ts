// Clipboard cleanup is best effort (unit 1a, 1A-R8-02): every text says that the copy may remain.

export function clipboardCopyText(seconds: number): string {
  return `Copied. AKM tries to clear its copy after ${seconds} seconds, on lock and on normal exit. The copy may remain if the clipboard is busy or AKM stops unexpectedly.`;
}

export const CLIPBOARD_LOCK_TEXT =
  "Locking closes the current session and tries to clear AKM’s clipboard copy. The copy may remain if the clipboard is busy.";
