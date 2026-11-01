import React from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./src/App";
import { api } from "./src/api";
import { retainedCompletions } from "./src/sessionBoundary";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./src/theme", () => ({ useTheme: () => ({ theme: "light", toggle: () => {} }) }));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const status = { path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Fake", needsUpgrade: false };

test("a confirmed import result is lost after hiding and navigating away", async () => {
  let finish!: (value: unknown) => void;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "keys_list") return [];
    if (command === "backup_info") return { path: "C:/fake/a.bak", exists: false, bytes: 0, modified: null };
    if (command === "env_import_preview") return { token: "fake-token", added: 1, updated: 0, unchanged: 0, changes: [] };
    if (command === "env_import_confirm") return new Promise((resolve) => { finish = resolve; });
    throw new Error(`unexpected ${command}`);
  });
  render(<App />);
  await screen.findByRole("button", { name: "Settings" });
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByRole("button", { name: "Confirm import" });
  fireEvent.click(screen.getByRole("button", { name: "Confirm import" }));
  expect(native.invoke.mock.calls.filter(([command]) => command === "env_import_confirm")).toHaveLength(1);
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Keys" }));
  await act(async () => finish({ added: 1, updated: 0 }));
  expect(retainedCompletions()).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  expect(screen.queryByText(/Imported: 1 added/)).toBeNull();
  expect(screen.queryByText(/An operation completed/)).toBeNull();
  console.log("Observed: dispatched confirmation resolved successfully while Settings was unmounted; remounted Settings and app receipt area gave no completion acknowledgment.");
});

test("a rejected old unlock promise exposes the old path after root teardown", async () => {
  let reject!: (value: unknown) => void;
  let statusCalls = 0;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") {
      statusCalls += 1;
      return statusCalls === 1 ? { ...status, path: "C:/fake/sensitive-a.kdbx", unlocked: false, vaultName: null } : status;
    }
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "keys_list") return [];
    if (command === "vault_unlock") return new Promise((_resolve, failed) => { reject = failed; });
    throw new Error(`unexpected ${command}`);
  });
  const root = render(<App />);
  await screen.findByRole("button", { name: "Unlock vault" });
  const result = api.unlock("C:/fake/sensitive-a.kdbx", "fake-password").catch((error) => error);
  root.unmount();
  render(<App />);
  await screen.findByRole("button", { name: "Settings" });
  const nativeError = { kind: "notFound", message: "No vault at C:/fake/sensitive-a.kdbx." };
  reject(nativeError);
  expect(await result).toEqual(nativeError);
  expect(document.body.textContent).not.toContain("sensitive-a.kdbx");
  console.log("Observed: an obsolete native unlock error was returned to the old API caller with its original path; this test does not show that the new root renders it.");
});

test("a stale status success exposes the old path after root teardown", async () => {
  let deliver!: (value: unknown) => void;
  let calls = 0;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") {
      calls += 1;
      if (calls === 1) return { ...status, path: "C:/fake/private-a.kdbx", vaultName: "Private A" };
      if (calls === 2) return new Promise((resolve) => { deliver = resolve; });
      return status;
    }
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "keys_list") return [];
    throw new Error(`unexpected ${command}`);
  });
  const root = render(<App />);
  await screen.findByRole("button", { name: "Settings" });
  const result = api.status();
  root.unmount();
  render(<App />);
  await screen.findByRole("button", { name: "Settings" });
  const obsolete = { ...status, path: "C:/fake/private-a.kdbx", vaultName: "Private A" };
  deliver(obsolete);
  expect(await result).toEqual(obsolete);
  expect(document.body.textContent).not.toContain("private-a.kdbx");
  console.log("Observed: obsolete status success returned its original vault path and name to the old API caller; this test does not show that the new root renders them.");
});
