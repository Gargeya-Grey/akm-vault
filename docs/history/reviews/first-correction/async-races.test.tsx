import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "W:/API Keys Manager/desktop/src/App";
import { Settings } from "W:/API Keys Manager/desktop/src/components/Settings";

const native = vi.hoisted(() => ({
  onLock: undefined as undefined | (() => void),
  status: vi.fn(), reload: vi.fn(), upgrade: vi.fn(),
  lock: vi.fn().mockResolvedValue(undefined), activity: vi.fn().mockResolvedValue(undefined),
  warnings: vi.fn().mockResolvedValue([]), backup: vi.fn().mockResolvedValue({ exists: false }),
  keys: vi.fn().mockResolvedValue([]), exportEnv: vi.fn(), previewImport: vi.fn(), confirmImport: vi.fn().mockResolvedValue({ added: 1, updated: 0 }),
}));
vi.mock("W:/API Keys Manager/desktop/src/api.ts", () => ({ api: native, errorText: String }));
vi.mock("W:/API Keys Manager/desktop/node_modules/@tauri-apps/api/event.js", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => { native.onLock = callback; return () => {}; }),
}));
vi.mock("W:/API Keys Manager/desktop/src/theme.ts", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView.tsx", () => ({ KeysView: () => <p>Unlocked region</p> }));
vi.mock("W:/API Keys Manager/desktop/src/components/Unlock.tsx", () => ({ Unlock: () => <p>Unlock screen</p> }));

const status = { path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Fake", needsUpgrade: false };
beforeEach(() => { vi.clearAllMocks(); native.status.mockResolvedValue(status); });
afterEach(cleanup);

test("reload reply after native lock must not reopen the UI", async () => {
  let resolve!: (value: unknown) => void;
  native.reload.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<App />);
  await screen.findByText("Unlocked region");
  fireEvent.click(screen.getByRole("button", { name: "Reload vault" }));
  act(() => native.onLock?.());
  expect(screen.getByText("Unlock screen")).toBeTruthy();
  await act(async () => resolve(status));
  expect(screen.queryByText("Unlocked region")).toBeNull();
});

test("upgrade reply after native lock must not reopen the UI", async () => {
  let resolve!: (value: unknown) => void;
  native.status.mockResolvedValueOnce({ ...status, needsUpgrade: true });
  native.upgrade.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<App />);
  await screen.findByText("Unlocked region");
  fireEvent.click(screen.getByRole("button", { name: "Review format upgrade" }));
  fireEvent.click(screen.getByRole("button", { name: "Upgrade format" }));
  act(() => native.onLock?.());
  expect(screen.getByText("Unlock screen")).toBeTruthy();
  await act(async () => resolve(status));
  expect(screen.queryByText("Unlocked region")).toBeNull();
});

test("editing import scope during preview must invalidate the old response", async () => {
  let resolve!: (value: unknown) => void;
  native.previewImport.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake" } });
  fireEvent.change(screen.getByLabelText("Target project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Target environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  expect(native.previewImport).toHaveBeenCalledWith("TOKEN=fake", "demo", "prod");
  fireEvent.change(screen.getByLabelText("Target environment"), { target: { value: "dev" } });
  await act(async () => resolve({ token: "prod-token", added: 1, updated: 0, unchanged: 0, changes: [{ name: "TOKEN", title: "TOKEN", action: "add" }] }));
  expect(screen.getByRole("region", { name: "Import review" }).textContent).toContain("demo / dev");
  fireEvent.click(screen.getByRole("button", { name: "Confirm import" }));
  await act(async () => {});
  expect(native.confirmImport).not.toHaveBeenCalledWith("prod-token");
});

test("export scope changes must invalidate an outstanding plaintext preview", async () => {
  let resolve!: (value: string) => void;
  native.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }, { project: "demo", environment: "dev" }]);
  native.exportEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  await act(async () => {});
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  expect(native.exportEnv).toHaveBeenCalledWith("demo", "prod");
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  await act(async () => resolve("TOKEN=fake-prod-secret\n"));
  expect(screen.queryByText("TOKEN=fake-prod-secret")).toBeNull();
});
