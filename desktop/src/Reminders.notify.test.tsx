import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { AttentionNotifier } from "./components/AttentionNotifier";
import { adoptReservation } from "./nativeSession";
import { nextAnnouncement } from "./reminders";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import type { KeyView, ReminderStatus } from "./types";

// Unit 4 (M13): the opt-in notification. The page sends a count and nothing else. It sends one
// only while the vault is unlocked, the setting is on, AKM is not blocked, and the window is in
// the background. These tests use a mocked native boundary: they do not prove that Windows
// shows a toast.

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: typeof native.onLock) => {
    if (event === "vault-locked") native.onLock = callback;
    return () => {};
  }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));

const NOW = "2026-06-01T12:00:00Z";
let status: ReminderStatus;
let keys: KeyView[];
let foreground = true;
let owner = "";

function key(id: string, title: string, overrides: Partial<KeyView> = {}): KeyView {
  return {
    id, title, username: null, issuer: null, project: "secret-project", environment: "prod",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}
const expired = (id: string) => key(id, `Private title ${id}`, { expiresAt: "2026-05-01T12:00:00Z" });
const fine = (id: string) => key(id, `Fine ${id}`, { expiresAt: "2027-06-01T12:00:00Z" });
const sent = () => native.invoke.mock.calls.filter(([command]) => command === "reminders_notify");

function setForeground(value: boolean) {
  foreground = value;
  vi.spyOn(document, "hasFocus").mockImplementation(() => foreground);
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval", "setTimeout", "clearTimeout"] });
  vi.setSystemTime(new Date(NOW));
  adoptReservation(crypto.randomUUID());
  owner = crypto.randomUUID();
  status = { enabled: true, permission: "granted", delivery: "none", blocked: false };
  keys = [];
  setForeground(true);
  native.onLock = undefined;
  native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "reminders_status") return status;
    if (command === "keys_list") return keys;
    if (command === "reminders_notify") {
      void args;
      status = { ...status, delivery: "sent" };
      return status;
    }
    if (command === "vault_status") return { sessionId: owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: keys.length, vaultName: "Fake", needsUpgrade: false };
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    throw new Error(command);
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  native.invoke.mockReset();
});

async function tick(ms = 30_000) {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}
async function mount() {
  await act(async () => { render(<AttentionNotifier />); });
}

test("the announcement table", () => {
  const table: [string, number, number, boolean, number, boolean][] = [
    // name, announced, count, foreground, announced after, notify
    ["no key needs attention", 0, 0, false, 0, false],
    ["first look in the foreground sets the baseline and says nothing", 0, 2, true, 2, false],
    ["first look in the background says the count", 0, 2, false, 2, true],
    ["the same count in the background says nothing again", 2, 2, false, 2, false],
    ["a higher count in the background says the new count", 2, 3, false, 3, true],
    ["a higher count in the foreground says nothing and is remembered", 2, 3, true, 3, false],
    ["a lower count is remembered, so a later rise is announced again", 3, 1, false, 1, false],
    ["all keys fine resets to zero", 3, 0, true, 0, false],
  ];
  for (const [name, announced, count, front, after, notify] of table) {
    expect(nextAnnouncement(announced, count, front), name).toEqual({ announced: after, notify });
  }
});

test("a setting that is off reads no key and sends nothing", async () => {
  status = { ...status, enabled: false };
  keys = [expired("a")];
  setForeground(false);
  await mount();
  await tick(120_000);
  expect(native.invoke.mock.calls.some(([command]) => command === "keys_list")).toBe(false);
  expect(sent()).toHaveLength(0);
});

test("an unlock in the background sends one count-only notification", async () => {
  keys = [expired("a"), expired("b"), fine("c")];
  setForeground(false);
  await mount();
  expect(sent()).toEqual([["reminders_notify", { count: 2 }]]);
  await tick(120_000);
  expect(sent()).toHaveLength(1);
});

test("in the foreground nothing is sent; once the window goes to the background a rise is announced", async () => {
  keys = [expired("a")];
  await mount();
  expect(sent()).toHaveLength(0);
  keys = [expired("a"), expired("b")];
  await tick();
  expect(sent()).toHaveLength(0);
  setForeground(false);
  keys = [expired("a"), expired("b"), expired("c")];
  await tick();
  expect(sent()).toEqual([["reminders_notify", { count: 3 }]]);
  await tick();
  expect(sent()).toHaveLength(1);
  keys = [expired("a")];
  await tick();
  keys = [expired("a"), expired("b")];
  await tick();
  expect(sent()).toEqual([["reminders_notify", { count: 3 }], ["reminders_notify", { count: 2 }]]);
});

test("a key that enters the due-soon window while the window is in the background is announced by the clock alone", async () => {
  const early = key("a", "Early key", { expiresAt: "2026-05-01T12:00:00Z" });
  const late = key("b", "Late key", { expiresAt: "2026-07-01T12:00:40Z" });
  keys = [early, late];
  setForeground(false);
  await mount();
  expect(sent()).toEqual([["reminders_notify", { count: 1 }]]);
  await tick(30_000);
  expect(sent()).toHaveLength(1);
  await tick(30_000);
  expect(sent()).toEqual([["reminders_notify", { count: 1 }], ["reminders_notify", { count: 2 }]]);
});

test("the notification carries no title, project, environment or value", async () => {
  keys = [expired("a"), expired("b")];
  setForeground(false);
  await mount();
  const everything = JSON.stringify(native.invoke.mock.calls.filter(([command]) => command === "reminders_notify"));
  for (const private_ of ["Private title", "secret-project", "prod", "FAKE_KEY"]) expect(everything).not.toContain(private_);
  expect(everything).toBe('[["reminders_notify",{"count":2}]]');
});

test("a denied permission or a block sends nothing and reads no key", async () => {
  keys = [expired("a")];
  setForeground(false);
  for (const refused of [{ permission: "denied" }, { permission: "prompt" }, { blocked: true }] as Partial<ReminderStatus>[]) {
    status = { enabled: true, permission: "granted", delivery: "none", blocked: false, ...refused };
    native.invoke.mockClear();
    await mount();
    await tick(90_000);
    expect(sent(), JSON.stringify(refused)).toHaveLength(0);
    expect(native.invoke.mock.calls.some(([command]) => command === "keys_list")).toBe(false);
    cleanup();
  }
});

test("a native refusal does not start a loop: the next looks see the block and stop", async () => {
  keys = [expired("a")];
  setForeground(false);
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "reminders_status") return status;
    if (command === "keys_list") return keys;
    if (command === "reminders_notify") {
      status = { ...status, delivery: "failed", blocked: true };
      throw { kind: "internal", message: "fake refusal" };
    }
    throw new Error(command);
  });
  await mount();
  expect(sent()).toHaveLength(1);
  keys = [expired("a"), expired("b")];
  await tick(300_000);
  expect(sent()).toHaveLength(1);
});

test("unmounting stops every look", async () => {
  keys = [expired("a")];
  const view = render(<AttentionNotifier />);
  await act(async () => {});
  view.unmount();
  native.invoke.mockClear();
  await tick(300_000);
  expect(native.invoke).not.toHaveBeenCalled();
});

test("a lock removes the notifier: no look and no notification afterwards", async () => {
  keys = [expired("a")];
  await act(async () => { render(<App />); });
  await tick(100);
  expect(screen.getByRole("button", { name: /^Open / })).toBeTruthy();
  expect(native.invoke.mock.calls.some(([command]) => command === "reminders_status")).toBe(true);
  setForeground(false);
  act(() => native.onLock?.({ payload: { sessionId: owner } }));
  native.invoke.mockClear();
  await tick(300_000);
  expect(native.invoke.mock.calls.filter(([command]) => ["keys_list", "reminders_status", "reminders_notify"].includes(command))).toHaveLength(0);
});
