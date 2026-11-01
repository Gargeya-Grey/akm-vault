import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "W:/API Keys Manager/desktop/src/App";
import { api } from "W:/API Keys Manager/desktop/src/api";
import { COMPLETION_MESSAGE } from "W:/API Keys Manager/desktop/src/sessionBoundary";

const native = vi.hoisted(() => ({
  active: true,
  onLock: undefined as undefined | (() => void),
  invoke: vi.fn(),
  save: undefined as undefined | Promise<unknown>,
  locking: undefined as undefined | Promise<unknown>,
  reloading: undefined as undefined | Promise<unknown>,
  warning: undefined as undefined | Promise<unknown>,
}));
vi.mock("W:/API Keys Manager/desktop/node_modules/@tauri-apps/api/core.js", () => ({ invoke: native.invoke }));
vi.mock("W:/API Keys Manager/desktop/node_modules/@tauri-apps/api/event.js", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => {
    native.onLock = callback;
    return () => { native.onLock = undefined; };
  }),
}));
vi.mock("W:/API Keys Manager/desktop/src/theme.ts", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView.tsx", () => ({ KeysView: () => <p>Unlocked region</p> }));

function deferred<T = unknown>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
function status() {
  return { path: "C:/fake/acceptance.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false };
}
function signalLock() { native.active = false; native.onLock?.(); }
async function unlockB() {
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("Unlocked region");
}
beforeEach(() => {
  native.active = true;
  native.save = undefined;
  native.locking = undefined;
  native.reloading = undefined;
  native.warning = undefined;
  native.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "env_save") return native.save;
    if (command === "vault_unlock") {
      if (native.active) native.onLock?.();
      native.active = true;
      return status();
    }
    if (command === "vault_lock") { signalLock(); return native.locking; }
    if (command === "vault_reload") return native.reloading ?? status();
    if (command === "vault_warnings") return native.warning ?? [];
    if (command === "vault_activity") return;
    throw new Error(`Unexpected command ${command}`);
  });
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

for (const result of ["native-receipt", "successful-save"] as const) {
  test(`old A ${result} acknowledges without hiding active B`, async () => {
    const saving = deferred();
    native.save = saving.promise;
    render(<App />);
    await screen.findByText("Unlocked region");
    const outcome = api.saveEnv("demo", "dev").catch((error: unknown) => error);
    act(signalLock);
    await unlockB();
    await act(async () => {
      if (result === "native-receipt") saving.reject({ kind: "completedWhileLocked", message: "Do not repeat" });
      else saving.resolve("C:/fake/owned-export.env");
      await outcome;
    });
    expect(screen.getByText("Unlocked region")).toBeTruthy();
    expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
    expect(native.active).toBe(true);
    act(signalLock);
    await unlockB();
    expect(native.invoke.mock.calls.filter(([command]) => command === "vault_unlock")).toHaveLength(2);
  });
}

test("a current-generation native receipt hides only the unavailable session and permits a new unlock", async () => {
  const saving = deferred();
  native.save = saving.promise;
  render(<App />);
  await screen.findByText("Unlocked region");
  const outcome = api.saveEnv("demo", "dev").catch((error: unknown) => error);
  await act(async () => {
    native.active = false;
    saving.reject({ kind: "completedWhileLocked", message: "Do not repeat" });
    await outcome;
  });
  expect(screen.queryByText("Unlocked region")).toBeNull();
  expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
  await unlockB();
});

test("an old reload rejection cannot replace B's visible state", async () => {
  const reloading = deferred();
  native.reloading = reloading.promise;
  render(<App />);
  await screen.findByText("Unlocked region");
  fireEvent.click(screen.getByRole("button", { name: "Reload vault" }));
  act(signalLock);
  await unlockB();
  await act(async () => reloading.reject({ kind: "io", message: "obsolete A reload failure" }));
  expect(screen.queryByText("obsolete A reload failure")).toBeNull();
  expect(screen.getByText("Unlocked region")).toBeTruthy();
});

test("an old warning reply cannot appear in B", async () => {
  vi.useFakeTimers();
  await act(async () => { render(<App />); });
  const warning = deferred();
  native.warning = warning.promise;
  await act(() => vi.advanceTimersByTimeAsync(3000));
  act(signalLock);
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Unlock vault" })));
  await act(async () => warning.resolve(["obsolete A warning"]));
  expect(screen.queryByText("obsolete A warning")).toBeNull();
  expect(screen.getByText("Unlocked region")).toBeTruthy();
});

test("a delayed A lock acknowledgment must not clear a newer B reload failure", async () => {
  const locking = deferred();
  native.locking = locking.promise;
  render(<App />);
  await screen.findByText("Unlocked region");
  fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
  await unlockB();
  native.reloading = Promise.reject({ kind: "changedOnDisk", message: "B could not reload its changed file" });
  fireEvent.click(screen.getByRole("button", { name: "Reload vault" }));
  await screen.findByText("B could not reload its changed file");
  await act(async () => locking.resolve(undefined));
  expect(screen.getByText("B could not reload its changed file")).toBeTruthy();
  expect(screen.getByText("Unlocked region")).toBeTruthy();
});
