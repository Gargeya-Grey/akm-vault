import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyDetail } from "./components/KeyDetail";
import { Settings } from "./components/Settings";
import type { KeyView, Status } from "./types";

// Unit 1a, 1A-R8-02: clipboard cleanup is best effort, so no text may promise it.
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
afterEach(() => cleanup());

const COPY_TEXT =
  "Copied. AKM tries to clear its copy after 30 seconds, on lock and on normal exit. The copy may remain if the clipboard is busy or AKM stops unexpectedly.";
const LOCK_TEXT =
  "Locking closes the current session and tries to clear AKM’s clipboard copy. The copy may remain if the clipboard is busy.";

const key: KeyView = {
  id: "fake-id",
  title: "Fake key",
  username: null,
  issuer: null,
  project: "demo",
  environment: "dev",
  scopes: [],
  envVar: null,
  resolvedEnvVar: "FAKE_KEY",
  url: null,
  notes: null,
  tags: [],
  expiresAt: null,
  rotationDays: null,
  createdAt: null,
  updatedAt: null,
  expired: false,
  rotationDueIn: null,
};
const status: Status = { sessionId: "fake-session", path: "C:/fake/a.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Fake", needsUpgrade: false };

function respond() {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "key_copy" || command === "env_copy") return 30;
    if (command === "keys_list") return [key];
    if (command === "backup_info") return { path: "C:/fake/a.kdbx.bak", exists: false, bytes: 0, modified: null };
    if (command === "vault_activity") return;
    throw new Error(`Unexpected command ${command}`);
  });
}

function promisesNothing(text: string) {
  expect(text).not.toMatch(/\bclears\b/);
  expect(text).not.toMatch(/\bwill\b[^.]*\b(try|retry|again)\b/);
}

test("the key copy text says that cleanup is an attempt and that the copy may remain", async () => {
  respond();
  render(<KeyDetail item={key} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  expect(await screen.findByText(COPY_TEXT)).toBeTruthy();
  promisesNothing(document.body.textContent ?? "");
});

test("the environment copy text and the lock text say that cleanup is an attempt", async () => {
  respond();
  render(<Settings status={status} onLocked={() => {}} />);
  expect(document.body.textContent).toContain(LOCK_TEXT);
  await screen.findByRole("option", { name: "demo" });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  await screen.findByRole("option", { name: "dev" });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  fireEvent.click(screen.getByRole("button", { name: "Copy to clipboard" }));
  expect(await screen.findByText(COPY_TEXT)).toBeTruthy();
  promisesNothing(document.body.textContent ?? "");
});
