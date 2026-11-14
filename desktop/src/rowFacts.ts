import { useEffect, useMemo, useSyncExternalStore } from "react";
import { expiryLabel, formatDateTime, keyState, plural, rotationLabel, type KeyTone } from "./format";
import { sessionGeneration } from "./sessionBoundary";
import type { AuditEntry, KeyView, RecycledKey } from "./types";
import { useNow } from "./useNow";

/**
 * The facts that a page draws about a key, as the text on screen (fault classes M and H, U2-R1-01
 * and U2-R1-02, docs/design/frontend-reads-and-rows.md). One function builds the facts of each
 * kind of row. The row, its accessible description, every dialog that acts on the key, and the
 * tests use them. Every key row and every place that names a key draws the key's short ID, so two
 * keys never draw the same, whatever their values. The tests of each view (RowIdentity,
 * KeyTarget, RecycleBin.facts) draw two keys that differ by one fact and tell them apart.
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

// Controls, format marks, separators, every space but U+0020, every character that Unicode marks
// as ignorable, the two marks of this drawing, and the separator that the page puts between facts.
const MARKED = /[\p{C}\p{Z}\p{Default_Ignorable_Code_Point}\u27E8\u27E9\u00B7]/u;
const mark = (char: string) => `⟨U+${char.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}⟩`;

/**
 * A stored value as the page draws it (U2-R1-01). The stored value never changes: a form edits it,
 * a search matches it and native code receives it as it is. A character that draws nothing, moves
 * the text around it or breaks the line draws as its code point, as ⟨U+200B⟩: a control (a
 * newline, a tab), a format mark (a zero-width space or joiner, a directional mark, override or
 * isolate, a byte order mark, a soft hyphen), a line or paragraph separator, a space other than
 * U+0020, and every other character that Unicode marks as ignorable. So do ⟨ and ⟩, so that a
 * value cannot imitate a mark, and the middle dot, so that a value cannot imitate the separator
 * between two facts. A U+0020 space draws as a space only between two other characters; a
 * leading, a trailing or a repeated space draws as ⟨U+0020⟩. Two different values therefore never
 * draw the same text, and a screen reader that folds spaces still hears them apart. Letters that
 * look alike in two scripts draw as they are: the short ID tells such keys apart.
 */
export function drawnText(value: string): string {
  const chars = [...value];
  return chars
    .map((char, index) => {
      if (char === " ") return index > 0 && index < chars.length - 1 && chars[index - 1] !== " " ? char : mark(char);
      return MARKED.test(char) ? mark(char) : char;
    })
    .join("");
}

/** A text of several lines, such as notes: each line drawn, and its line ends kept as lines. */
export function drawnLines(value: string): string[] {
  return value.split(/\r?\n/).map(drawnText);
}

function fact(label: string, value: string | null, fallback: string): DrawnFact {
  return value === null ? { label, text: fallback, set: false } : { label, text: drawnText(value), set: true };
}

/**
 * The shortest prefix of each ID, of at least 8 characters, that no other ID of `ids` starts with.
 * An ID that is itself the start of another ID draws in full. Two different IDs never draw the same.
 */
export function shortIds(ids: Iterable<string>): Map<string, string> {
  const sorted = [...new Set(ids)].sort();
  const common = (a: string | undefined, b: string | undefined) => {
    if (a === undefined || b === undefined) return 0;
    let length = 0;
    while (length < a.length && length < b.length && a[length] === b[length]) length += 1;
    return length;
  };
  const result = new Map<string, string>();
  sorted.forEach((id, index) => {
    const needed = Math.max(common(sorted[index - 1], id), common(id, sorted[index + 1])) + 1;
    result.set(id, id.slice(0, Math.min(id.length, Math.max(8, needed))));
  });
  return result;
}

// The key IDs that the page knows in this session: every ID that a view has listed, live keys,
// the Recycle Bin and the journal alike. A short ID is unique among all of them. A new session
// starts with none.
const KNOWN_EVENT = "akm-known-key-ids";
let knownSession = -1;
let known = new Set<string>();
let knownVersion = 0;

function knownIds(): Set<string> {
  if (knownSession !== sessionGeneration()) {
    knownSession = sessionGeneration();
    known = new Set();
  }
  return known;
}

function learn(ids: readonly string[]) {
  const set = knownIds();
  const before = set.size;
  for (const id of ids) set.add(id);
  if (set.size === before) return;
  knownVersion += 1;
  window.dispatchEvent(new Event(KNOWN_EVENT));
}

function subscribe(listener: () => void) {
  window.addEventListener(KNOWN_EVENT, listener);
  return () => window.removeEventListener(KNOWN_EVENT, listener);
}

const knownSnapshot = () => `${sessionGeneration()}:${knownVersion}`;
const SEPARATOR = "\u0000";

export type ShortId = (id: string) => string;

/**
 * The short ID of each key that a view lists. The IDs join the IDs that the page knows, and every
 * view that draws a short ID draws it again when that set grows.
 */
export function useShortIds(listed: readonly string[]): ShortId {
  const version = useSyncExternalStore(subscribe, knownSnapshot);
  const joined = listed.join(SEPARATOR);
  useEffect(() => learn(joined === "" ? [] : joined.split(SEPARATOR)), [joined]);
  return useMemo(() => {
    const map = shortIds([...knownIds(), ...(joined === "" ? [] : joined.split(SEPARATOR))]);
    return (id: string) => map.get(id) ?? shortIds([...map.keys(), id]).get(id)!;
    // `version` stands for the set of known IDs, which changes in place.
  }, [version, joined]);
}

/** The identity facts of a key: what tells one key from another, in the order a fact list shows them. */
export function keyIdentity(key: KeyView) {
  return {
    project: fact("Project", key.project, NO_PROJECT),
    environment: fact("Environment", key.environment, NO_ENVIRONMENT),
    variable: fact("Variable", key.resolvedEnvVar, ""),
    issuer: fact("Issuer", key.issuer, NO_ISSUER),
    scopes: { label: "Scopes", text: key.scopes.length > 0 ? plural(key.scopes.length, "scope") : "", set: key.scopes.length > 0 } as DrawnFact,
  };
}

export interface KeyRowFacts extends ReturnType<typeof keyIdentity> {
  title: string;
  renewal: string;
  expiry: string;
  state: string;
  tone: KeyTone;
  id: DrawnFact;
}

/** Every fact that a key row draws. `now` is the clock of the list (see `useNow`). */
export function keyRowFacts(key: KeyView, now: number, shortId: ShortId): KeyRowFacts {
  const state = keyState(key, now);
  return {
    title: drawnText(key.title),
    ...keyIdentity(key),
    renewal: key.rotationDays === null ? "no rotation" : `rotate ${rotationLabel(key, now)}`,
    expiry: key.expiresAt ? expiryLabel(key) : "no expiry",
    state: state.label,
    tone: state.tone,
    id: { label: "ID", text: shortId(key.id), set: true },
  };
}

/** The row facts of one key, for a dialog that acts on it: the same facts and short ID as its row. */
export function useKeyRowFacts(key: KeyView): KeyRowFacts {
  const now = useNow([key]);
  const shortId = useShortIds([key.id]);
  return keyRowFacts(key, now, shortId);
}

/** Every fact of a key row as one list, in the order a dialog that acts on the key shows them. */
export function keyFactList(facts: KeyRowFacts): DrawnFact[] {
  return [
    facts.project,
    facts.environment,
    facts.variable,
    facts.issuer,
    ...(facts.scopes.set ? [facts.scopes] : []),
    { label: "Renewal", text: facts.renewal, set: true },
    { label: "Expiry", text: facts.expiry, set: true },
    { label: "Status", text: facts.state, set: true },
    facts.id,
  ];
}

/** The values of a key that only its drawer shows, drawn. Notes keep their lines. */
export function keyDetails(key: KeyView) {
  return {
    username: key.username === null ? null : drawnText(key.username),
    scopes: key.scopes.map(drawnText),
    tags: key.tags.map(drawnText),
    url: key.url === null ? null : drawnText(key.url),
    notes: key.notes === null ? null : drawnLines(key.notes),
  };
}

/**
 * AKM's one caseless rule, as `caseless` in akm-core (FW-06). Each letter becomes the lowercase of
 * its uppercase when that uppercase is one letter, and its own lowercase otherwise: `Σ`, `σ` and `ς`
 * are one letter, `ẞ` and `ß` are one letter, and `ß` is not `ss`. Each letter maps alone, with no
 * context and no normalization. The page and the core each use the Unicode tables of their own
 * runtime, so a letter that only the newer table knows can fold differently.
 */
export function caseless(text: string): string {
  let folded = "";
  for (const letter of text) {
    const upper = letter.toUpperCase();
    folded += [...upper].length === 1 ? upper.toLowerCase() : letter.toLowerCase();
  }
  return folded;
}

/** What a search of the key list matches, folded by `caseless`: each stored value, and each value as its row draws it. */
export function searchText(key: KeyView, facts: KeyRowFacts): string {
  const stored = [key.title, key.issuer, key.project, key.environment, key.username, key.resolvedEnvVar, key.url, key.notes, ...key.scopes, ...key.tags];
  const drawn = [facts.title, facts.issuer.text, facts.project.text, facts.environment.text, facts.variable.text, facts.id.text];
  return caseless([...stored, ...drawn].filter(Boolean).join("\n"));
}

export interface BinRowFacts {
  title: string;
  project: DrawnFact;
  environment: DrawnFact;
  variable: DrawnFact;
  issuer: DrawnFact;
  renewal: string;
  expiry: string;
  id: DrawnFact;
  removed: string;
  history: string;
}

/** Every fact that a Recycle Bin row draws. The bin shows the full ID on every row. */
export function binRowFacts(entry: RecycledKey, now: number): BinRowFacts {
  const { title, project, environment, variable, issuer, renewal, expiry } = keyRowFacts(entry.key, now, (id) => id);
  return {
    title,
    project,
    environment,
    variable,
    issuer,
    renewal,
    expiry,
    id: { label: "ID", text: entry.key.id, set: true },
    removed: entry.deletedAt ? formatDateTime(entry.deletedAt) : "at an unknown time",
    history: entry.key.historyCount > 0 ? plural(entry.key.historyCount, "earlier version") : "",
  };
}

/** Every fact of a Recycle Bin row as one list, as the permanent-delete confirmation shows them. */
export function binFactList(facts: BinRowFacts): DrawnFact[] {
  return [
    facts.project,
    facts.environment,
    facts.variable,
    facts.issuer,
    { label: "Renewal", text: facts.renewal, set: true },
    { label: "Expiry", text: facts.expiry, set: true },
    facts.id,
    { label: "Removed", text: facts.removed, set: true },
    ...(facts.history ? [{ label: "History", text: facts.history, set: true }] : []),
  ];
}

export interface JournalKeyFacts {
  /** The title as drawn, "Key" for a record that names a key without a title, or "Vault". */
  name: string;
  /** The short ID, or null for a record of the vault. */
  id: string | null;
  /** The project and the environment as drawn, or "—". */
  scope: string;
}

/** The key that a journal record names. */
export function journalKeyFacts(entry: AuditEntry, shortId: ShortId): JournalKeyFacts {
  return {
    name: entry.keyTitle !== null ? drawnText(entry.keyTitle) : entry.keyId === null ? "Vault" : "Key",
    id: entry.keyId === null ? null : shortId(entry.keyId),
    scope: [entry.project, entry.environment].filter((part): part is string => Boolean(part)).map(drawnText).join(" · ") || "—",
  };
}

/** A key named in a sentence: its drawn title in quotes, then its ID. */
export function namedKey(facts: { title: string; id: DrawnFact }): string {
  return `“${facts.title}” (ID ${facts.id.text})`;
}

/** An identity fact as the accessible description reads it: "issuer Acme", or "no issuer". */
function spoken(item: DrawnFact): string {
  return item.set ? `${item.label.toLowerCase()} ${item.text}` : item.text.toLowerCase();
}

function sentence(parts: string[]): string {
  const text = parts.join(", ");
  return `${text.charAt(0).toUpperCase()}${text.slice(1)}.`;
}

/**
 * What a screen reader hears about the identity of a key row: the identity facts that the row
 * draws, and its ID. The title is the row's name; the status sentences follow.
 */
export function keyRowDescription(facts: KeyRowFacts): string {
  const parts = [facts.project, facts.environment, facts.variable, facts.issuer].map(spoken);
  if (facts.scopes.set) parts.push(facts.scopes.text);
  parts.push(`ID ${facts.id.text}`);
  return sentence(parts);
}

/** What a screen reader hears about a Recycle Bin entry: every fact that its row draws. */
export function binRowDescription(facts: BinRowFacts): string {
  const parts = [facts.project, facts.environment, facts.variable, facts.issuer].map(spoken);
  parts.push(`ID ${facts.id.text}`, facts.renewal, facts.expiry === "no expiry" ? facts.expiry : `expires ${facts.expiry}`, `removed ${facts.removed}`);
  if (facts.history) parts.push(facts.history);
  return sentence(parts);
}
