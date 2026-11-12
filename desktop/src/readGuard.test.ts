/// <reference types="vite/client" />
import { expect, test } from "vitest";
import { api } from "./api";

// Fault classes K and L, by construction: a view reads native state only through
// `useNativeRead` (nativeRead.ts). This test fails when a file calls a read of `api` itself.

/** The members of `api` that read native state for the screen. */
const READS = ["audit", "backup", "history", "keys", "recycled", "remindersStatus", "status", "warnings"];
/** Every other member: it changes state, asks native for an effect, or ends a session. */
const ACTIONS = [
  "activity", "add", "copy", "copyEnv", "create", "discardView", "dismissWarnings", "exportEnv", "importEnv",
  "launchTerminal", "lock", "lockCurrent", "purge", "recoverJournal", "reload", "remindersNotify", "remindersSet",
  "remove", "restore", "reveal", "rotate", "saveEnv", "unlock", "update", "upgrade",
];

/**
 * The only files that may call a read of `api`, with the reads each one calls and why it is not a
 * view. The test also fails when a file here no longer calls a read that this table names.
 */
const ALLOWED: Record<string, { reads: string[]; why: string }> = {
  "./nativeRead.ts": { reads: ["audit", "backup", "history", "keys", "recycled", "remindersStatus"], why: "the mechanism" },
  "./App.tsx": { reads: ["status", "warnings"], why: "the session poll keeps its own request, epoch and transition identity; the warning reply arrives through the event that api.ts owns" },
  "./components/AttentionNotifier.tsx": { reads: ["keys", "remindersStatus"], why: "it shows nothing; it sends a count and stops when it unmounts" },
};

const sources = import.meta.glob<string>(["./**/*.{ts,tsx}", "!./**/*.test.{ts,tsx}", "!./testing/**", "!./api.ts"], {
  query: "?raw", import: "default", eager: true,
});

function readsIn(source: string): string[] {
  const found = new Set<string>();
  for (const match of source.matchAll(/\bapi\s*\.\s*([A-Za-z_$][\w$]*)/g)) if (READS.includes(match[1])) found.add(match[1]);
  return [...found].sort();
}

test("every member of api is classified as a read or an action", () => {
  expect(Object.keys(api).sort()).toEqual([...READS, ...ACTIONS].sort());
});

test("the scan sees the source files", () => {
  expect(Object.keys(sources)).toEqual(expect.arrayContaining(["./App.tsx", "./nativeRead.ts", "./components/KeysView.tsx", "./components/Settings.tsx"]));
});

test("no view calls a read of api itself", () => {
  const offenders: string[] = [];
  for (const [file, source] of Object.entries(sources)) {
    const reads = readsIn(source);
    const allowed = ALLOWED[file]?.reads ?? [];
    for (const read of reads) if (!allowed.includes(read)) offenders.push(`${file} calls api.${read}`);
  }
  expect(offenders).toEqual([]);
});

test("every allowed read is still called, so the list cannot grow stale", () => {
  for (const [file, { reads }] of Object.entries(ALLOWED)) {
    expect(sources[file], file).toBeDefined();
    expect(readsIn(sources[file]), file).toEqual([...reads].sort());
  }
});

test("no file reaches api through an index or a destructured name", () => {
  const offenders = Object.entries(sources)
    .filter(([, source]) => /\bapi\s*\[/.test(source) || /\}\s*=\s*api\b/.test(source) || /=\s*api\s*;/.test(source))
    .map(([file]) => file);
  expect(offenders).toEqual([]);
});

test("the scan finds a direct read", () => {
  expect(readsIn("const keys = await api.keys();")).toEqual(["keys"]);
  expect(readsIn("void api . recycled ()")).toEqual(["recycled"]);
  expect(readsIn("await api.remove(id)")).toEqual([]);
});
