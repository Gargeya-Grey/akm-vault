import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { COMPLETION_MESSAGE } from "./sessionBoundary";

const native = vi.hoisted(() => ({
  active: true,
  onLock: undefined as undefined | (() => void),
  rejectSave: undefined as undefined | ((error: unknown) => void),
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => { native.onLock = callback; return () => { native.onLock = undefined; }; }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
afterEach(cleanup);

test("a completed export receipt from session A cannot false-lock active session B", async () => {
  const status = () => ({ path: "C:/fake/vault.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "env_save") return new Promise((_done, reject) => { native.rejectSave = reject; });
    if (command === "vault_unlock") {
      if (native.active) native.onLock?.();
      native.active = true;
      return status();
    }
    if (command === "vault_lock") { native.active = false; native.onLock?.(); return; }
    if (command === "vault_warnings") return [];
    if (command === "vault_activity") return;
    throw new Error(`Unexpected test command ${command}`);
  });
  render(<App />);
  await screen.findByText("Unlocked region");
  const oldExport = api.saveEnv("demo", "dev").catch((error: unknown) => error);
  act(() => { native.active = false; native.onLock?.(); });
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
    const status = () => ({ path: "C:/fake/vault.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return status();
      if (command === "vault_lock") {
        native.active = false;
        native.onLock?.();
        return new Promise<void>((done, failed) => { resolveLock = done; rejectLock = failed; });
      }
      if (command === "vault_unlock") { native.active = true; return status(); }
      if (command === "vault_reload") throw { kind: "io", message: "The newer session could not reload. Keep this error." };
      if (command === "vault_warnings") return [];
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
