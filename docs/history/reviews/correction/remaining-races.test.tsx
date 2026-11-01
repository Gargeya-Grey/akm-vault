import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "W:/API Keys Manager/desktop/src/App";
import { Settings } from "W:/API Keys Manager/desktop/src/components/Settings";

const native = vi.hoisted(() => ({
  onLock: undefined as undefined | (() => void),
  status: vi.fn(), unlock: vi.fn(),
  backup: vi.fn(), keys: vi.fn(), previewImport: vi.fn(), confirmImport: vi.fn(),
  activity: vi.fn(), warnings: vi.fn(),
}));
vi.mock("W:/API Keys Manager/desktop/src/api", () => ({ api: native, errorText: String }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => {
    native.onLock = callback;
    return () => { native.onLock = undefined; };
  }),
}));
vi.mock("W:/API Keys Manager/desktop/src/theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));

const status = { path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Fake", needsUpgrade: false };

beforeEach(() => {
  vi.resetAllMocks();
  native.backup.mockResolvedValue({ exists: false });
  native.keys.mockResolvedValue([]);
  native.activity.mockResolvedValue(undefined);
  native.warnings.mockResolvedValue([]);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

test("an unlock failure is still explained after its own native vault-locked notification", async () => {
  native.status.mockResolvedValue({ ...status, unlocked: false, keyCount: 0, vaultName: null });
  let reject!: (reason: unknown) => void;
  native.unlock.mockReturnValueOnce(new Promise((_resolve, no) => { reject = no; }));
  render(<App />);
  await screen.findByRole("button", { name: "Unlock vault" });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-wrong-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  expect(native.unlock).toHaveBeenCalledTimes(1);
  act(() => native.onLock!());
  await act(async () => reject(new Error("That password does not open this vault")));
  expect(screen.queryByText("Unlocked region")).toBeNull();
  expect(screen.queryByText(/That password does not open this vault/)).not.toBeNull();
});

test("initial status failure overtaken by lock still exposes a recovery action", async () => {
  let reject!: (reason: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((_resolve, no) => { reject = no; }));
  await act(async () => { render(<App />); });
  expect(native.onLock).toBeTypeOf("function");
  act(() => native.onLock!());
  await act(async () => reject(new Error("fake unavailable settings path")));
  expect(screen.queryByText("Unlocked region")).toBeNull();
  const recovery = screen.queryByRole("button", { name: "Try again" }) ?? screen.queryByRole("button", { name: "Unlock vault" });
  expect(recovery, document.body.innerHTML).not.toBeNull();
});

test("hiding invalidates a pending import review, not only export previews", async () => {
  let resolve!: (value: unknown) => void;
  native.previewImport.mockReturnValueOnce(new Promise((yes) => { resolve = yes; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "prod"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  await act(async () => resolve({ token: "old-hidden-token", added: 1, updated: 0, unchanged: 0, changes: [] }));
  expect(screen.queryByRole("region", { name: "Import review" })).toBeNull();
});

test("reopening import after hiding cannot discard an already-dispatched confirmation acknowledgment", async () => {
  let resolve!: (value: unknown) => void;
  native.previewImport.mockResolvedValueOnce({ token: "confirmed-old-token", added: 1, updated: 0, unchanged: 0, changes: [] });
  native.confirmImport.mockReturnValueOnce(new Promise((yes) => { resolve = yes; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByRole("button", { name: "Confirm import" });
  fireEvent.click(screen.getByRole("button", { name: "Confirm import" }));
  expect(native.confirmImport).toHaveBeenCalledExactlyOnceWith("confirmed-old-token");
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  await act(async () => resolve({ added: 1, updated: 0 }));
  expect(screen.queryByText("Imported: 1 added, 0 updated.")).not.toBeNull();
});
