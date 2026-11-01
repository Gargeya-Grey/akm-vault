import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "W:/API Keys Manager/desktop/src/App";
import { api } from "W:/API Keys Manager/desktop/src/api";

const native = vi.hoisted(() => ({
  active: true,
  onLock: undefined as undefined | (() => void),
  rejectSave: undefined as undefined | ((error: unknown) => void),
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => {
    native.onLock = callback;
    return () => { native.onLock = undefined; };
  }),
}));
vi.mock("W:/API Keys Manager/desktop/src/theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));

afterEach(cleanup);

test("a completed export from an older session must not strand a newer session in a false-locked state", async () => {
  const status = () => ({ path: "C:/fake/vault.kdbx", exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "env_save") return new Promise((_resolve, reject) => { native.rejectSave = reject; });
    if (command === "vault_unlock") {
      if (native.active) native.onLock?.();
      native.active = true;
      return status();
    }
    if (command === "vault_lock") {
      native.active = false;
      native.onLock?.();
      return;
    }
    if (command === "vault_warnings") return [];
    if (command === "vault_activity") return;
    throw new Error(`Unexpected mocked command ${command}`);
  });
  render(<App />);
  await screen.findByText("Unlocked region");
  const oldExport = api.saveEnv("demo", "dev").catch((error: unknown) => error);
  expect(native.rejectSave).toBeTypeOf("function");
  act(() => { native.active = false; native.onLock!(); });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("Unlocked region");
  await act(async () => {
    native.rejectSave!({ kind: "completedWhileLocked", message: "The older export completed. Do not repeat it." });
    await oldExport;
  });
  expect(native.active).toBe(true);
  expect(screen.queryByText("Unlocked region")).toBeNull();
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-correct-password" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Unlock vault" })));
  const unlocks = native.invoke.mock.calls.filter(([command]) => command === "vault_unlock").length;
  const locks = native.invoke.mock.calls.filter(([command]) => command === "vault_lock").length;
  expect(screen.queryByText("Unlocked region"), `native remains active=${native.active}, unlocks=${unlocks}, native locks=${locks}`).not.toBeNull();
});
