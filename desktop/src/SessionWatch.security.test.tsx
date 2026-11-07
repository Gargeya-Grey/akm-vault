import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { confirmNativeLock, nativeOwner } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import type { Status } from "./types";

const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked secrets region</p> }));
vi.mock("./components/Unlock", () => ({ Unlock: () => <p>Unlock screen</p> }));

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();
const removed: string[] = [];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const flush = () => act(async () => {});
const calls = (command: string) => native.invoke.mock.calls.filter(([name]) => name === command);

// The native side as a state machine. A status call computes its answer when the call
// arrives, like the real command, and may deliver it later.
const world = { owner: crypto.randomUUID() as string | null, hold: undefined as undefined | Promise<unknown> };
function status(): Status {
  return { sessionId: world.owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: world.owner !== null, keyCount: 1, vaultName: world.owner ? "Fake" : null, needsUpgrade: false };
}
function installNative() {
  native.invoke.mockReset().mockImplementation(async (command: string, args?: { sessionId?: string | null }) => {
    if (command === "vault_status") { const answer = status(); if (world.hold) return world.hold; return answer; }
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") {
      if (args?.sessionId !== world.owner) throw { kind: "locked", message: "A different native session is current." };
      world.owner = null;
      return;
    }
    throw new Error(command);
  });
}
function registerNormally(name: string, handler: Handler) {
  handlers.set(name, handler);
  return Promise.resolve(() => { removed.push(name); if (handlers.get(name) === handler) handlers.delete(name); });
}

beforeEach(() => {
  const previous = nativeOwner();
  if (previous !== null) confirmNativeLock(previous); // Each test starts as a fresh page: no known owner.
  handlers.clear();
  removed.length = 0;
  world.owner = crypto.randomUUID();
  world.hold = undefined;
  installNative();
  native.listen.mockReset().mockImplementation(registerNormally);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
});

test("the first status request waits until both listeners are registered", async () => {
  const lock = deferred<() => void>();
  const expire = deferred<() => void>();
  native.listen.mockImplementation((name: string) => (name === "vault-locked" ? lock.promise : expire.promise));
  render(<App />);
  await flush();
  expect(calls("vault_status")).toHaveLength(0);
  lock.resolve(() => {});
  await flush();
  expect(calls("vault_status")).toHaveLength(0);
  expire.resolve(() => {});
  await screen.findByText("Unlocked secrets region");
  expect(calls("vault_status")).toHaveLength(1);
});

test("a lock that happens while the listeners are registering is never shown as unlocked", async () => {
  const lock = deferred<() => void>();
  const expire = deferred<() => void>();
  native.listen.mockImplementation((name: string) => (name === "vault-locked" ? lock.promise : expire.promise));
  render(<App />);
  await flush();
  world.owner = null; // The event for this lock has no listener yet.
  lock.resolve(() => {});
  expire.resolve(() => {});
  await screen.findByText("Unlock screen");
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
});

for (const failing of ["vault-locked", "vault-view-expired"] as const) {
  test(`a failed ${failing} registration fails closed, locks the native session and blocks opening`, async () => {
    const owner = world.owner;
    native.listen.mockImplementation((name: string, handler: Handler) => (name === failing ? Promise.reject(new Error("registration refused")) : registerNormally(name, handler)));
    render(<App />);
    await screen.findByText("AKM could not watch the session. Restart AKM.");
    expect(screen.queryByText("Unlocked secrets region")).toBeNull();
    expect(screen.queryByText("Unlock screen")).toBeNull();
    expect(calls("vault_lock")).toHaveLength(1);
    expect(calls("vault_lock")[0][1]).toEqual({ sessionId: owner });
    expect(world.owner).toBeNull();
    expect(calls("vault_prepare_open")).toHaveLength(0);
  });
}

test("a registration that never resolves fails closed within the stated deadline", async () => {
  vi.useFakeTimers();
  const owner = world.owner;
  native.listen.mockImplementation((name: string, handler: Handler) => (name === "vault-view-expired" ? new Promise(() => {}) : registerNormally(name, handler)));
  await act(async () => { render(<App />); });
  expect(screen.queryByText("AKM could not watch the session. Restart AKM.")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(2900));
  expect(screen.queryByText("AKM could not watch the session. Restart AKM.")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(200));
  expect(screen.getByText("AKM could not watch the session. Restart AKM.")).toBeTruthy();
  expect(calls("vault_lock")[0][1]).toEqual({ sessionId: owner });
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
});

test("an unconfirmed native lock after a failed registration says so", async () => {
  native.listen.mockImplementation(() => Promise.reject(new Error("registration refused")));
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "vault_warnings") return { application: [], operations: [] };
    throw { kind: "locked", message: "refused" };
  });
  render(<App />);
  await screen.findByText("AKM could not watch the session. Restart AKM.");
  await screen.findByText(/Could not confirm the native lock\. Close AKM before leaving this computer\./);
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
});

test("a replaced root: late registrations of the old root are removed and it never asks for status", async () => {
  const old = { lock: deferred<() => void>(), expire: deferred<() => void>() };
  native.listen.mockImplementationOnce(() => old.lock.promise).mockImplementationOnce(() => old.expire.promise);
  const first = render(<App />);
  await flush();
  first.unmount();
  const lateCleanup = vi.fn();
  old.lock.resolve(lateCleanup);
  old.expire.resolve(lateCleanup);
  await flush();
  expect(lateCleanup).toHaveBeenCalledTimes(2);
  expect(calls("vault_status")).toHaveLength(0);
  render(<App />);
  await screen.findByText("Unlocked secrets region");
  expect(calls("vault_status")).toHaveLength(1);
});

test("a status reply that reaches a replaced root cannot change the shared owner state", async () => {
  const reply = deferred<Status>();
  world.hold = reply.promise;
  const first = render(<App />);
  await flush();
  expect(calls("vault_status")).toHaveLength(1);
  first.unmount();
  const second = deferred<() => void>();
  const secondExpire = deferred<() => void>();
  native.listen.mockImplementation((name: string) => (name === "vault-locked" ? second.promise : secondExpire.promise));
  render(<App />);
  await flush();
  await act(async () => reply.resolve(status()));
  expect(nativeOwner()).not.toBe(world.owner);
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  second.resolve(() => {});
  secondExpire.resolve(() => {});
  world.hold = undefined;
  await screen.findByText("Unlocked secrets region");
});

test("an unlocked view with no accepted status for nine seconds locks natively", async () => {
  vi.useFakeTimers();
  const owner = world.owner;
  await act(async () => { render(<App />); });
  expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
  world.hold = new Promise(() => {}); // The status channel hangs; the lock channel still works.
  await act(() => vi.advanceTimersByTimeAsync(8000));
  expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.getByText("Unlock screen")).toBeTruthy();
  expect(calls("vault_lock")[0][1]).toEqual({ sessionId: owner });
});

for (const event of ["old-a", "current-b"] as const) {
  test(`a ${event === "old-a" ? "late lock event of the older session A" : "real lock of the current session B"} while the first status is in flight ${event === "old-a" ? "never locks B" : "always wins over B's unlocked status"}`, async () => {
    const a = crypto.randomUUID();
    const b = crypto.randomUUID();
    const reply = deferred<Status>();
    world.hold = reply.promise;
    world.owner = b;
    render(<App />);
    await flush();
    expect(calls("vault_status")).toHaveLength(1);
    act(() => handlers.get("vault-locked")?.({ payload: { sessionId: event === "old-a" ? a : b } }));
    await act(async () => reply.resolve({ ...status(), sessionId: b }));
    if (event === "old-a") {
      expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
      expect(nativeOwner()).toBe(b);
      act(() => handlers.get("vault-locked")?.({ payload: { sessionId: b } }));
      expect(screen.queryByText("Unlocked secrets region")).toBeNull();
    } else {
      expect(screen.queryByText("Unlocked secrets region")).toBeNull();
      expect(nativeOwner()).not.toBe(b);
    }
  });
}
