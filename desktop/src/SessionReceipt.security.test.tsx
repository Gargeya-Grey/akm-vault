import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { acknowledgeCompletions, COMPLETION_MESSAGE, retainedCompletions } from "./sessionBoundary";

const native = vi.hoisted(() => ({
  active: true,
  owner: "",
  onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void),
  rejectSave: undefined as undefined | ((error: unknown) => void),
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
beforeEach(() => { native.active = true; native.owner = crypto.randomUUID(); });
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

for (const between of [true, false]) {
 for (const reply of ["completedWhileLocked", "success"]) {
  test(`A's ${reply} delivered ${between ? "between" : "after"} roots replays without false-locking B`, async () => {
    let deliverSave!: () => void;
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
      if (command === "env_save") return new Promise((done, reject) => { deliverSave = () => reply === "success" ? done("C:/fake/private-a-export.env") : reject({ kind: "completedWhileLocked", message: "fake native completion" }); });
      if (command === "vault_warnings") return { application: [], operations: [] };
      if (command === "vault_activity") return;
      throw new Error(command);
    });
    const a = render(<App />);
    await screen.findByText("Unlocked region");
    const pending = api.saveEnv("fake-a", "dev").catch((error: unknown) => error);
    a.unmount();
    if (!between) { render(<App />); await screen.findByText("Unlocked region"); }
    await act(async () => { deliverSave(); await pending; });
    if (between) { render(<App />); await screen.findByText("Unlocked region"); }
    expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
    expect(screen.getByText("Unlocked region")).toBeTruthy();
    expect(document.body.textContent).not.toContain("private-a-export.env");
    fireEvent.click(screen.getByRole("button", { name: "Dismiss operation outcomes" }));
    cleanup();
    render(<App />);
    await screen.findByText("Unlocked region");
    expect(screen.queryByText(COMPLETION_MESSAGE)).toBeNull();
  });
 }
}

test("a completed export receipt from session A cannot false-lock active session B", async () => {
  const status = () => ({ sessionId: native.active ? native.owner : null, path: "C:/fake/vault.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "env_save") return new Promise((_done, reject) => { native.rejectSave = reject; });
    if (command === "vault_prepare_open") {
      if (native.active) native.onLock?.({ payload: { sessionId: native.owner } });
      native.active = false;
      native.owner = crypto.randomUUID();
      return native.owner;
    }
    if (command === "vault_unlock") {
      native.active = true;
      return status();
    }
    if (command === "vault_lock") { native.active = false; native.onLock?.({ payload: { sessionId: native.owner } }); return; }
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    throw new Error(`Unexpected test command ${command}`);
  });
  render(<App />);
  await screen.findByText("Unlocked region");
  const oldExport = api.saveEnv("demo", "dev").catch((error: unknown) => error);
  act(() => { native.active = false; native.onLock?.({ payload: { sessionId: native.owner } }); });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("Unlocked region");
  await act(async () => {
    native.rejectSave?.({ kind: "completedWhileLocked", message: "The older export completed. Do not repeat it." });
    await oldExport;
  });
  expect(native.active).toBe(true);
  expect(screen.getByText("Unlocked region")).toBeTruthy();
  expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
  expect(native.invoke.mock.calls.filter(([command]) => command === "vault_unlock")).toHaveLength(1);
});

for (const completion of ["success", "failure"] as const) {
  test(`an old lock ${completion} reply cannot clear or replace a newer session error`, async () => {
    native.active = true;
    let resolveLock!: () => void;
    let rejectLock!: (cause: unknown) => void;
    const status = () => ({ sessionId: native.active ? native.owner : null, path: "C:/fake/vault.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return status();
      if (command === "vault_lock") {
        native.active = false;
        native.onLock?.({ payload: { sessionId: native.owner } });
        return new Promise<void>((done, failed) => { resolveLock = done; rejectLock = failed; });
      }
      if (command === "vault_prepare_open") { native.owner = crypto.randomUUID(); return native.owner; }
      if (command === "vault_unlock") { native.active = true; return status(); }
      if (command === "vault_reload") throw { kind: "io", message: "The newer session could not reload. Keep this error." };
      if (command === "vault_warnings") return { application: [], operations: [] };
      if (command === "vault_activity") return;
      throw new Error(`Unexpected test command ${command}`);
    });
    render(<App />);
    await screen.findByText("Unlocked region");
    fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
    await screen.findByText("Unlocked region");
    fireEvent.click(screen.getByRole("button", { name: "Reload vault" }));
    await screen.findByText("The newer session could not reload. Keep this error.");
    await act(async () => {
      if (completion === "success") resolveLock(); else rejectLock("obsolete lock failure");
    });
    expect(screen.getByText("The newer session could not reload. Keep this error.")).toBeTruthy();
    expect(screen.getByText("Unlocked region")).toBeTruthy();
    expect(native.active).toBe(true);
  });
}
