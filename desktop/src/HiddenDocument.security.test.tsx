import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { acknowledgeCompletions, retainedCompletions, settleOutcome } from "./sessionBoundary";
import type { Status } from "./types";

// Review 1, finding 1. A hidden document discards the page's review. Native must hear about it
// without waiting for a window event, or an open import dialog stays live for an approval of a
// form that the page already threw away. The native side is a mock here: these tests prove what
// the page sends, and the Rust tests prove what native does with it.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Current unlocked keys</p> }));

afterEach(() => {
  cleanup();
  for (const { id, outcome } of retainedCompletions()) if (outcome === "pending") settleOutcome(id, "unconfirmed");
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  vi.restoreAllMocks();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture(options: { unlocked?: boolean; discard?: () => unknown } = {}) {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const owner = crypto.randomUUID();
  let active = options.unlocked ?? true;
  const pending = deferred<unknown>();
  native.invoke.mockReset().mockImplementation(async (command: string, args?: { sessionId?: string | null }) => {
    if (command === "env_import") return pending.promise;
    if (command === "view_discard") return options.discard?.();
    if (command === "vault_status") return { sessionId: active ? owner : null, path: "C:/fake/current.kdbx", exists: true, unlocked: active, keyCount: 1, vaultName: active ? "Fake" : null, needsUpgrade: false } satisfies Status;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") { expect(args?.sessionId).toBe(owner); active = false; return; }
    if (command === "keys_list") return [{ project: "demo", environment: "dev" }];
    if (command === "backup_info") return { path: "C:/fake/current.bak", exists: false, bytes: 0, modified: null };
    if (command === "reminders_status") return { enabled: false, permission: "granted", delivery: "none", blocked: false };
    throw new Error(command);
  });
  return { pending, active: () => active };
}

const calls = (command: string) => native.invoke.mock.calls.filter(([name]) => name === command);
const flush = () => act(async () => {});

function hide() {
  vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
}
function show() {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
}

async function settingsWithPendingImport() {
  render(<App />);
  await screen.findByText("Current unlocked keys");
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  await screen.findByRole("option", { name: "demo" });
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [name, value] of [["Paste the .env contents", "TOKEN=fake-private-input"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(name), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await flush();
  expect(calls("env_import")).toHaveLength(1);
}

test("hiding the document with an import confirmation pending tells native, and a late approval gets a truthful error", async () => {
  const f = fixture();
  await settingsWithPendingImport();
  expect(calls("view_discard")).toHaveLength(0);
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  expect(screen.queryByRole("dialog")).toBeNull();
  show();
  await act(async () => f.pending.reject({ kind: "stalePreview", message: "The vault changed while the import waited for confirmation. Start it again." }));
  const outcomes = screen.getByRole("region", { name: "Operation outcomes" });
  expect(outcomes.textContent).not.toContain("Imported");
  expect(document.body.textContent).not.toContain("fake-private-input");
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "reportedError" }]);
  expect(f.active()).toBe(true);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("hiding the document reaches native from every view, not only from Settings", async () => {
  fixture();
  render(<App />);
  await screen.findByText("Current unlocked keys");
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  show();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(2);
});

test("a hidden document with no unlocked vault sends nothing", async () => {
  fixture({ unlocked: false });
  render(<App />);
  await flush();
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(0);
});

test("a discard that native cannot take fails closed and locks the vault", async () => {
  const f = fixture({ discard: () => { throw { kind: "internal", message: "Could not deliver native presentation expiry." }; } });
  render(<App />);
  await screen.findByText("Current unlocked keys");
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  expect(calls("vault_lock")).toHaveLength(1);
  expect(f.active()).toBe(false);
});

test("a discard that finds the vault already locked does not lock anything again", async () => {
  fixture({ discard: () => { throw { kind: "locked", message: "The vault is locked." }; } });
  render(<App />);
  await screen.findByText("Current unlocked keys");
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  expect(calls("vault_lock")).toHaveLength(0);
});
