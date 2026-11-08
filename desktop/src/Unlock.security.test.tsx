import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";

const native = vi.hoisted(() => ({
  owner: "",
  onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void),
  status: vi.fn(), unlock: vi.fn(), create: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (command: string, args: unknown) => {
  if (command === "vault_status") return native.status();
  if (command === "vault_prepare_open") return native.owner;
  if (command === "vault_unlock") return native.unlock(args);
  if (command === "vault_create") return native.create(args);
  if (command === "vault_warnings") return { application: [], operations: [] };
  if (command === "vault_activity") return;
  throw new Error(command);
} }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
beforeEach(() => { vi.clearAllMocks(); native.owner = crypto.randomUUID(); });
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

for (const operation of ["unlock", "create"] as const) {
  test(`a delayed ${operation} response cannot reopen after a native lock`, async () => {
    const status = { sessionId: null, path: "C:/fake/vault.kdbx", exists: operation === "unlock", unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false };
    native.status.mockResolvedValue(status);
    let resolve!: (value: unknown) => void;
    native[operation].mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<App />);
    const label = operation === "unlock" ? "Unlock vault" : "Create vault";
    await screen.findByRole("button", { name: label });
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-review-password" } });
    if (operation === "create") fireEvent.change(screen.getByLabelText("Confirm password"), { target: { value: "fake-review-password" } });
    fireEvent.click(screen.getByRole("button", { name: label }));
    await act(async () => {}); // Reservation acknowledgment precedes credential dispatch.
    expect(native[operation]).toHaveBeenCalled();
    act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
    await act(async () => resolve({ ...status, sessionId: native.owner, unlocked: true }));
    expect(screen.queryByText("Unlocked region")).toBeNull();
    expect((screen.getByLabelText("Master password") as HTMLInputElement).value).toBe("");
  });
}

test("the pins and the length rule count Unicode characters, as the native rule does", async () => {
  native.status.mockResolvedValue({ sessionId: null, path: "C:/fake/vault.kdbx", exists: false, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  render(<App />);
  const create = await screen.findByRole("button", { name: "Create vault" });
  const enter = (password: string) => {
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: password } });
    fireEvent.change(screen.getByLabelText("Confirm password"), { target: { value: password } });
  };
  const pins = () => document.querySelector(".pins-state")?.textContent;

  enter("\u{1F511}".repeat(6));
  expect(pins()).toBe("6 of 12 pins set");
  fireEvent.click(create);
  await act(async () => {});
  expect(native.create).not.toHaveBeenCalled();
  await screen.findByText("Use at least 12 characters for the master password.");

  enter("\u{1F511}".repeat(12));
  expect(pins()).toBe("12 of 12 pins set");
});

test("a terminal authentication error remains visible on the locked form", async () => {
  native.status.mockResolvedValue({ sessionId: null, path: "C:/fake/vault.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  native.unlock.mockRejectedValueOnce("That password does not open this vault.");
  render(<App />);
  await screen.findByRole("button", { name: "Unlock vault" });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-wrong-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("That password does not open this vault.");
  expect(screen.queryByText("Unlocked region")).toBeNull();
});
