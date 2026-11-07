import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { acceptNativeStatus, nativeOwner, releaseNativeOwner } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions, settleOutcome } from "./sessionBoundary";
import type { Status } from "./types";

// The native dialog is the review (P2R2-02). A hidden document discards the page's form and tells
// native which session it was in (P2R2-03), but the user's click in the native dialog stands. The
// page must still hear the truth of a request whose form it threw away. The native side is a mock
// here: these tests prove what the page sends and shows, and the Rust tests prove what native does.

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

type Args = { sessionId?: string | null } | undefined;

function fixture(options: { unlocked?: boolean; discard?: (args: Args) => unknown } = {}) {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const owner = crypto.randomUUID();
  let current: string | null = options.unlocked === false ? null : owner;
  const pending = deferred<unknown>();
  native.invoke.mockReset().mockImplementation(async (command: string, args?: Args) => {
    if (command === "env_import") return pending.promise;
    if (command === "view_discard") return options.discard?.(args);
    if (command === "vault_status") return { sessionId: current, path: "C:/fake/current.kdbx", exists: true, unlocked: current !== null, keyCount: 1, vaultName: current ? "Fake" : null, needsUpgrade: false } satisfies Status;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") {
      if (args?.sessionId !== current) throw { kind: "locked", message: "The vault is locked." };
      current = null;
      return;
    }
    if (command === "keys_list") return [{ project: "demo", environment: "dev" }];
    if (command === "backup_info") return { path: "C:/fake/current.bak", exists: false, bytes: 0, modified: null };
    if (command === "reminders_status") return { enabled: false, permission: "granted", delivery: "none", blocked: false };
    throw new Error(command);
  });
  return { owner, pending, active: () => current !== null };
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

const outcomes = () => screen.getByRole("region", { name: "Operation outcomes" });

test("hiding the document sends the session that the page read, and nothing else", async () => {
  const f = fixture();
  await settingsWithPendingImport();
  expect(calls("view_discard")).toHaveLength(0);
  hide();
  await flush();
  expect(calls("view_discard")).toEqual([["view_discard", { sessionId: f.owner }]]);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("an approval after a hide is reported as a completed import, never as not done", async () => {
  const f = fixture();
  await settingsWithPendingImport();
  hide();
  await flush();
  expect(screen.queryByRole("dialog")).toBeNull();
  show();
  await act(async () => f.pending.resolve({ added: 1, updated: 0 }));
  expect(outcomes().textContent).toContain("Import completed");
  expect(outcomes().textContent).not.toContain("declined");
  expect(document.body.textContent).not.toContain("fake-private-input");
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "completed" }]);
  expect(f.active()).toBe(true);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("a decline after a hide is reported as a decline", async () => {
  const f = fixture();
  await settingsWithPendingImport();
  hide();
  await flush();
  show();
  await act(async () => f.pending.resolve(null));
  expect(outcomes().textContent).toContain("declined in the confirmation window");
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "cancelled" }]);
});

test("a refusal after a hide stays an error that does not claim nothing was saved", async () => {
  const f = fixture();
  await settingsWithPendingImport();
  hide();
  await flush();
  show();
  await act(async () => f.pending.reject({ kind: "stalePreview", message: "The session or vault changed. Start this operation again." }));
  expect(outcomes().textContent).not.toContain("Imported");
  expect(outcomes().textContent).toContain("does not confirm that nothing was saved");
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "reportedError" }]);
  expect(f.active()).toBe(true);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("a discard that is delivered after the approval changes neither the import nor the lock state", async () => {
  const late = deferred<unknown>();
  const f = fixture({ discard: () => late.promise });
  await settingsWithPendingImport();
  hide();
  await flush();
  show();
  await act(async () => f.pending.resolve({ added: 1, updated: 0 }));
  await act(async () => late.resolve(undefined));
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "completed" }]);
  expect(f.active()).toBe(true);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("a discard that never returns does not hide the outcome of the import", async () => {
  const f = fixture({ discard: () => new Promise(() => {}) });
  await settingsWithPendingImport();
  hide();
  await flush();
  show();
  await act(async () => f.pending.resolve({ added: 1, updated: 0 }));
  expect(outcomes().textContent).toContain("Import completed");
  expect(f.active()).toBe(true);
});

test("hiding the document reaches native from every view, not only from Settings", async () => {
  const f = fixture();
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
  expect(calls("view_discard")).toEqual([["view_discard", { sessionId: f.owner }], ["view_discard", { sessionId: f.owner }]]);
});

test("a hidden document with no unlocked vault sends nothing", async () => {
  fixture({ unlocked: false });
  render(<App />);
  await flush();
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(0);
});

test("a hidden document after the vault was locked sends nothing", async () => {
  const f = fixture();
  render(<App />);
  await screen.findByText("Current unlocked keys");
  fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
  await flush();
  expect(f.active()).toBe(false);
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(0);
});

test("a hide while the page has no native session sends nothing, not an empty or a guessed one", async () => {
  const f = fixture();
  render(<App />);
  await screen.findByText("Current unlocked keys");
  releaseNativeOwner(f.owner); // The page lost its session before React showed it: status still says unlocked.
  expect(nativeOwner()).toBeNull();
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(0);
  expect(calls("vault_lock")).toHaveLength(0);
});

test("a discard that native cannot take fails closed and locks that session", async () => {
  const f = fixture({ discard: () => { throw { kind: "internal", message: "Could not deliver native presentation expiry." }; } });
  render(<App />);
  await screen.findByText("Current unlocked keys");
  hide();
  await flush();
  expect(calls("view_discard")).toHaveLength(1);
  expect(calls("vault_lock")).toEqual([["vault_lock", { sessionId: f.owner }]]);
  expect(f.active()).toBe(false);
});

test("a discard that fails after another vault was opened locks the old session and not the new one", async () => {
  const late = deferred<unknown>();
  const f = fixture({ discard: () => late.promise });
  render(<App />);
  await screen.findByText("Current unlocked keys");
  hide();
  await flush();
  expect(calls("view_discard")).toEqual([["view_discard", { sessionId: f.owner }]]);
  const replacement = crypto.randomUUID();
  act(() => acceptNativeStatus({ sessionId: replacement, path: "C:/fake/other.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Other", needsUpgrade: false }));
  expect(nativeOwner()).toBe(replacement);
  await act(async () => late.reject({ kind: "internal", message: "Could not deliver native presentation expiry." }));
  expect(calls("vault_lock").every(([, args]) => (args as Args)?.sessionId === f.owner)).toBe(true);
  expect(calls("vault_lock").some(([, args]) => (args as Args)?.sessionId === replacement)).toBe(false);
  expect(calls("vault_status")).toHaveLength(1);
  expect(nativeOwner()).toBe(replacement);
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
