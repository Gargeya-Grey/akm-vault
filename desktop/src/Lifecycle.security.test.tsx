import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { adoptReservation } from "./nativeSession";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "./sessionBoundary";
import type { KeyView } from "./types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));

const NEW_SECRET = "fake-new-secret-value";
const OLD_SECRET = "fake-old-secret-value";

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Rotation proof", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [], expiresAt: null,
    rotationDays: null, createdAt: null, updatedAt: null, expired: false, rotationDueIn: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); native.invoke.mockReset(); });

function boot(handlers: Record<string, (args?: Record<string, unknown>) => unknown>, keys: () => KeyView[]) {
  const sessionId = crypto.randomUUID();
  native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "vault_status") return { sessionId, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: keys().length, vaultName: "Fake", needsUpgrade: false };
    if (command === "keys_list") return keys();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (handlers[command]) return handlers[command](args);
    throw new Error(command);
  });
}

async function openDetail(title: string) {
  await act(async () => { render(<App />); });
  fireEvent.click(await screen.findByRole("button", { name: `Open ${title}` }));
}

test("Key Detail shows the secret-change time or unknown, and the history count", async () => {
  boot({}, () => [key({ rotationDays: 30 })]);
  await openDetail("Rotation proof");
  let rows = within(screen.getByRole("dialog", { name: "Rotation proof" }));
  expect(rows.getByText("Secret changed").nextSibling?.textContent).toBe("unknown");
  expect(rows.getByText("History").nextSibling?.textContent).toBe("No earlier versions");
  // The rotation age is unknown too: AKM never counts from a time it does not have.
  expect(rows.getByText("Rotation").nextSibling?.textContent).toBe("every 30 days · unknown");
  cleanup();
  boot({}, () => [key({ secretChange: "known", secretChangedAt: "2026-11-07T08:15:33Z", historyCount: 2 })]);
  await openDetail("Rotation proof");
  rows = within(screen.getByRole("dialog", { name: "Rotation proof" }));
  expect(rows.getByText("Secret changed").nextSibling?.textContent).toMatch(/2026/);
  expect(rows.getByText("History").nextSibling?.textContent).toBe("2 earlier versions in the vault history");
  cleanup();
  boot({}, () => [key({ secretChange: "changed_outside" })]);
  await openDetail("Rotation proof");
  rows = within(screen.getByRole("dialog", { name: "Rotation proof" }));
  expect(rows.getByText("Secret changed").nextSibling?.textContent).toBe("unknown · changed outside AKM");
});

test("a key with a rotation interval and no known change time is not shown as on schedule", async () => {
  boot({}, () => [key({ rotationDays: 30 }), key({ id: "other", title: "No interval" }), key({ id: "known", title: "On schedule", rotationDays: 30, rotationDueIn: 20, secretChange: "known", secretChangedAt: "2026-11-07T08:15:33Z" })]);
  await act(async () => { render(<App />); });
  const state = async (title: string) => (await screen.findByRole("button", { name: `Open ${title}` })).querySelector(".state")?.textContent;
  expect(await state("Rotation proof")).toBe("Rotation unknown");
  expect(await state("No interval")).toBe("OK");
  expect(await state("On schedule")).toBe("OK");
  expect((await screen.findByRole("button", { name: "Open Rotation proof" })).textContent).toContain("rotate unknown");
});

test("a rotation sends the value once, clears the field and shows only the receipt", async () => {
  let current = key();
  boot({
    key_rotate: () => {
      current = key({ secretChange: "known", secretChangedAt: "2026-11-07T08:15:33Z", historyCount: 1 });
      return { key: current, changedAt: "2026-11-07T08:15:33Z", previousChangedAt: null };
    },
  }, () => [current]);
  await openDetail("Rotation proof");
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret" }));
  const input = screen.getByLabelText("New secret") as HTMLInputElement;
  expect(input.type).toBe("password");
  fireEvent.change(input, { target: { value: NEW_SECRET } });
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret", hidden: false }));
  await screen.findByText(/Rotated “Rotation proof”/);
  expect(native.invoke.mock.calls.filter(([command]) => command === "key_rotate")).toEqual([["key_rotate", { id: "fake-id", secret: NEW_SECRET }]]);
  expect(screen.queryByLabelText("New secret")).toBeNull();
  expect(document.body.textContent).not.toContain(NEW_SECRET);
  expect(document.body.textContent).toContain("Rotating here does not change the secret at the provider");
  fireEvent.click(screen.getByRole("button", { name: "Open Rotation proof" }));
  expect(within(screen.getByRole("dialog", { name: "Rotation proof" })).getByText("History").nextSibling?.textContent).toBe("1 earlier version in the vault history");
});

test("a refused rotation keeps the dialog open, clears the typed value and claims no success", async () => {
  boot({
    key_rotate: () => { throw { kind: "invalid", message: "the new secret is the same as the current secret, so nothing was changed." }; },
  }, () => [key()]);
  await openDetail("Rotation proof");
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret" }));
  fireEvent.change(screen.getByLabelText("New secret"), { target: { value: OLD_SECRET } });
  fireEvent.click(screen.getAllByRole("button", { name: "Rotate secret" }).at(-1)!);
  await screen.findByText(/nothing was changed/);
  expect((screen.getByLabelText("New secret") as HTMLInputElement).value).toBe("");
  expect(document.body.textContent).not.toContain("Rotated “");
});

test("a pending rotation cannot be cancelled or closed, so no cancel hides a request that may commit", async () => {
  let finish!: (value: unknown) => void;
  boot({ key_rotate: () => new Promise((done) => { finish = done; }) }, () => [key()]);
  await openDetail("Rotation proof");
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret" }));
  fireEvent.change(screen.getByLabelText("New secret"), { target: { value: NEW_SECRET } });
  fireEvent.click(screen.getAllByRole("button", { name: "Rotate secret" }).at(-1)!);
  await screen.findByRole("button", { name: "Rotating…" });
  expect((screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByRole("button", { name: "Rotating…" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.getByLabelText("New secret")).toBeTruthy();
  expect(native.invoke.mock.calls.filter(([command]) => command === "key_rotate")).toHaveLength(1);
  await act(async () => { finish({ key: key({ historyCount: 1 }), changedAt: "2026-11-07T08:15:33Z", previousChangedAt: null }); });
  await screen.findByText(/Rotated “Rotation proof”/);
});

test("removing a key says it moved to the Recycle Bin, and the bin restores it", async () => {
  let live = [key()];
  let bin: { key: KeyView; deletedAt: string | null }[] = [];
  boot({
    key_remove: () => { bin = live.map((item) => ({ key: item, deletedAt: "2026-11-07T09:00:00Z" })); live = []; },
    recycled_list: () => bin,
    key_restore: () => { const restored = bin[0].key; live = [restored]; bin = []; return restored; },
  }, () => live);
  await openDetail("Rotation proof");
  fireEvent.click(screen.getByRole("button", { name: "Move to Recycle Bin" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move to Recycle Bin" }));
  await screen.findByText(/Moved “Rotation proof” to the Recycle Bin/);
  fireEvent.click(screen.getByRole("button", { name: "Recycle Bin" }));
  fireEvent.click(await screen.findByRole("button", { name: "Restore Rotation proof" }));
  await screen.findByText(/Restored “Rotation proof” to the key list/);
  expect(screen.queryByRole("button", { name: "Restore Rotation proof" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Keys" }));
  await screen.findByRole("button", { name: "Open Rotation proof" });
});

test("deleting for good needs a confirmation, and cancelling sends nothing", async () => {
  let bin = [{ key: key(), deletedAt: null as string | null }];
  boot({
    recycled_list: () => bin,
    key_purge: () => { bin = []; },
  }, () => []);
  await act(async () => { render(<App />); });
  fireEvent.click(await screen.findByRole("button", { name: "Recycle Bin" }));
  fireEvent.click(await screen.findByRole("button", { name: "Delete Rotation proof for good" }));
  const dialog = screen.getByRole("dialog");
  expect(dialog.textContent).toContain("cannot be undone");
  expect(dialog.textContent).toContain("KeePassXC merge");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(native.invoke.mock.calls.some(([command]) => command === "key_purge")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Delete Rotation proof for good" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete for good" }));
  await screen.findByText(/Deleted “Rotation proof” for good/);
  expect(native.invoke.mock.calls.filter(([command]) => command === "key_purge")).toEqual([["key_purge", { id: "fake-id" }]]);
  await waitFor(() => expect(screen.getByText("The Recycle Bin is empty.")).toBeTruthy());
});

test("the empty Recycle Bin says nothing is deleted without the user's choice", async () => {
  boot({ recycled_list: () => [] }, () => []);
  await act(async () => { render(<App />); });
  fireEvent.click(await screen.findByRole("button", { name: "Recycle Bin" }));
  await screen.findByText("The Recycle Bin is empty.");
  expect(document.body.textContent).toContain("AKM never empties this list by itself");
});

// API level: a committed lifecycle change overtaken by a lock is a secret-free receipt, never a retryable failure.
for (const [name, call] of [
  ["key_rotate", () => api.rotate("fake-id", NEW_SECRET)],
  ["key_restore", () => api.restore("fake-id")],
  ["key_purge", () => api.purge("fake-id")],
] as const) {
  test(`${name} overtaken by lock reports completion without a secret`, async () => {
    adoptReservation(crypto.randomUUID());
    let resolve!: (value: unknown) => void;
    native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const outcome = call().catch((error: unknown) => error);
    invalidateSession();
    resolve({ key: key(), changedAt: "2026-11-07T08:15:33Z", previousChangedAt: null });
    const error = await outcome;
    expect(error).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
    expect(JSON.stringify(error)).not.toContain(NEW_SECRET);
    expect(JSON.stringify(retainedCompletions())).not.toContain(NEW_SECRET);
    expect(retainedCompletions()).toHaveLength(1);
  });
}

test("a list of recycled keys overtaken by lock is refused as locked", async () => {
  adoptReservation(crypto.randomUUID());
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const outcome = api.recycled().catch((error: unknown) => error);
  invalidateSession();
  resolve([{ key: key(), deletedAt: null }]);
  expect(await outcome).toMatchObject({ kind: "locked" });
});

beforeEach(() => { adoptReservation(crypto.randomUUID()); });
