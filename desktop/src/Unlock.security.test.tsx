import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";

const native = vi.hoisted(() => ({
  onLock: undefined as undefined | (() => void),
  status: vi.fn(), unlock: vi.fn(), create: vi.fn(),
  activity: vi.fn().mockResolvedValue(undefined), warnings: vi.fn().mockResolvedValue([]),
}));
vi.mock("./api", () => ({ api: native, errorText: String }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => { native.onLock = callback; return () => { native.onLock = undefined; }; }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

for (const operation of ["unlock", "create"] as const) {
  test(`a delayed ${operation} response cannot reopen after a native lock`, async () => {
    const status = { path: "C:/fake/vault.kdbx", exists: operation === "unlock", unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false };
    native.status.mockResolvedValue(status);
    let resolve!: (value: unknown) => void;
    native[operation].mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<App />);
    const label = operation === "unlock" ? "Unlock vault" : "Create vault";
    await screen.findByRole("button", { name: label });
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-review-password" } });
    if (operation === "create") fireEvent.change(screen.getByLabelText("Confirm password"), { target: { value: "fake-review-password" } });
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(native[operation]).toHaveBeenCalled();
    act(() => native.onLock?.());
    await act(async () => resolve({ ...status, unlocked: true }));
    expect(screen.queryByText("Unlocked region")).toBeNull();
    expect((screen.getByLabelText("Master password") as HTMLInputElement).value).toBe("");
  });
}

test("a terminal authentication error remains visible on the locked form", async () => {
  native.status.mockResolvedValue({ path: "C:/fake/vault.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  native.unlock.mockRejectedValueOnce("That password does not open this vault.");
  render(<App />);
  await screen.findByRole("button", { name: "Unlock vault" });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-wrong-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  await screen.findByText("That password does not open this vault.");
  expect(screen.queryByText("Unlocked region")).toBeNull();
});
