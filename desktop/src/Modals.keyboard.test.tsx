import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { adoptReservation } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import type { KeyView } from "./types";

// M12: every modal contains focus, closes with Escape, returns focus to where the user was,
// and takes Enter, using the keyboard only. The inventory below is from
//   grep -rn "role=\"dialog\"" desktop/src --include=*.tsx
// KeyDetail, KeyForm (add and edit), RotateDialog, ConfirmDialog (move to the Recycle Bin,
// delete for good, format upgrade) and the Use keys import dialog.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Keyboard proof", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [], expiresAt: null,
    rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

beforeEach(() => { adoptReservation(crypto.randomUUID()); });
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); native.invoke.mockReset(); });

type Handlers = Record<string, (args?: Record<string, unknown>) => unknown>;
type User = ReturnType<typeof userEvent.setup>;

function boot(handlers: Handlers, keys: () => KeyView[], needsUpgrade = () => false) {
  const sessionId = crypto.randomUUID();
  native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "vault_status") return { sessionId, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: keys().length, vaultName: "Fake", needsUpgrade: needsUpgrade() };
    if (command === "keys_list") return keys();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "backup_info") return { path: "C:/fake/vault.kdbx.bak", exists: false, bytes: 0, modified: null };
    if (handlers[command]) return handlers[command](args);
    throw new Error(command);
  });
  return sessionId;
}

const focused = () => document.activeElement;
const dialogs = () => document.querySelectorAll<HTMLElement>("[role=\"dialog\"]");
const dialog = () => {
  expect(dialogs()).toHaveLength(1);
  return dialogs()[0];
};
const heading = () => dialog().querySelector("h2")?.textContent;
const buttonNamed = (box: HTMLElement, name: string) =>
  [...box.querySelectorAll<HTMLButtonElement>("button")].find((button) => (button.getAttribute("aria-label") ?? button.textContent)?.trim() === name);

async function start(user: User) {
  await act(async () => { render(<App />); });
  const row = await screen.findByRole("button", { name: "Open Keyboard proof" });
  row.focus();
  await user.keyboard("{Enter}");
  return row;
}

const FOCUSABLE = "button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href]";

/** Tab and Shift+Tab two more times than the dialog has controls: focus must never leave it. */
async function expectContained(user: User) {
  const box = dialog();
  const steps = box.querySelectorAll(FOCUSABLE).length + 2;
  expect(box.contains(focused())).toBe(true);
  for (let step = 0; step < steps; step += 1) {
    await user.tab();
    expect(box.contains(focused())).toBe(true);
  }
  for (let step = 0; step < steps; step += 1) {
    await user.tab({ shift: true });
    expect(box.contains(focused())).toBe(true);
  }
}

async function tabTo(user: User, name: string) {
  const box = dialog();
  const steps = box.querySelectorAll(FOCUSABLE).length + 2;
  for (let step = 0; step < steps; step += 1) {
    if (focused() === buttonNamed(box, name)) return;
    await user.tab();
  }
  throw new Error(`could not reach ${name} by keyboard`);
}

test("Key Detail contains focus, closes with Escape and returns focus to its row", async () => {
  boot({}, () => [key()]);
  const user = userEvent.setup({ delay: null });
  const row = await start(user);
  expect(dialog().getAttribute("aria-modal")).toBe("true");
  expect(heading()).toBe("Keyboard proof");
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(row);
}, 30_000);

test("the add form contains focus, closes with Escape and returns focus to Add key", async () => {
  boot({}, () => [key()]);
  const user = userEvent.setup({ delay: null });
  await act(async () => { render(<App />); });
  await screen.findByRole("button", { name: "Open Keyboard proof" });
  const add = screen.getByRole("button", { name: "Add key" });
  add.focus();
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Add a key");
  expect(focused()).toBe(screen.getByLabelText("Title"));
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(add);
}, 30_000);

test("the edit form, opened from Key Detail, returns focus to the key's row on Escape", async () => {
  boot({}, () => [key()]);
  const user = userEvent.setup({ delay: null });
  const row = await start(user);
  await tabTo(user, "Edit");
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Edit key");
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(row);
}, 30_000);

test("Rotate secret contains focus, Escape returns to Key Detail, and Escape again returns to the row", async () => {
  boot({}, () => [key()]);
  const user = userEvent.setup({ delay: null });
  const row = await start(user);
  await tabTo(user, "Rotate secret");
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Rotate secret");
  expect(focused()).toBe(screen.getByLabelText("New secret"));
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(heading()).toBe("Keyboard proof");
  expect(dialog().contains(focused())).toBe(true);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(row);
}, 30_000);

test("Enter in the Rotate form submits it once, and the dialog hands back to Key Detail", async () => {
  let current = key();
  boot({
    key_rotate: () => {
      current = key({ secretChange: "known", secretChangedAt: "2026-11-07T08:15:33Z", historyCount: 1 });
      return { key: current, changedAt: "2026-11-07T08:15:33Z", previousChangedAt: null };
    },
  }, () => [current]);
  const user = userEvent.setup({ delay: null });
  const row = await start(user);
  await tabTo(user, "Rotate secret");
  await user.keyboard("{Enter}");
  await user.keyboard("fake-new-secret-value{Enter}");
  await screen.findByText(/Rotated “Keyboard proof”/);
  expect(native.invoke.mock.calls.filter(([command]) => command === "key_rotate")).toHaveLength(1);
  expect(heading()).toBe("Keyboard proof");
  await user.keyboard("{Escape}");
  expect(focused()).toBe(row);
}, 30_000);

test("the Recycle Bin confirmation from Key Detail contains focus; Escape and Enter work; success lands on the list", async () => {
  let live = [key(), key({ id: "other-id", title: "Other key" })];
  boot({ key_remove: () => { live = live.filter((item) => item.id !== "fake-id"); } }, () => live);
  const user = userEvent.setup({ delay: null });
  const row = await start(user);
  await tabTo(user, "Move to Recycle Bin");
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Move this key to the Recycle Bin?");
  expect(focused()).toBe(buttonNamed(dialog(), "Cancel"));
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(heading()).toBe("Keyboard proof");
  await tabTo(user, "Move to Recycle Bin");
  await user.keyboard("{Enter}");
  await tabTo(user, "Move to Recycle Bin");
  await user.keyboard("{Enter}");
  await screen.findByText(/Moved “Keyboard proof” to the Recycle Bin/);
  expect(dialogs()).toHaveLength(0);
  expect(row.isConnected).toBe(false);
  expect(focused()).toBe(screen.getByRole("searchbox", { name: "Search keys" }));
}, 30_000);

// Cancel is the default of every destructive confirmation, so an Enter typed by habit deletes nothing.
for (const [name, open, request] of [
  ["Delete for good", async (user: User) => {
    await act(async () => { render(<App />); });
    await user.click(await screen.findByRole("button", { name: "Recycle Bin" }));
    (await screen.findByRole("button", { name: "Delete Keyboard proof for good" })).focus();
    await user.keyboard("{Enter}");
  }, "key_purge"],
  ["Move to Recycle Bin", async (user: User) => {
    await start(user);
    await tabTo(user, "Move to Recycle Bin");
    await user.keyboard("{Enter}");
  }, "key_remove"],
] as const) {
  test(`${name}: the first Enter in its confirmation cancels, and sends nothing`, async () => {
    boot({ recycled_list: () => [{ key: key(), deletedAt: null }], key_purge: () => {}, key_remove: () => {} }, () => [key()]);
    const user = userEvent.setup({ delay: null });
    await open(user);
    expect(dialog()).toBeTruthy();
    expect(focused()).toBe(buttonNamed(dialog(), "Cancel"));
    await user.keyboard("{Enter}");
    expect(native.invoke.mock.calls.map(([command]) => command)).not.toContain(request);
    expect(screen.queryByRole("heading", { name: /for good\?|to the Recycle Bin\?/ })).toBeNull();
  }, 30_000);
}

test("Delete for good contains focus, Escape returns to its button, and success lands on the Recycle Bin heading", async () => {
  let bin = [{ key: key(), deletedAt: null as string | null }];
  boot({ recycled_list: () => bin, key_purge: () => { bin = []; } }, () => []);
  const user = userEvent.setup({ delay: null });
  await act(async () => { render(<App />); });
  await user.click(await screen.findByRole("button", { name: "Recycle Bin" }));
  const purge = await screen.findByRole("button", { name: "Delete Keyboard proof for good" });
  purge.focus();
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Delete this key for good?");
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(purge);
  await user.keyboard("{Enter}");
  await tabTo(user, "Delete for good");
  await user.keyboard("{Enter}");
  await screen.findByText(/Deleted “Keyboard proof” for good/);
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(screen.getByRole("heading", { level: 1 }));
}, 30_000);

test("the import dialog contains focus, Escape returns to Import, and Enter submits one request", async () => {
  boot({ env_import: () => ({ added: 1, updated: 0 }) }, () => []);
  const user = userEvent.setup({ delay: null });
  await act(async () => { render(<App />); });
  await user.click(await screen.findByRole("button", { name: "Use keys" }));
  const open = await screen.findByRole("button", { name: "Import .env…" });
  open.focus();
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Import keys");
  expect(focused()).toBe(screen.getByLabelText("Paste the .env contents"));
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(dialogs()).toHaveLength(0);
  expect(focused()).toBe(open);
  await user.keyboard("{Enter}");
  await user.keyboard("FAKE_TOKEN=fake-value");
  await user.tab();
  await user.keyboard("demo");
  await user.tab();
  await user.keyboard("dev{Enter}");
  await screen.findByText("Imported: 1 added, 0 updated.");
  expect(native.invoke.mock.calls.filter(([command]) => command === "env_import")).toHaveLength(1);
  expect(focused()).toBe(open);
}, 30_000);

test("the format-upgrade confirmation contains focus; Escape returns to its button; success lands on the list", async () => {
  let upgraded = false;
  let owner = "";
  owner = boot({
    vault_upgrade: () => {
      upgraded = true;
      return { sessionId: owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Fake", needsUpgrade: false };
    },
  }, () => [key()], () => !upgraded);
  const user = userEvent.setup({ delay: null });
  await act(async () => { render(<App />); });
  const review = await screen.findByRole("button", { name: "Review format upgrade" });
  review.focus();
  await user.keyboard("{Enter}");
  expect(heading()).toBe("Upgrade this vault to KDBX 4.1?");
  await expectContained(user);
  await user.keyboard("{Escape}");
  expect(focused()).toBe(review);
  await user.keyboard("{Enter}");
  await tabTo(user, "Upgrade format");
  await user.keyboard("{Enter}");
  await waitFor(() => expect(upgraded).toBe(true));
  await waitFor(() => expect(dialogs()).toHaveLength(0));
  expect(screen.queryByRole("button", { name: "Review format upgrade" })).toBeNull();
  expect(focused()).toBe(await screen.findByRole("searchbox", { name: "Search keys" }));
}, 30_000);

test("a busy dialog ignores Escape, so a pending request cannot be hidden", async () => {
  let finish!: (value: unknown) => void;
  boot({ key_rotate: () => new Promise((done) => { finish = done; }) }, () => [key()]);
  const user = userEvent.setup({ delay: null });
  await start(user);
  await tabTo(user, "Rotate secret");
  await user.keyboard("{Enter}");
  await user.keyboard("fake-new-secret-value{Enter}");
  await screen.findByRole("button", { name: "Rotating…" });
  await user.keyboard("{Escape}");
  expect(heading()).toBe("Rotate secret");
  await act(async () => { finish({ key: key({ historyCount: 1 }), changedAt: "2026-11-07T08:15:33Z", previousChangedAt: null }); });
  await screen.findByText(/Rotated “Keyboard proof”/);
}, 30_000);
