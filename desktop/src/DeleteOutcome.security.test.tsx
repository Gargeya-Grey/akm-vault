import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
afterEach(cleanup);

for (const warningKind of ["backupRefresh", "backupRetention"] as const) {
test(`a committed delete with ${warningKind} does not promise recovery`, async () => {
  let deleted = false;
  const sessionId = crypto.randomUUID();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return { sessionId, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: deleted ? 0 : 1, vaultName: "Fake", needsUpgrade: false };
    if (command === "keys_list") return [{ id: "fake-id", title: "Delete proof", username: null, issuer: null, project: null, environment: null, scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, expired: false, rotationDueIn: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0 }];
    if (command === "key_remove") { deleted = true; return; }
    if (command === "vault_warnings") return { application: [], operations: deleted ? [{ id: `delete-${warningKind}-warning`, originSession: "A", kind: warningKind }] : [] };
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  // Flush the immediate mocked bootstrap and key-list promises before looking
  // up the row; this does not rely on wall-clock scheduling between workers.
  await act(async () => { render(<App />); });
  fireEvent.click(await screen.findByRole("button", { name: "Open Delete proof" }));
  fireEvent.click(screen.getByRole("button", { name: "Move to Recycle Bin" }));
  expect(document.body.textContent).not.toContain("stays in its .bak copy");
  expect(document.body.textContent).not.toContain("AKM retains a dated encrypted");
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move to Recycle Bin" }));
  await screen.findByText(/Moved “Delete proof” to the Recycle Bin/);
  expect(document.body.textContent).not.toContain("A backup remains in the .bak file");
  expect(document.body.textContent).not.toContain("The previous vault was retained");
  expect(document.body.textContent).toContain("backup notices");
  await act(() => api.warnings());
  const notices = screen.getByRole("region", { name: "Saved-operation notices" }).textContent;
  expect(notices).toContain(warningKind === "backupRefresh" ? "convenience backup could not be refreshed" : "retention of its pre-change encrypted backup could not be confirmed");
  expect(notices).not.toContain("saved with a retained");
});
}
