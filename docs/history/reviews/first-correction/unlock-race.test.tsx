import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "W:/API Keys Manager/desktop/src/App";

const native = vi.hoisted(() => ({
  onLock: undefined as undefined | (() => void),
  status: vi.fn(), unlock: vi.fn(),
  activity: vi.fn().mockResolvedValue(undefined), warnings: vi.fn().mockResolvedValue([]),
}));
vi.mock("W:/API Keys Manager/desktop/src/api.ts", () => ({ api: native, errorText: String }));
vi.mock("W:/API Keys Manager/desktop/node_modules/@tauri-apps/api/event.js", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => { native.onLock = callback; return () => {}; }),
}));
vi.mock("W:/API Keys Manager/desktop/src/theme.ts", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView.tsx", () => ({ KeysView: () => <p>Unlocked region</p> }));
afterEach(cleanup);

test("an unlock reply overtaken by native lock must not reopen the UI", async () => {
  const status = { path: "C:/fake/vault.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false };
  native.status.mockResolvedValue(status);
  let resolve!: (value: unknown) => void;
  native.unlock.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<App />);
  await screen.findByRole("button", { name: "Unlock vault" });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-review-password" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
  expect(native.unlock).toHaveBeenCalled();
  act(() => native.onLock?.());
  await act(async () => resolve({ ...status, unlocked: true }));
  expect(screen.queryByText("Unlocked region")).toBeNull();
});
