import { expiryLabel, formatDateTime, keyState, plural, rotationLabel } from "./format";
import type { KeyView, RecycledKey } from "./types";

/**
 * The facts that a row draws, as the text on screen (fault classes M and H,
 * docs/design/frontend-reads-and-rows.md). The row draws these strings, the look-alike check
 * compares them, and the accessible description and the confirmations are built from them. So an
 * unset value and a literal value that reads like its fallback draw the same, and count the same.
 */
export interface DrawnFact {
  /** The label of the fact in a fact list and in the accessible description. */
  label: string;
  /** The text on screen. An unset value draws its fallback. */
  text: string;
  /** The key sets this fact. The accessible description reads an unset fact as its fallback. */
  set: boolean;
}

export const NO_PROJECT = "No project";
export const NO_ENVIRONMENT = "No environment";
export const NO_ISSUER = "No issuer";

function fact(label: string, value: string | null, fallback: string): DrawnFact {
  return value === null ? { label, text: fallback, set: false } : { label, text: value, set: true };
}

/** The identity facts of a key: what tells one key from another, in the order a fact list shows them. */
export function keyIdentity(key: KeyView) {
  return {
    project: fact("Project", key.project, NO_PROJECT),
    environment: fact("Environment", key.environment, NO_ENVIRONMENT),
    variable: fact("Variable", key.resolvedEnvVar, ""),
    issuer: fact("Issuer", key.issuer, NO_ISSUER),
    scopes: { label: "Scopes", text: key.scopes.length > 0 ? plural(key.scopes.length, "scope") : "", set: key.scopes.length > 0 },
  };
}

/** Every fact that a key row draws. `now` is the clock of the list (see `useNow`). */
export function keyRowFacts(key: KeyView, now: number) {
  const state = keyState(key, now);
  return {
    title: key.title,
    ...keyIdentity(key),
    renewal: key.rotationDays === null ? "no rotation" : `rotate ${rotationLabel(key, now)}`,
    expiry: key.expiresAt ? expiryLabel(key) : "no expiry",
    state: state.label,
    tone: state.tone,
  };
}

export type KeyRowFacts = ReturnType<typeof keyRowFacts>;

/** Every fact that a Recycle Bin row draws. The bin shows the ID on every row. */
export function binRowFacts(entry: RecycledKey) {
  const { project, environment, variable, issuer } = keyIdentity(entry.key);
  return {
    title: entry.key.title,
    project,
    environment,
    variable,
    issuer,
    id: { label: "ID", text: entry.key.id, set: true },
    removed: entry.deletedAt ? formatDateTime(entry.deletedAt) : "at an unknown time",
    history: entry.key.historyCount > 0 ? plural(entry.key.historyCount, "earlier version") : "",
  };
}

/** The drawn text of every fact, flat, so that two rows compare exactly as they look. */
function drawn(facts: Record<string, unknown>): string {
  return JSON.stringify(Object.values(facts).map((value) => (typeof value === "object" && value !== null && "text" in value ? (value as DrawnFact).text : value)));
}

/**
 * The shortest prefix of `id`, of at least `min` characters, that no other ID in `ids` starts
 * with. Two different IDs never get the same text.
 */
export function shortId(id: string, ids: readonly string[], min = 8): string {
  for (let length = Math.min(min, id.length); length < id.length; length += 1) {
    const prefix = id.slice(0, length);
    if (!ids.some((other) => other !== id && other.startsWith(prefix))) return prefix;
  }
  return id;
}

/**
 * The ID text of each row that matches another row of the same list in every drawn fact. Both
 * rows of a pair get one. Rows that differ in any drawn fact get none.
 */
export function lookalikeIds<T>(rows: readonly T[], id: (row: T) => string, facts: (row: T) => Record<string, unknown>): Map<string, string> {
  const byDrawing = new Map<string, string[]>();
  for (const row of rows) {
    const key = drawn(facts(row));
    byDrawing.set(key, [...(byDrawing.get(key) ?? []), id(row)]);
  }
  const all = rows.map(id);
  const shown = new Map<string, string>();
  for (const ids of byDrawing.values()) {
    if (ids.length > 1) for (const each of ids) shown.set(each, shortId(each, all));
  }
  return shown;
}

/** An identity fact as the accessible description reads it: "issuer Acme", or "no issuer". */
function spoken(item: DrawnFact): string {
  return item.set ? `${item.label.toLowerCase()} ${item.text}` : item.text.toLowerCase();
}

/**
 * What a screen reader hears about the identity of a key row: the identity facts that the row
 * draws, and its ID when the row shows one. The title is the row's name.
 */
export function keyRowDescription(facts: KeyRowFacts, id: string | undefined): string {
  const parts = [facts.project, facts.environment, facts.variable, facts.issuer].map(spoken);
  if (facts.scopes.set) parts.push(facts.scopes.text);
  if (id !== undefined) parts.push(`ID ${id}`);
  const text = parts.join(", ");
  return `${text.charAt(0).toUpperCase()}${text.slice(1)}.`;
}

/** What a screen reader hears about a Recycle Bin entry: every fact that its row draws. */
export function binRowDescription(facts: ReturnType<typeof binRowFacts>): string {
  const parts = [facts.project, facts.environment, facts.variable, facts.issuer].map(spoken);
  parts.push(`ID ${facts.id.text}`, `removed ${facts.removed}`);
  if (facts.history) parts.push(facts.history);
  const text = parts.join(", ");
  return `${text.charAt(0).toUpperCase()}${text.slice(1)}.`;
}
