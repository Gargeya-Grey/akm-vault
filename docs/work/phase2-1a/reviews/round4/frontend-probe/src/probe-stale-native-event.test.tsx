import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";

const native = vi.hoisted(() => ({ active: true, path: "C:/fake/a.kdbx", onLock: undefined as undefined | (() => void), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_event: string, cb: () => void) => { native.onLock = cb; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Active vault view</p> }));
afterEach(cleanup);

test("late delivery of an old native lock event hides an already open B without locking B", async () => {
  native.active = true;
  native.path = "C:/fake/a.kdbx";
  const status = () => ({ path: native.path, exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string, args?: { path?: string }) => {
    if (command === "vault_status") return status();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") { native.active = false; return; }
    if (command === "vault_unlock") { native.path = args?.path ?? native.path; native.active = true; return status(); }
    throw new Error(command);
  });
  render(<App />);
  await screen.findByText("Active vault view");
  fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
  expect(native.active).toBe(false);
  fireEvent.change(screen.getByLabelText("Vault file"), { target: { value: "C:/fake/b.kdbx" } });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-b" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("Active vault view");
  expect(native.active).toBe(true);
  expect(native.path).toBe("C:/fake/b.kdbx");
  // The queued A event is delivered after B's successful open.
  act(() => native.onLock?.());
  expect(screen.queryByText("Active vault view")).toBeNull();
  expect(screen.getByRole("button", { name: "Unlock vault" })).toBeTruthy();
  expect(native.active).toBe(true);
  expect(native.path).toBe("C:/fake/b.kdbx");
});

test("an old lock event arriving during B's successful native unlock cancels its UI reply", async () => {
  native.active = true;
  native.path = "C:/fake/a.kdbx";
  let finishUnlock!: (status: unknown) => void;
  const status = () => ({ path: native.path, exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string, args?: { path?: string }) => {
    if (command === "vault_status") return status();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") { native.active = false; return; }
    if (command === "vault_unlock") {
      native.path = args?.path ?? native.path;
      native.active = true;
      return new Promise((resolve) => { finishUnlock = resolve; });
    }
    throw new Error(command);
  });
  render(<App />);
  await screen.findByText("Active vault view");
  fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
  fireEvent.change(screen.getByLabelText("Vault file"), { target: { value: "C:/fake/b.kdbx" } });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-b" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  expect(native.active).toBe(true);
  act(() => native.onLock?.());
  await act(async () => finishUnlock(status()));
  expect(screen.queryByText("Active vault view")).toBeNull();
  expect(screen.getByRole("button", { name: "Unlock vault" })).toBeTruthy();
  expect(native.active).toBe(true);
  expect(native.path).toBe("C:/fake/b.kdbx");
});
