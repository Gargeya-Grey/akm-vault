import { dataGeneration, sessionGeneration } from "./sessionBoundary";
import type { Sealed } from "./types";

/**
 * Plaintext from the vault, sealed at the boundary (U2-R1-04, contract 4). `native.ts` seals the
 * reply of every command that returns plaintext under the stamp that its request took: the
 * session, the generation of the vault data and the scope. Only `plaintext.tsx` opens a sealed
 * value, and only while all three still hold and the view names the same scope. A value found
 * stale is dropped at once and never shows again: only a new request shows plaintext again.
 * `plaintextGuard.test.ts` keeps every other file away from this module.
 */

export interface Stamp {
  session: number;
  data: number;
  scope: string;
}

interface Held extends Stamp {
  text: string;
}

const held = new WeakMap<Sealed, Held>();

/** The stamp of a request that is about to leave. */
export function stampFor(scope: string): Stamp {
  return { session: sessionGeneration(), data: dataGeneration(), scope };
}

/** A token with no fields, no prototype and no text in it. */
export function seal(text: string, stamp: Stamp): Sealed {
  const token = Object.freeze(Object.create(null)) as Sealed;
  held.set(token, { ...stamp, text });
  return token;
}

/** The text while it may show under `scope`, else null. A value that may not show is dropped. */
export function openFor(token: Sealed | null, scope: string | null): string | null {
  if (token === null) return null;
  const value = held.get(token);
  if (value === undefined) return null;
  if (scope === null || value.scope !== scope || value.session !== sessionGeneration() || value.data !== dataGeneration()) {
    held.delete(token);
    return null;
  }
  return value.text;
}

/**
 * The number of lines of a sealed value that are not blank, while it holds under its own stamp,
 * and null when it no longer holds. A count, never the text.
 */
export function lineCount(token: Sealed | null): number | null {
  const value = token === null ? undefined : held.get(token);
  if (value === undefined || value.session !== sessionGeneration() || value.data !== dataGeneration()) return null;
  return value.text.split("\n").filter((line) => line.trim()).length;
}

/** Drops a sealed value now. */
export function discard(token: Sealed | null) {
  if (token !== null) held.delete(token);
}
