import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "@project/App";
import { api } from "@project/api";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@project/theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("@project/components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
afterEach(cleanup);

test("a drained warning is lost when its delayed response arrives between root mounts", async () => {
  let resolveWarning!: (value: unknown) => void;
  let path = "C:/fake/a.kdbx";
  let warningCalls = 0;
  const report = { application: [], operations: [{ id: "notice-a", originSession: "session-a", kind: "journal" }] };
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return { path, exists: true, unlocked: true, keyCount: 0, vaultName: "Fake", needsUpgrade: false };
    if (command === "vault_warnings") {
      warningCalls += 1;
      return warningCalls === 1 ? new Promise((resolve) => { resolveWarning = resolve; }) : { application: [], operations: [] };
    }
    if (command === "vault_activity") return;
    throw new Error(`Unexpected command ${command}`);
  });
  const first = render(<App />);
  await screen.findByText("Unlocked region");
  const pending = api.warnings();
  first.unmount();
  await act(async () => { resolveWarning(report); await pending; });
  path = "C:/fake/b.kdbx";
  render(<App />);
  await screen.findByText("Unlocked region");
  await act(async () => { await api.warnings(); });
  expect(warningCalls).toBe(2);
  expect(screen.queryByRole("region", { name: "Saved-operation notices" })).toBeNull();
});
