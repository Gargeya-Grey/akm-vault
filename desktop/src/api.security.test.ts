import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { adoptReservation, nativeOwner, releaseNativeOwner } from "./nativeSession";
import { acknowledgeNotices, retainedOperationNotices } from "./operationNotices";
import { acknowledgeCompletions, COMPLETION_EVENT, invalidateSession, retainedCompletions, sessionGeneration, SESSION_LOSS_EVENT } from "./sessionBoundary";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
const IMPORT = ["TOKEN=fake-private-value", "private-scope", "private-env"] as const;
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
beforeEach(() => { adoptReservation(crypto.randomUUID()); });
afterEach(() => { vi.resetAllMocks(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

test("a mutation reply overtaken by lock emits a secret-free receipt, not a retryable failure", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const notice = vi.fn();
  const lost = vi.fn();
  window.addEventListener(COMPLETION_EVENT, notice);
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.importEnv(...IMPORT).catch((error: unknown) => error);
    invalidateSession();
    resolve({ added: 1, updated: 0 });
    expect(await outcome).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
    expect(notice).toHaveBeenCalledTimes(2); // Pending and settlement update one retained record.
    expect(lost).toHaveBeenCalledTimes(1);
    expect(retainedCompletions()).toHaveLength(1);
    expect(JSON.stringify(notice.mock.calls)).not.toContain("fake-private-value");
  } finally { window.removeEventListener(COMPLETION_EVENT, notice); window.removeEventListener(SESSION_LOSS_EVENT, lost); }
});

test("an obsolete secret reply is suppressed without claiming a completed mutation", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const outcome = api.reveal("fake-id").catch((error: unknown) => error);
  invalidateSession();
  resolve("fake-private-plaintext");
  expect(await outcome).toMatchObject({ kind: "locked" });
  expect(JSON.stringify(await outcome)).not.toContain("fake-private-plaintext");
});

test("a cancelled save overtaken by a session change remains cancellation, not completion or uncertainty", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const lost = vi.fn();
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.saveEnv("fake-scope", "fake-env").catch((error: unknown) => error);
    const pending = retainedCompletions()[0];
    invalidateSession();
    resolve(null);
    expect(await outcome).toMatchObject({ kind: "locked" });
    expect(retainedCompletions()).toEqual([{ ...pending, outcome: "cancelled" }]);
    expect(lost).not.toHaveBeenCalled();
  } finally { window.removeEventListener(SESSION_LOSS_EVENT, lost); }
});

test("native known-commit errors are normalized and retain only four fixed classification fields", async () => {
  native.invoke.mockRejectedValueOnce({ kind: "completedWhileLocked", message: "FAKE_PRIVATE_NATIVE_ERROR C:/private-a/export.env", value: "fake-secret" });
  const error = await api.importEnv(...IMPORT).catch((cause: unknown) => cause);
  expect(error).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
  const records = retainedCompletions();
  expect(records).toHaveLength(1);
  expect(Object.keys(records[0]).sort()).toEqual(["generation", "id", "operation", "outcome"]);
  expect(records[0]).toMatchObject({ operation: "import", outcome: "completedWhileLocked" });
  for (const privateText of ["fake-private-value", "private-scope", "FAKE_PRIVATE_NATIVE_ERROR", "export.env", "fake-secret"]) {
    expect(JSON.stringify(error)).not.toContain(privateText);
    expect(JSON.stringify(records)).not.toContain(privateText);
  }
});

function dispatchedOperation(): string {
  const call = native.invoke.mock.calls.find(([command]) => command === "env_import");
  return (call?.[1] as { operationId: string }).operationId;
}
function recordFor(operation: string, kind: string) {
  return { application: [], operations: [{ id: operation, originSession: crypto.randomUUID(), kind }] };
}

for (const [kind, outcome] of [["importApplied", "completed"], ["importSessionEnded", "completedWhileLocked"], ["importFailed", "reportedError"], ["importDeclined", "cancelled"]] as const) {
  test(`a lost import reply is settled from the native ${kind} record and the record is acknowledged`, async () => {
    native.invoke.mockReturnValueOnce(new Promise(() => {}));
    void api.importEnv(...IMPORT).catch(() => {});
    expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "pending" }]);
    const operation = dispatchedOperation();
    native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, kind) : undefined);
    await api.warnings();
    expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome }]);
    expect(retainedOperationNotices()).toEqual([]);
    expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [operation] });
    await api.warnings();
    expect(retainedCompletions()).toHaveLength(1);
  });
}

test("a native import record for a request this page never made is shown as a notice, not folded", async () => {
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(crypto.randomUUID(), "importApplied") : undefined);
  await api.warnings();
  expect(retainedCompletions()).toEqual([]);
  expect(retainedOperationNotices()).toMatchObject([{ kind: "importApplied" }]);
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

test("a native import record cannot overwrite an outcome the reply already settled", async () => {
  native.invoke.mockResolvedValueOnce({ added: 1, updated: 0 });
  await api.importEnv(...IMPORT);
  const operation = dispatchedOperation();
  expect(retainedCompletions()).toMatchObject([{ outcome: "completed" }]);
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "importFailed") : undefined);
  await api.warnings();
  expect(retainedCompletions()).toMatchObject([{ outcome: "completed" }]);
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

test("a reply from the native command acknowledges its own native record", async () => {
  native.invoke.mockResolvedValueOnce({ added: 1, updated: 0 });
  await api.importEnv(...IMPORT);
  expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [dispatchedOperation()] });
});

test("a transport failure keeps the native record because it may be the only truth", async () => {
  native.invoke.mockRejectedValueOnce(new Error("channel closed"));
  await api.importEnv(...IMPORT).catch(() => {});
  expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
});

// P2R2-03. A discard names the session that the page read when it handled the hide. A discard
// that fails locks that session and no other.
const lockCalls = () => native.invoke.mock.calls.filter(([command]) => command === "vault_lock");

test("a discard carries the session that was named, not the one that is current when it is sent", async () => {
  const named = crypto.randomUUID();
  native.invoke.mockResolvedValue(undefined);
  await api.discardView(named);
  expect(native.invoke).toHaveBeenCalledTimes(1);
  expect(native.invoke).toHaveBeenCalledWith("view_discard", { sessionId: named });
});

test("a failed discard locks the session it named and never asks which session is current", async () => {
  const named = crypto.randomUUID();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "view_discard") throw { kind: "internal", message: "Could not deliver native presentation expiry." };
    if (command === "vault_lock") return;
    throw new Error(command);
  });
  await api.discardView(named);
  expect(native.invoke.mock.calls.map(([command]) => command)).toEqual(["view_discard", "vault_lock"]);
  expect(lockCalls()).toEqual([["vault_lock", { sessionId: named }]]);
});

test("a failed discard from a retired session leaves the session that replaced it unlocked", async () => {
  const retired = crypto.randomUUID();
  const replacement = nativeOwner();
  expect(replacement).not.toBeNull();
  native.invoke.mockImplementation(async (command: string, args?: { sessionId?: string }) => {
    if (command === "view_discard") throw { kind: "internal", message: "Could not deliver native presentation expiry." };
    if (command === "vault_lock" && args?.sessionId !== replacement) throw { kind: "locked", message: "The vault is locked." };
    throw new Error(command);
  });
  await api.discardView(retired);
  expect(lockCalls()).toEqual([["vault_lock", { sessionId: retired }]]);
  expect(nativeOwner()).toBe(replacement);
});

test("a discard that finds its session locked does nothing more", async () => {
  native.invoke.mockRejectedValue({ kind: "locked", message: "The vault is locked." });
  await api.discardView(crypto.randomUUID());
  expect(lockCalls()).toEqual([]);
});

// P2R2-03, class B. Every request that acts on the open vault names the session in which the
// page made it. Native refuses a request for a session that is not the current one.
const SCOPED_REQUESTS: [string, () => Promise<unknown>][] = [
  ["vault_activity", () => api.activity()],
  ["vault_upgrade", () => api.upgrade()],
  ["vault_reload", () => api.reload()],
  ["keys_list", () => api.keys()],
  ["key_reveal", () => api.reveal("fake-id")],
  ["key_copy", () => api.copy("fake-id")],
  ["key_add", () => api.add({ title: "Fake key", secret: "fake-secret" } as never)],
  ["key_update", () => api.update("fake-id", { title: "Fake key" } as never)],
  ["key_remove", () => api.remove("fake-id")],
  ["key_rotate", () => api.rotate("fake-id", "fake-new-secret")],
  ["recycled_list", () => api.recycled()],
  ["key_restore", () => api.restore("fake-id")],
  ["key_purge", () => api.purge("fake-id")],
  ["audit_read", () => api.audit()],
  ["audit_recover", () => api.recoverJournal()],
  ["env_export", () => api.exportEnv("fake-scope", "fake-env")],
  ["env_copy", () => api.copyEnv("fake-scope", "fake-env")],
  ["env_save", () => api.saveEnv("fake-scope", "fake-env")],
  ["env_launch_terminal", () => api.launchTerminal("fake-scope", "fake-env")],
  ["key_history", () => api.history("fake-id")],
  ["backup_info", () => api.backup()],
];

test.each(SCOPED_REQUESTS)("%s names the session that the page was in when it made the request", async (command, request) => {
  const owner = nativeOwner();
  expect(owner).not.toBeNull();
  native.invoke.mockRejectedValue({ kind: "internal", message: "stop here" });
  await request().catch(() => {});
  const call = native.invoke.mock.calls.find(([name]) => name === command);
  expect(call?.[1]).toMatchObject({ sessionId: owner });
});

test.each(SCOPED_REQUESTS)("%s sends no usable session when the page has none", async (command, request) => {
  releaseNativeOwner(nativeOwner() as string);
  expect(nativeOwner()).toBeNull();
  native.invoke.mockRejectedValue({ kind: "locked", message: "The vault is locked." });
  await request().catch(() => {});
  const call = native.invoke.mock.calls.find(([name]) => name === command);
  expect(call?.[1]).toMatchObject({ sessionId: "" });
});

// The terminal launch starts a program and cannot be taken back, so the page sends a scope and
// nothing else, and a late reply is a completion, never a failure to retry.
test("a terminal launch sends the session and the scope and no program, folder or argument", async () => {
  native.invoke.mockResolvedValueOnce(2);
  expect(await api.launchTerminal("fake-project", "fake-env")).toBe(2);
  expect(native.invoke).toHaveBeenCalledExactlyOnceWith("env_launch_terminal", { project: "fake-project", environment: "fake-env", sessionId: nativeOwner() });
});

test("a terminal launch leaves a record that survives the page dropping its reply", async () => {
  native.invoke.mockResolvedValueOnce(2);
  await api.launchTerminal("fake-project", "fake-env");
  expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "completed" }]);
});

test("a cancelled terminal launch answers null and leaves no completion record", async () => {
  native.invoke.mockResolvedValueOnce(null);
  expect(await api.launchTerminal("fake-project", "fake-env")).toBeNull();
  expect(retainedCompletions()).toEqual([]);
});

test("a terminal launch overtaken by a session change reports a completion, not a retryable failure", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const lost = vi.fn();
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.launchTerminal("fake-project", "fake-env").catch((error: unknown) => error);
    invalidateSession();
    resolve(1);
    expect(await outcome).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
    expect(lost).toHaveBeenCalledTimes(1);
  } finally { window.removeEventListener(SESSION_LOSS_EVENT, lost); }
});

test("a terminal launch overtaken by a session change keeps a terminal notice, owned by the old session", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const origin = sessionGeneration();
  const lost = vi.fn();
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.launchTerminal("fake-project", "fake-env").catch((error: unknown) => error);
    invalidateSession();
    const newer = sessionGeneration();
    resolve(2);
    expect(await outcome).toMatchObject({ kind: "completedWhileLocked" });
    expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "completedWhileLocked", generation: origin }]);
    expect(lost).toHaveBeenCalledTimes(1);
    expect((lost.mock.calls[0][0] as CustomEvent<{ generation: number }>).detail.generation).toBe(origin);
    expect(origin).not.toBe(newer);
    expect(sessionGeneration()).toBe(newer);
  } finally { window.removeEventListener(SESSION_LOSS_EVENT, lost); }
});

test("a cancelled terminal launch overtaken by a session change stays a cancellation", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const lost = vi.fn();
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.launchTerminal("fake-project", "fake-env").catch((error: unknown) => error);
    invalidateSession();
    resolve(null);
    expect(await outcome).toMatchObject({ kind: "locked" });
    expect(lost).not.toHaveBeenCalled();
  } finally { window.removeEventListener(SESSION_LOSS_EVENT, lost); }
});

test("a history reply overtaken by a session change is suppressed", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const outcome = api.history("fake-id").catch((error: unknown) => error);
  invalidateSession();
  resolve([{ index: 0, modifiedAt: null, changed: [], secretChanged: false }]);
  expect(await outcome).toMatchObject({ kind: "locked" });
});

test("a disclosure the user declined in the native window rejects as declined and carries no value", async () => {
  native.invoke.mockResolvedValueOnce(null);
  const revealed = await api.reveal("fake-id").catch((error: unknown) => error);
  expect(revealed).toMatchObject({ kind: "declined", message: expect.stringContaining("No value was shown") });
  native.invoke.mockResolvedValueOnce(null);
  const copied = await api.copy("fake-id").catch((error: unknown) => error);
  expect(copied).toMatchObject({ kind: "declined", message: expect.stringContaining("Nothing was copied") });
  expect(retainedCompletions()).toHaveLength(0); // a decline is no completed mutation
});

test("a declined bulk export or copy answers null, never a value or a number", async () => {
  native.invoke.mockResolvedValueOnce(null);
  expect(await api.exportEnv("fake-scope", "fake-env")).toBeNull();
  native.invoke.mockResolvedValueOnce(null);
  expect(await api.copyEnv("fake-scope", "fake-env")).toBeNull();
  native.invoke.mockResolvedValueOnce("TOKEN=fake-approved\n");
  expect(await api.exportEnv("fake-scope", "fake-env")).toBe("TOKEN=fake-approved\n");
});

test("an approved single reveal and copy still answer the value and the clear time", async () => {
  native.invoke.mockResolvedValueOnce("fake-approved-value");
  expect(await api.reveal("fake-id")).toBe("fake-approved-value");
  native.invoke.mockResolvedValueOnce(30);
  expect(await api.copy("fake-id")).toBe(30);
});

test("a recovery the user declined answers null and leaves no completion record", async () => {
  native.invoke.mockResolvedValueOnce(null);
  expect(await api.recoverJournal()).toBeNull();
  expect(retainedCompletions()).toHaveLength(0);
  native.invoke.mockResolvedValueOnce({ keptName: "j-0.jsonl.damaged-20261109T101500Z", sha256: "ab".repeat(32), bytes: 213 });
  expect(await api.recoverJournal()).toMatchObject({ keptName: "j-0.jsonl.damaged-20261109T101500Z", bytes: 213 });
  expect(native.invoke).toHaveBeenLastCalledWith("audit_recover", { sessionId: nativeOwner() });
});
