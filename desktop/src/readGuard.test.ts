/// <reference types="vite/client" />
import { expect, test } from "vitest";
import { api } from "./api";
import { reads } from "./native";
import { nativeReads } from "./nativeRead";
import { importersOf, pageSources, reachesOf, tauriGlobalsIn, type Sources } from "./testing/importGraph";

// Fault classes K and L, by construction (U2-R1-05). The functions that read native state live in
// `native.ts`. Only the modules below may reach that file, and only `native.ts` may reach the Tauri
// IPC. The test reads the import graph with the TypeScript parser and checks it by the resolved
// module, so an alias, a re-export, a dynamic import, a `require` and a hook all count, whatever
// the binding is called. A view reads native state only through `useNativeRead`.

/** The members of `reads`: they read native state for the screen. */
const READS = ["audit", "backup", "history", "keys", "recycled", "remindersStatus", "status", "warnings"];
/** The members of `api`: each changes state, asks native for an effect, or ends a session. */
const ACTIONS = [
  "activity", "add", "copy", "copyEnv", "create", "discardView", "dismissWarnings", "exportEnv", "importEnv",
  "launchTerminal", "lock", "lockCurrent", "purge", "recoverJournal", "reload", "remindersNotify", "remindersSet",
  "remove", "restore", "reveal", "rotate", "saveEnv", "unlock", "update", "upgrade",
];

/**
 * Each module that few files may reach, with every file that may reach it and why. The test also
 * fails when a file here no longer reaches its module, so the table cannot grow stale.
 */
const RESTRICTED: Record<string, Record<string, string>> = {
  "./native.ts": {
    "./api.ts": "the actions: every request goes through the one boundary function in native.ts",
    "./nativeRead.ts": "the read mechanism: it owns the request identity, the loading state, the failure and the unmount",
    "./App.tsx": "the session poll keeps its own request, epoch and transition identity; the warning poll folds native records",
    "./components/AttentionNotifier.tsx": "it shows nothing: it reads the reminder setting and the keys, sends a count and stops when it unmounts",
  },
  "@tauri-apps/api/core": { "./native.ts": "the one boundary function with native code" },
  "@tauri-apps/api": {},
};

const all = import.meta.glob<string>(["./**/*.{ts,tsx}"], { query: "?raw", import: "default", eager: true });
const sources = pageSources(all);

/** Every way in which a file reaches a restricted module that it may not reach, and every Tauri global. */
function violations(files: Sources): string[] {
  const found: string[] = [];
  for (const [module, allowed] of Object.entries(RESTRICTED)) {
    for (const reach of importersOf(module, files)) {
      if (!Object.hasOwn(allowed, reach.file)) found.push(`${reach.file} reaches ${module} by ${reach.how}`);
    }
  }
  for (const file of Object.keys(files)) for (const name of tauriGlobalsIn(file, files)) found.push(`${file} names ${name}`);
  return found;
}

/** The real sources with one more file. */
function planted(file: string, text: string): Sources {
  return { ...sources, [file]: text };
}

test("every member of reads is a read, and api holds no read", () => {
  expect(Object.keys(reads).sort()).toEqual([...READS].sort());
  expect(Object.keys(api).sort()).toEqual([...ACTIONS].sort());
  for (const name of READS) expect(Object.hasOwn(api, name), name).toBe(false);
});

test("the scan sees the page sources and no test file", () => {
  expect(Object.keys(sources)).toEqual(expect.arrayContaining(["./App.tsx", "./api.ts", "./native.ts", "./nativeRead.ts", "./components/KeysView.tsx", "./components/Settings.tsx"]));
  expect(Object.keys(sources).filter((file) => file.includes(".test."))).toEqual([]);
});

test("no file reaches native state except through the files that the table names", () => {
  expect(violations(sources)).toEqual([]);
});

test("every allowed file still reaches its module, so the table cannot grow stale", () => {
  for (const [module, allowed] of Object.entries(RESTRICTED)) {
    for (const file of Object.keys(allowed)) {
      expect(reachesOf(file, sources).some((reach) => reach.module === module), `${file} -> ${module}`).toBe(true);
    }
  }
});

test("a view cannot run a read itself: each entry of nativeReads is a token that only useNativeRead runs", () => {
  for (const [name, entry] of Object.entries(nativeReads)) {
    const token = typeof entry === "function" ? entry("fake-id") : entry;
    expect(typeof token, name).toBe("object");
    expect(Object.isFrozen(token), name).toBe(true);
    expect(Object.keys(token), name).toEqual([]);
  }
});

// The forms that the review found (U2-R1-05), and the other forms of the same reach. Each one,
// planted beside the real sources, makes the guard fail.
const PLANTS: [string, string, string][] = [
  ["the reviewer's alias", "./components/PlantedAlias.tsx", `import { reads as native } from "../native";\nexport const A = () => { void native.keys().then(() => {}); return null; };`],
  ["a helper that re-exports by name", "./helpers.ts", `export { reads as r } from "./native";`],
  ["a helper that re-exports everything", "./helpers.ts", `export * from "./native";`],
  ["a helper that imports and passes the reads on", "./helpers.ts", `import { reads } from "./native";\nexport const r = reads;`],
  ["a namespace import", "./components/Planted.tsx", `import * as N from "../native";\nexport const B = () => N.reads.keys();`],
  ["a dynamic import", "./components/Planted.tsx", `export async function C() { const m = await import("../native"); return m.reads.recycled(); }`],
  ["a dynamic import with a computed name", "./components/Planted.tsx", `const where = "../" + "native";\nexport const D = () => import(/* @vite-ignore */ where);`],
  ["a require", "./components/Planted.tsx", `declare const require: (name: string) => { reads: { keys: () => void } };\nexport const E = () => require("../native").reads.keys();`],
  ["an import-equals require", "./components/Planted.ts", `import native = require("../native");\nexport const F = native;`],
  ["an import glob", "./components/Planted.ts", `export const G = import.meta.glob("../native.ts", { eager: true });`],
  ["an effect in a hook file", "./components/useDirectKeys.ts", `import { useEffect } from "react";\nimport { reads } from "../native";\nexport function useDirectKeys(set: (keys: unknown) => void) { useEffect(() => { void reads.keys().then(set); }, [set]); }`],
  ["an import for its side effect", "./components/Planted.ts", `import "../native";`],
  ["a direct IPC import", "./components/Planted.ts", `import { invoke } from "@tauri-apps/api/core";\nexport const H = () => invoke("keys_list");`],
  ["an IPC import through the package root", "./components/Planted.ts", `import { core } from "@tauri-apps/api";\nexport const I = () => core.invoke("keys_list");`],
  ["the Tauri global", "./components/Planted.ts", `export const J = () => (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => void } }).__TAURI_INTERNALS__.invoke("keys_list");`],
  ["the Tauri global by a string", "./components/Planted.ts", `export const K = () => (window as unknown as Record<string, unknown>)["__TAURI_INTERNALS__"];`],
];

for (const [name, file, text] of PLANTS) {
  test(`the guard fails on ${name}`, () => {
    expect(violations(planted(file, text)).filter((line) => line.startsWith(file))).not.toEqual([]);
  });
}

test("a type-only import gives no value at run time, so it passes", () => {
  expect(violations(planted("./components/Planted.ts", `import type { NativeCall } from "../native";\nimport { type NativeCall as Other } from "../native";\nexport type T = NativeCall | Other;`))).toEqual([]);
});
