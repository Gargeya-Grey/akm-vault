/// <reference types="vite/client" />
import { expect, test } from "vitest";
import { NATIVE_ERROR_KINDS } from "./types";

// The error contract with native code (U2-R1-07). Every native command rejects with a
// `CommandError`, and the page knows its kinds from the closed list in types.ts. `native.ts` treats
// a rejection of any other shape as a transport failure and keeps the outcome record. This test
// reads the native sources, so it fails when native code builds a kind that the list does not name,
// builds a kind in a way that the scan cannot read, or rejects a command with another error type.

const rust = import.meta.glob<string>("../src-tauri/src/**/*.rs", { query: "?raw", import: "default", eager: true });
const mock = import.meta.glob<string>("../e2e/native-boundary.ts", { query: "?raw", import: "default", eager: true });

interface Scan { kinds: Set<string>; unreadable: string[]; commandsWithOtherErrors: string[]; commands: number }

/** The text between the bracket at `open` and the bracket that closes it, at the first level only. */
function topLevel(text: string, open: number): string {
  let depth = 0;
  let out = "";
  for (let index = open; index < text.length; index += 1) {
    const char = text[index];
    if (char === "{" || char === "(") depth += 1;
    if (char === "}" || char === ")") depth -= 1;
    if (depth === 0) return out;
    if (depth === 1 && index > open) out += char;
  }
  return out;
}

function scan(sources: Record<string, string>): Scan {
  const result: Scan = { kinds: new Set(), unreadable: [], commandsWithOtherErrors: [], commands: 0 };
  for (const [file, text] of Object.entries(sources)) {
    const constants = new Map([...text.matchAll(/const\s+([A-Z_]+)\s*:\s*&str\s*=\s*"([^"]*)"/g)].map((match) => [match[1], match[2]]));
    // `Self::new` builds a CommandError only inside error.rs, where the type and its conversions live.
    const builder = file.endsWith("/error.rs") ? /\b(?:CommandError|Self)::new\(\s*/g : /\bCommandError::new\(\s*/g;
    for (const match of text.matchAll(builder)) {
      const rest = text.slice(match.index + match[0].length);
      const literal = /^"([^"]*)"/.exec(rest);
      const constant = /^([A-Z_]+)\s*,/.exec(rest);
      const forwarded = /^&[a-z_.]*\bkind\s*,/.test(rest); // passes on a kind that native code built already
      if (literal) result.kinds.add(literal[1]);
      else if (constant && constants.has(constant[1])) result.kinds.add(constants.get(constant[1])!);
      else if (!forwarded) result.unreadable.push(`${file}: CommandError::new(${rest.slice(0, 40)}`);
    }
    // A struct literal that names a kind, outside `CommandError::new`.
    for (const match of text.matchAll(/(?<!(?:->|struct|impl|for)\s*)\bCommandError\s*\{/g)) {
      if (/\bkind\s*:/.test(topLevel(text, match.index + match[0].length - 1))) result.unreadable.push(`${file}: a CommandError literal with a kind`);
    }
    for (const match of text.matchAll(/#\[tauri::command[^\]]*\]\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+(\w+)/g)) {
      result.commands += 1;
      const signature = text.slice(match.index, text.indexOf("{", match.index + match[0].length));
      if (!/->\s*Result<[\s\S]*,\s*CommandError\s*>\s*$/.test(signature.trim())) result.commandsWithOtherErrors.push(`${file}: ${match[1]}`);
    }
  }
  return result;
}

const native = scan(rust);

test("the scan reads the native sources", () => {
  expect(Object.keys(rust).length).toBeGreaterThan(10);
  expect(native.commands).toBeGreaterThan(20);
  expect(native.kinds.size).toBeGreaterThan(10);
});

test("every native command rejects with a CommandError", () => {
  expect(native.commandsWithOtherErrors).toEqual([]);
});

test("the scan can read every kind that native code builds", () => {
  expect(native.unreadable).toEqual([]);
});

test("the closed list in types.ts names exactly the kinds that native code builds", () => {
  expect([...native.kinds].sort()).toEqual([...NATIVE_ERROR_KINDS].sort());
});

test("the mocked native boundary of the browser tests replies only with listed kinds, and names its one transport failure", () => {
  const text = Object.values(mock).join("\n");
  const thrown = [...text.matchAll(/(?:throw|reject\()\s*\{\s*kind:\s*"([^"]+)"/g)].map((match) => match[1]);
  expect(thrown.length).toBeGreaterThan(0);
  expect(thrown.filter((kind) => !(NATIVE_ERROR_KINDS as readonly string[]).includes(kind))).toEqual([]);
  expect([...text.matchAll(/kind:\s*"transport"/g)]).toHaveLength(1);
  expect(text).toContain('const transportFailure = () => ({ kind: "transport"');
});

// The forms that the scan must catch. Each one, alone in a fake file, gives a finding.
for (const [name, text, check] of [
  ["a new literal kind", `fn f() -> CommandError { CommandError::new("fakeNewKind", "x") }`, (found: Scan) => found.kinds.has("fakeNewKind")],
  ["a new kind through a constant", `const FAKE: &str = "fakeConstant";\nfn f() -> CommandError { CommandError::new(FAKE, "x") }`, (found: Scan) => found.kinds.has("fakeConstant")],
  ["a kind from an expression", `fn f(kind: &str) -> CommandError { CommandError::new(kind, "x") }`, (found: Scan) => found.unreadable.length === 1],
  ["a struct literal with a kind", `fn f() -> CommandError { CommandError { kind: "fakeLiteral".into(), message: String::new() } }`, (found: Scan) => found.unreadable.length === 1],
  ["a command that rejects with a string", `#[tauri::command]\npub fn fake_command() -> Result<(), String> {\n Ok(())\n}`, (found: Scan) => found.commandsWithOtherErrors.length === 1],
] as const) {
  test(`the scan finds ${name}`, () => {
    expect(check(scan({ "../src-tauri/src/fake.rs": text }))).toBe(true);
  });
}
