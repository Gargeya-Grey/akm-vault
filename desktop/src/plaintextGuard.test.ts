import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { DATA_CHANGES, mutations, PLAINTEXT_COMMANDS } from "./native";
import { adoptReservation } from "./nativeSession";
import * as plaintext from "./plaintext";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";

// U2-R1-04, fault class "a stale value" (contract 4), by construction. A view never holds the
// text of a plaintext reply: native.ts seals it, and only plaintext.tsx opens it, under the stamp
// of its request. This test fails when a reply of plaintext reaches a caller unsealed, or when a
// mutation is not classed as a change of the vault data.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
beforeEach(() => { adoptReservation(crypto.randomUUID()); });
afterEach(() => { vi.resetAllMocks(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

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
