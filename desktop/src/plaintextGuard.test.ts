/// <reference types="vite/client" />
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { DATA_CHANGES, mutations, PLAINTEXT_COMMANDS } from "./native";
import { adoptReservation } from "./nativeSession";
import * as plaintext from "./plaintext";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import { importersOf, pageSources, reachesOf, type Sources } from "./testing/importGraph";

// U2-R1-04, fault class "a stale value" (contract 4), by construction. A view never holds the
// text of a plaintext reply: native.ts seals it, and only plaintext.tsx opens it, under the stamp
// of its request. This test fails when a native command answers with text that nobody classed,
// when a reply of plaintext reaches a caller unsealed, or when any other file reaches sealed.ts.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
beforeEach(() => { adoptReservation(crypto.randomUUID()); });
afterEach(() => { vi.resetAllMocks(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

const all = import.meta.glob<string>(["./**/*.{ts,tsx}"], { query: "?raw", import: "default", eager: true });
const sources = pageSources(all);
const rust = import.meta.glob<string>("../src-tauri/src/**/*.rs", { query: "?raw", import: "default", eager: true });

/** The files that may reach sealed.ts, and why. */
const SEALED_READERS: Record<string, string> = {
  "./native.ts": "it seals the reply of every command that answers with plaintext",
  "./plaintext.tsx": "the one holder and the one drawer of a sealed value",
};
/** The native commands that answer with text that is not plaintext from the vault. */
const NOT_PLAINTEXT: Record<string, string> = {
  vault_prepare_open: "a session reservation ID",
  env_save: "the path of the saved file, or null after a cancel",
};

function violations(files: Sources): string[] {
  return importersOf("./sealed.ts", files).filter((reach) => !Object.hasOwn(SEALED_READERS, reach.file)).map((reach) => `${reach.file} reaches ./sealed.ts by ${reach.how}`);
}

/** The native commands whose reply type holds a String. */
function textCommands(files: Record<string, string>): string[] {
  const found: string[] = [];
  for (const text of Object.values(files)) {
    for (const match of text.matchAll(/#\[tauri::command[^\]]*\]\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+(\w+)/g)) {
      const signature = text.slice(match.index, text.indexOf("{", match.index + match[0].length));
      const reply = signature.slice(signature.lastIndexOf("->"));
      if (/\bString\b/.test(reply)) found.push(match[1]);
    }
  }
  return found.sort();
}

test("every native command that answers with text is sealed, or is named here as not plaintext", () => {
  expect(textCommands(rust)).toEqual([...Object.keys(PLAINTEXT_COMMANDS), ...Object.keys(NOT_PLAINTEXT)].sort());
});

test("the scan of the native commands finds a new command that answers with text", () => {
  expect(textCommands({ "fake.rs": "#[tauri::command]\npub async fn fake_reveal(id: String) -> Result<Option<String>, CommandError> {\n}" })).toEqual(["fake_reveal"]);
});

for (const [command, request] of [
  ["key_reveal", () => api.reveal("fake-id")],
  ["env_export", () => api.exportEnv("fake-project", "fake-env")],
] as const) {
  test(`the reply of ${command} reaches its caller sealed, with no text in it`, async () => {
    native.invoke.mockImplementation(async (name: string) => (name === command ? "FAKE_PLAINTEXT=fake-sealed-value" : undefined));
    const value = await request();
    expect(typeof value).toBe("object");
    expect(value).not.toBeNull();
    expect(Object.isFrozen(value)).toBe(true);
    expect(Object.getPrototypeOf(value)).toBeNull();
    expect(Reflect.ownKeys(value as object)).toEqual([]);
    expect(JSON.stringify(value)).toBe("{}");
    expect(Object.keys(PLAINTEXT_COMMANDS)).toContain(command);
  });
}

test("only the boundary and the holder reach sealed.ts", () => {
  expect(violations(sources)).toEqual([]);
  for (const file of Object.keys(SEALED_READERS)) {
    expect(reachesOf(file, sources).some((reach) => reach.module === "./sealed.ts"), file).toBe(true);
  }
});

/** The mutations that change no key, and the reads that can change the data. */
const NOT_DATA_CHANGES: Record<string, string> = {
  env_save: "it writes a file outside the vault",
  env_launch_terminal: "it starts a terminal and changes no key",
  audit_recover: "it starts a new journal and changes no key",
};
const READS_THAT_CHANGE_DATA: Record<string, string> = {
  vault_reload: "it reads the vault file again, so every key can change",
};

test("every mutation makes held plaintext stale, or is named here as changing no key", () => {
  const expected = [...mutations].filter((command) => !Object.hasOwn(NOT_DATA_CHANGES, command)).concat(Object.keys(READS_THAT_CHANGE_DATA));
  expect([...DATA_CHANGES].sort()).toEqual(expected.sort());
  for (const command of Object.keys(NOT_DATA_CHANGES)) expect(mutations.has(command), command).toBe(true);
});

test("plaintext.tsx gives a view no function that returns the text", () => {
  expect(Object.keys(plaintext).sort()).toEqual(["Plaintext", "plaintextScope", "useSealed"]);
});

for (const [name, file, text] of [
  ["a view that opens a sealed value", "./components/Planted.tsx", `import { openFor } from "../sealed";\nexport const A = (token: never) => openFor(token, "x");`],
  ["a helper that re-exports the opener", "./helpers.ts", `export { openFor } from "./sealed";`],
  ["a dynamic import of sealed.ts", "./components/Planted.ts", `export const B = () => import("../sealed");`],
] as const) {
  test(`the guard fails on ${name}`, () => {
    expect(violations({ ...sources, [file]: text })).not.toEqual([]);
  });
}
