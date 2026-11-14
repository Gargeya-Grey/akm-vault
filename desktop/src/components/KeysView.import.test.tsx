import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeysView } from "./KeysView";
import { api } from "../api";
import { reads } from "../native";
import { adoptReservation } from "../nativeSession";
import { acknowledgeNotices, retainedOperationNotices } from "../operationNotices";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "../sessionBoundary";
import type { KeyView } from "../types";

// FV-13: an import that ends after the user went from Use keys to Keys. The key list mounted before
// the import was applied, so it must read again when the page learns of the import: by the reply,
// by the outcome record that settles the waiting receipt, or by a record that has no receipt here.
// The real api, boundary, receipts and read mechanism run; only the Tauri IPC is mocked.

type Deferred = { resolve: (value: unknown) => void; reject: (reason: unknown) => void };
const native = vi.hoisted(() => ({
  calls: [] as { command: string; args: Record<string, unknown> }[],
  pending: new Map<string, Deferred[]>(),
  records: [] as { id: string; originSession: string; kind: string }[],
  keys: [] as unknown[],
  holdKeys: false,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string, args: Record<string, unknown> = {}) => {
    native.calls.push({ command, args });
    if (command === "keys_list" && !native.holdKeys) return native.keys;
    if (command === "vault_warnings") return { application: [], operations: native.records };
    if (command === "vault_dismiss_warnings") return;
    return new Promise((resolve, reject) => {
      const queue = native.pending.get(command) ?? [];
      queue.push({ resolve, reject });
      native.pending.set(command, queue);
    });
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

function key(id: string, title: string): KeyView {
  return {
    id, title, username: null, issuer: null, project: "demo", environment: "dev", scopes: [], envVar: null,
    resolvedEnvVar: title.toUpperCase(), url: null, notes: null, tags: [], expiresAt: null, rotationDays: null,
    createdAt: null, updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
  };
}
const BEFORE = [key("existing-key-0001", "Existing")];
const AFTER = [...BEFORE, key("imported-key-0002", "Imported")];

beforeEach(() => {
  native.calls = [];
  native.pending.clear();
  native.records = [];
  native.keys = BEFORE;
  native.holdKeys = false;
  adoptReservation(crypto.randomUUID());
});
afterEach(() => {
  cleanup();
  invalidateSession();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

const callsOf = (command: string) => native.calls.filter((call) => call.command === command);
const shown = (title: string) => screen.queryByRole("button", { name: `Open ${title}` }) !== null;

async function settle(command: string, how: "resolve" | "reject", value: unknown) {
  const next = native.pending.get(command)?.shift();
  if (!next) throw new Error(`no pending ${command}`);
  await act(async () => { next[how](value); });
  await act(async () => {});
}

/** The list of Keys is on screen, and an import that Use keys sent still waits for its native window. */
async function keysShownWhileAnImportWaits() {
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
  expect(shown("Existing")).toBe(true);
  let reply!: ReturnType<typeof api.importEnv>;
  await act(async () => { reply = api.importEnv("IMPORTED=fake-import-value", "demo", "dev"); });
  reply.catch(() => {});
  return callsOf("env_import").at(-1)!.args.operationId as string;
}

async function deliver(id: string, kind: string) {
  native.records = [...native.records, { id, originSession: "A", kind }];
  await act(async () => { await reads.warnings(); });
  await act(async () => {});
}

test("(a) an import whose reply arrives while Keys is on screen makes the list show the new key", async () => {
  await keysShownWhileAnImportWaits();
  native.keys = AFTER;
  await settle("env_import", "resolve", { added: 1, updated: 0 });
  expect(shown("Imported")).toBe(true);
  expect(shown("Existing")).toBe(true);
});

test("(b) an import whose outcome record settles the waiting receipt makes the list show the new key", async () => {
  const operation = await keysShownWhileAnImportWaits();
  native.keys = AFTER;
  await deliver(operation, "importApplied");
  expect(shown("Imported")).toBe(true);
});

test("(c) an applied import that this page has no receipt for makes the list show the new key", async () => {
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
  native.keys = AFTER;
  await deliver(crypto.randomUUID(), "importApplied");
  expect(shown("Imported")).toBe(true);
});

test("a declined or failed import reads nothing again", async () => {
  const operation = await keysShownWhileAnImportWaits();
  const before = callsOf("keys_list").length;
  await settle("env_import", "resolve", null);
  await deliver(operation, "importFailed");
  expect(callsOf("keys_list")).toHaveLength(before);
});

test("a late reply of the read that started before the import cannot replace the new list", async () => {
  native.holdKeys = true;
  await act(async () => { render(<KeysView />); });
  let reply!: ReturnType<typeof api.importEnv>;
  await act(async () => { reply = api.importEnv("IMPORTED=fake-import-value", "demo", "dev"); });
  reply.catch(() => {});
  await settle("env_import", "resolve", { added: 1, updated: 0 });
  expect(callsOf("keys_list")).toHaveLength(2);
  const [older, newer] = native.pending.get("keys_list")!;
  await act(async () => { newer.resolve(AFTER); });
  await act(async () => { older.resolve(BEFORE); });
  await act(async () => {});
  expect(shown("Imported")).toBe(true);
});
