import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { discard, lineCount, openFor } from "./sealed";
import { PLAINTEXT_EVENT, plaintextVersion } from "./sessionBoundary";
import type { Sealed } from "./types";

/**
 * The one way a view holds and draws plaintext from the vault (U2-R1-04, contract 4). A view gets
 * only sealed tokens from `api`. It keeps one in `useSealed` and draws it with `<Plaintext>`. The
 * text shows only while its session, the vault data and the scope that the view names are the
 * ones of its request. A lock or a new session, a change of vault data (an add, an edit, a
 * rotation, a removal, a restore, a purge, an import, an upgrade or a reload), a scope change, a
 * list read that fails or starts again (the view then names no scope), a hide and an unmount each
 * drop it. Only a new request shows plaintext again.
 */

/** The scope that a request of plaintext names, built the same way by `native.ts` and the view. */
export const plaintextScope = {
  reveal: (id: string) => JSON.stringify(["reveal", id]),
  export: (project: string | null, environment: string | null) => JSON.stringify(["export", project, environment]),
};

function subscribe(listener: () => void) {
  window.addEventListener(PLAINTEXT_EVENT, listener);
  return () => window.removeEventListener(PLAINTEXT_EVENT, listener);
}

/** Renders the caller again when the session or the vault data changes. */
function usePlaintextClock() {
  return useSyncExternalStore(subscribe, plaintextVersion);
}

export interface SealedState {
  /** The token, for `<Plaintext>`, while it may show. It holds no text that a view can read. */
  value: Sealed | null;
  /** True while the value may show under the scope. */
  shown: boolean;
  /**
   * Keeps a new token and drops the one before it. It answers the number of lines of the new
   * value that are not blank (a count, never the text), or null when no value may show.
   */
  hold: (token: Sealed | null) => number | null;
}

/** The holder of one plaintext in a view. `scope` is null while the view may show none. */
export function useSealed(scope: string | null): SealedState {
  usePlaintextClock();
  const [value, setValue] = useState<Sealed | null>(null);
  const current = useRef<Sealed | null>(null);
  const hold = useCallback((token: Sealed | null) => {
    if (current.current !== token) discard(current.current);
    current.current = token;
    setValue(token);
    return lineCount(token);
  }, []);
  // The view's state goes when it unmounts, and the text goes with it.
  useEffect(() => () => discard(current.current), []);
  const text = openFor(value, scope);
  useEffect(() => {
    if (value !== null && text === null) hold(null);
  }, [value, text, hold]);
  return { value: text === null ? null : value, shown: text !== null, hold };
}

/**
 * Draws a sealed value while it may show under `scope`, and `fallback` otherwise. With `trim`, the
 * value draws without its outer blank space, and `empty` stands in for a value that is all blank.
 */
export function Plaintext({ value, scope, fallback = null, trim = false, empty = null }: { value: Sealed | null; scope: string | null; fallback?: React.ReactNode; trim?: boolean; empty?: React.ReactNode }) {
  usePlaintextClock();
  const text = openFor(value, scope);
  if (text === null) return <>{fallback}</>;
  return <>{trim ? text.trim() || empty : text}</>;
}
