import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { reads } from "./native";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import { beginNativeOpen, endNativeOpen } from "./nativeSession";

const native = vi.hoisted(() => ({ invoke: vi.fn(), onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Active B</p> }));
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

let current = { sessionId: "", path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
function defaultReply(command: string) {
  if (command === "vault_status") return current;
  if (command === "vault_prepare_open") return crypto.randomUUID();
  if (command === "vault_warnings") return { application: [], operations: [] };
  if (command === "vault_activity") return;
  throw new Error(command);
}
function defaults() {
  const token = beginNativeOpen();
  endNativeOpen(token);
  current = { ...current, sessionId: crypto.randomUUID() };
  native.invoke.mockReset();
  native.invoke.mockImplementation(async (command: string) => defaultReply(command));
}

for (const between of [true, false]) {
  for (const kind of ["status", "prepare-success", "unlock-success", "lock-success", "read-success", "prepare-error", "unlock-error", "status-error", "read-error", "lock-error"] as const) {
    test(`obsolete ${kind} delivered ${between ? "between" : "after"} roots returns only sanitized cancellation`, async () => {
      defaults();
      const root = render(<App />);
      await screen.findByText("Active B");
      let deliver!: (value: unknown) => void;
      const deferredCommand = kind.startsWith("prepare") ? "vault_prepare_open" : kind.startsWith("unlock") ? "vault_unlock" : kind.startsWith("status") ? "vault_status" : kind.startsWith("lock") ? "vault_lock" : "keys_list";
      let held = false;
      native.invoke.mockImplementation(async (command: string) => {
        if (command === deferredCommand && !held) {
          held = true;
          return new Promise((resolve, reject) => { deliver = kind.endsWith("error") ? reject : resolve; });
        }
        return defaultReply(command);
      });
      const call = kind.startsWith("status") ? reads.status() : kind.startsWith("unlock") || kind.startsWith("prepare") ? api.unlock("C:/fake/private-a.kdbx", "fake-password") : kind.startsWith("lock") ? api.lock() : reads.keys();
      const old = call.catch((error: unknown) => error);
      await act(async () => {});
      expect(held).toBe(true); // Unlock cases defer credentials, not their preceding reservation.
      root.unmount();
      current = { ...current, sessionId: crypto.randomUUID() }; // Native replacement B before root handoff.
      if (!between) { render(<App />); await screen.findByText("Active B"); }
      await act(async () => deliver(kind.endsWith("error") ? { kind: "notFound", message: "No vault at C:/fake/private-a.kdbx. Private A" } : kind === "prepare-success" ? crypto.randomUUID() : { ...current, path: "C:/fake/private-a.kdbx", vaultName: "Private A" }));
      const result = await old;
      expect(result).toMatchObject({ kind: "locked" });
      expect(JSON.stringify(result)).not.toContain("private-a");
      expect(JSON.stringify(result)).not.toContain("Private A");
      if (between) { render(<App />); await screen.findByText("Active B"); }
      expect(screen.getByText("Active B")).toBeTruthy();
      expect(document.body.textContent).not.toContain("private-a");
      expect(retainedCompletions()).toHaveLength(0);
    });
  }
}

for (const command of ["vault_prepare_open", "vault_unlock"]) {
test(`a current-generation ${command} error remains actionable instead of being replaced by cancellation`, async () => {
  defaults();
  const reason = { kind: "notFound", message: "No vault at the currently selected C:/fake/b.kdbx." };
  native.invoke.mockImplementation(async (name: string) => { if (name === command) throw reason; return defaultReply(name); });
  expect(await api.unlock(current.path, "fake-password").catch((error: unknown) => error)).toBe(reason);
});
}

test("a bootstrap status overtaken by lock uses a fresh retry rather than obsolete path metadata", async () => {
  defaults();
  let resolve!: (value: unknown) => void;
  // Reserve the status request explicitly; warning polling is independent.
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_status") return new Promise((done) => { resolve = done; });
    throw new Error(command);
  });
  await act(async () => { render(<App />); });
  act(() => native.onLock?.({ payload: { sessionId: current.sessionId } }));
  await act(async () => resolve({ ...current, path: "C:/fake/private-a.kdbx", vaultName: "Private A" }));
  expect(document.body.textContent).not.toContain("private-a");
  const retry = screen.getByRole("button", { name: "Try again" });
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return { ...current, sessionId: null, unlocked: false, vaultName: null };
    if (command === "vault_warnings") return { application: [], operations: [] };
    throw new Error(command);
  });
  fireEvent.click(retry);
  await screen.findByRole("button", { name: "Unlock vault" });
  expect((screen.getByLabelText("Vault file") as HTMLInputElement).value).toBe(current.path);
});
