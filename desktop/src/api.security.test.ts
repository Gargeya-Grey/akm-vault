import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { reads } from "./native";
import { plaintextScope } from "./plaintext";
import { openFor } from "./sealed";
import { adoptReservation, nativeOwner, releaseNativeOwner } from "./nativeSession";
import { acknowledgeNotices, retainedOperationNotices } from "./operationNotices";
import { acknowledgeCompletions, COMPLETION_EVENT, dataGeneration, invalidateSession, retainedCompletions, sessionGeneration, SESSION_LOSS_EVENT } from "./sessionBoundary";
import { NATIVE_ERROR_KINDS } from "./types";

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

function dispatchedOperation(command = "env_import"): string {
  const call = native.invoke.mock.calls.find(([name]) => name === command);
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
    await reads.warnings();
    expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome }]);
    expect(retainedOperationNotices()).toEqual([]);
    expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [operation] });
    await reads.warnings();
    expect(retainedCompletions()).toHaveLength(1);
  });
}

// U2-R2-01. An applied import changed the vault data, so a value that is on the screen is old. The
// record says so also when no receipt waits for it. The data generation hides such a value.
test("an applied import record changes the data generation once when the reply was lost", async () => {
  native.invoke.mockRejectedValueOnce(new Error("the reply was lost"));
  await api.importEnv(...IMPORT).catch(() => {});
  expect(retainedCompletions()).toMatchObject([{ operation: "import", outcome: "lost" }]);
  const operation = dispatchedOperation();
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "importApplied") : undefined);
  const before = dataGeneration();
  await reads.warnings();
  expect(dataGeneration()).toBe(before + 1);
  expect(retainedOperationNotices()).toMatchObject([{ kind: "importApplied" }]);
  await reads.warnings();
  expect(dataGeneration()).toBe(before + 1);
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

test("an applied import record of a request this page never made changes the data generation once", async () => {
  const record = recordFor(crypto.randomUUID(), "importApplied");
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? record : undefined);
  const before = dataGeneration();
  await reads.warnings();
  await reads.warnings();
  expect(dataGeneration()).toBe(before + 1);
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

test("an applied import record after the reply said the same changes no data generation again", async () => {
  native.invoke.mockResolvedValueOnce({ added: 1, updated: 0 });
  await api.importEnv(...IMPORT);
  const operation = dispatchedOperation();
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "importApplied") : undefined);
  const before = dataGeneration();
  await reads.warnings();
  expect(dataGeneration()).toBe(before);
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

test("a native import record for a request this page never made is shown as a notice, not folded", async () => {
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(crypto.randomUUID(), "importApplied") : undefined);
  await reads.warnings();
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
  await reads.warnings();
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

// U2-R1-07. The one boundary function decides where an error came from. A native error reply is
// a plain object with a string message and a kind on the closed list in types.ts. Every other
// rejection is a transport failure: the page cannot know the outcome, so it keeps the native
// record, and its receipt says that the outcome was lost.
const TRANSPORT_SHAPES: [string, unknown][] = [
  ["the reviewer's object with an unknown kind", { kind: "transport", message: "fake channel closed" }],
  ["an object with another unknown kind", { kind: "fakeUnknownKind", message: "fake channel closed" }],
  ["a string", "fake channel closed"],
  ["an Error", new Error("fake channel closed")],
  ["an Error that carries a listed kind", Object.assign(new Error("fake channel closed"), { kind: "launch" })],
  ["an object whose kind is not a string", { kind: 7, message: "fake channel closed" }],
  ["a listed kind without a message", { kind: "launch" }],
  ["an array", ["launch", "fake channel closed"]],
  ["null", null],
  ["undefined", undefined],
];
const OPERATIONS = [
  ["env_import", () => api.importEnv(...IMPORT)],
  ["env_launch_terminal", () => api.launchTerminal("fake-project", "fake-env")],
] as const;

for (const [command, request] of OPERATIONS) {
  for (const [shape, rejection] of TRANSPORT_SHAPES) {
    test(`${command}: a rejection that is ${shape} is a transport failure, so the native record stays`, async () => {
      native.invoke.mockImplementation(async (name: string) => { if (name === command) throw rejection; });
      const error = await request().catch((cause: unknown) => cause);
      expect(error).toMatchObject({ kind: "transport", message: expect.any(String) });
      expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
      expect(retainedCompletions()).toMatchObject([{ outcome: "lost" }]);
    });
  }
  for (const kind of NATIVE_ERROR_KINDS) {
    test(`${command}: a native error reply of kind ${kind} is the outcome, so the page acknowledges the record`, async () => {
      native.invoke.mockImplementation(async (name: string) => { if (name === command) throw { kind, message: "fake native failure" }; });
      const error = await request().catch((cause: unknown) => cause);
      expect(error).toMatchObject({ kind });
      expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [dispatchedOperation(command)] });
      expect(retainedCompletions()).toMatchObject([{ outcome: kind === "completedWhileLocked" ? "completedWhileLocked" : "reportedError" }]);
    });
  }
}

test("a transport failure after the session changed keeps the record and is a cancellation", async () => {
  let fail!: (reason: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((_, reject) => { fail = reject; }));
  const outcome = api.launchTerminal("fake-project", "fake-env").catch((error: unknown) => error);
  invalidateSession();
  fail(new Error("fake channel closed"));
  expect(await outcome).toMatchObject({ kind: "locked" });
  expect(retainedCompletions()).toMatchObject([{ outcome: "unconfirmed" }]);
  expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
});

test("an error passes on only its kind and its message", async () => {
  native.invoke.mockRejectedValueOnce({ kind: "io", message: "fake native failure", value: "fake-secret" });
  expect(await reads.keys().catch((error: unknown) => error)).toEqual({ kind: "io", message: "fake native failure" });
  native.invoke.mockRejectedValueOnce(Object.assign(new Error("fake channel closed"), { value: "fake-secret" }));
  expect(await reads.keys().catch((error: unknown) => error)).toEqual({ kind: "transport", message: "fake channel closed" });
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
  ["keys_list", () => reads.keys()],
  ["key_reveal", () => api.reveal("fake-id")],
  ["key_copy", () => api.copy("fake-id")],
  ["key_add", () => api.add({ title: "Fake key", secret: "fake-secret" } as never)],
  ["key_update", () => api.update("fake-id", { title: "Fake key" } as never)],
  ["key_remove", () => api.remove("fake-id")],
  ["key_rotate", () => api.rotate("fake-id", "fake-new-secret")],
  ["recycled_list", () => reads.recycled()],
  ["key_restore", () => api.restore("fake-id")],
  ["key_purge", () => api.purge("fake-id")],
  ["audit_read", () => reads.audit()],
  ["audit_recover", () => api.recoverJournal()],
  ["env_export", () => api.exportEnv("fake-scope", "fake-env")],
  ["env_copy", () => api.copyEnv("fake-scope", "fake-env")],
  ["env_save", () => api.saveEnv("fake-scope", "fake-env")],
  ["env_launch_terminal", () => api.launchTerminal("fake-scope", "fake-env")],
  ["key_history", () => reads.history("fake-id")],
  ["backup_info", () => reads.backup()],
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

// The terminal launch starts a program and cannot be taken back, so the page sends a scope and a
// fresh operation ID and nothing else, and a late reply is a completion, never a failure to retry.
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

test("a terminal launch sends the session, the scope and a fresh operation ID, and no program, folder or argument", async () => {
  native.invoke.mockResolvedValue(2);
  expect(await api.launchTerminal("fake-project", "fake-env")).toBe(2);
  const first = dispatchedOperation("env_launch_terminal");
  expect(first).toMatch(UUID);
  expect(native.invoke.mock.calls).toEqual([
    ["env_launch_terminal", { project: "fake-project", environment: "fake-env", operationId: first, sessionId: nativeOwner() }],
    ["vault_dismiss_warnings", { ids: [first] }],
  ]);
  native.invoke.mockClear();
  await api.launchTerminal("fake-project", "fake-env");
  expect(dispatchedOperation("env_launch_terminal")).not.toBe(first);
});

test("a terminal launch leaves a record that survives the page dropping its reply", async () => {
  native.invoke.mockResolvedValueOnce(2);
  await api.launchTerminal("fake-project", "fake-env");
  expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "completed" }]);
});

test("a cancelled terminal launch answers null and leaves a cancellation, never a completion", async () => {
  native.invoke.mockResolvedValueOnce(null);
  expect(await api.launchTerminal("fake-project", "fake-env")).toBeNull();
  expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "cancelled" }]);
  expect(retainedCompletions()).toHaveLength(1);
});

test("a terminal launch is on record as pending before its reply arrives", async () => {
  native.invoke.mockReturnValueOnce(new Promise(() => {}));
  void api.launchTerminal("fake-project", "fake-env").catch(() => {});
  expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "pending" }]);
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(dispatchedOperation("env_launch_terminal"), "terminalStarted") : undefined);
  await reads.warnings();
});

for (const [kind, outcome] of [["terminalStarted", "completed"], ["terminalSessionEnded", "completedWhileLocked"], ["terminalFailed", "reportedError"], ["terminalDeclined", "cancelled"]] as const) {
  test(`a lost terminal reply is settled from the native ${kind} record and the record is acknowledged`, async () => {
    native.invoke.mockReturnValueOnce(new Promise(() => {}));
    void api.launchTerminal("fake-project", "fake-env").catch(() => {});
    const operation = dispatchedOperation("env_launch_terminal");
    native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, kind) : undefined);
    await reads.warnings();
    expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome }]);
    expect(retainedOperationNotices()).toEqual([]);
    expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [operation] });
    await reads.warnings();
    expect(retainedCompletions()).toHaveLength(1);
  });
}

for (const [reply, settle] of [["a count", (value: unknown) => Promise.resolve(value ?? 1)], ["a decline", () => Promise.resolve(null)], ["a native error", () => Promise.reject({ kind: "launch", message: "fake launch failure" })]] as const) {
  test(`a terminal reply of ${reply} acknowledges its own native record`, async () => {
    native.invoke.mockImplementation(async (command: string) => command === "env_launch_terminal" ? settle(undefined) : undefined);
    await api.launchTerminal("fake-project", "fake-env").catch(() => {});
    expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: [dispatchedOperation("env_launch_terminal")] });
  });
}

test("a terminal transport failure keeps the native record because it may be the only truth", async () => {
  native.invoke.mockRejectedValueOnce(new Error("channel closed"));
  await api.launchTerminal("fake-project", "fake-env").catch(() => {});
  expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
  const operation = dispatchedOperation("env_launch_terminal");
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "terminalStarted") : undefined);
  await reads.warnings();
  expect(retainedOperationNotices()).toMatchObject([{ id: operation, kind: "terminalStarted" }]);
  acknowledgeNotices([operation]);
});

// Class B: a native record settles only a receipt of its own operation.
for (const [request, command, kind] of [
  [() => api.importEnv(...IMPORT), "env_import", "terminalStarted"],
  [() => api.launchTerminal("fake-project", "fake-env"), "env_launch_terminal", "importApplied"],
] as const) {
  test(`a ${kind} record never settles the receipt of ${command}, and stays a notice`, async () => {
    let reply!: (value: unknown) => void;
    native.invoke.mockReturnValueOnce(new Promise((done) => { reply = done; }));
    const settled = request().catch(() => {});
    const operation = dispatchedOperation(command);
    native.invoke.mockImplementation(async (name: string) => name === "vault_warnings" ? recordFor(operation, kind) : undefined);
    await reads.warnings();
    expect(retainedCompletions()).toMatchObject([{ outcome: "pending" }]);
    expect(retainedOperationNotices()).toMatchObject([{ id: operation, kind }]);
    expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
    reply(command === "env_import" ? { added: 0, updated: 0 } : 1);
    await settled;
    expect(retainedCompletions()).toMatchObject([{ outcome: "completed" }]);
  });
}

test("after a reload while the native review is open, the terminal record is the only truth and stays a notice", async () => {
  native.invoke.mockReturnValueOnce(new Promise(() => {}));
  void api.launchTerminal("fake-project", "fake-env").catch(() => {});
  const operation = dispatchedOperation("env_launch_terminal");
  vi.resetModules();
  const reloaded = { reads: (await import("./native")).reads, notices: await import("./operationNotices"), boundary: await import("./sessionBoundary") };
  native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "terminalStarted") : undefined);
  await reloaded.reads.warnings();
  expect(reloaded.boundary.retainedCompletions()).toEqual([]);
  expect(reloaded.notices.retainedOperationNotices()).toMatchObject([{ id: operation, kind: "terminalStarted" }]);
  expect(native.invoke).not.toHaveBeenCalledWith("vault_dismiss_warnings", expect.anything());
  await reads.warnings(); // The page before the reload settles its own receipt, so later tests start clean.
});

// Contract 5: a record from session A settles A's receipt and never changes session B.
test("a terminal record from an older session settles that session's receipt and leaves the newer session current", async () => {
  native.invoke.mockReturnValueOnce(new Promise(() => {}));
  const origin = sessionGeneration();
  void api.launchTerminal("fake-project", "fake-env").catch(() => {});
  const operation = dispatchedOperation("env_launch_terminal");
  invalidateSession();
  const newer = sessionGeneration();
  const lost = vi.fn();
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    native.invoke.mockImplementation(async (command: string) => command === "vault_warnings" ? recordFor(operation, "terminalSessionEnded") : undefined);
    await reads.warnings();
    expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "completedWhileLocked", generation: origin }]);
    expect(lost.mock.calls.map(([event]) => (event as CustomEvent<{ generation: number }>).detail.generation)).toEqual([origin]);
    expect(sessionGeneration()).toBe(newer);
    expect(native.invoke.mock.calls.map(([command]) => command)).not.toContain("vault_lock");
  } finally { window.removeEventListener(SESSION_LOSS_EVENT, lost); }
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
  const outcome = reads.history("fake-id").catch((error: unknown) => error);
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
  // The value arrives sealed, and opens under the scope of its request (U2-R1-04).
  expect(openFor(await api.exportEnv("fake-scope", "fake-env"), plaintextScope.export("fake-scope", "fake-env"))).toBe("TOKEN=fake-approved\n");
});

test("an approved single reveal and copy still answer the value and the clear time", async () => {
  native.invoke.mockResolvedValueOnce("fake-approved-value");
  expect(openFor(await api.reveal("fake-id"), plaintextScope.reveal("fake-id"))).toBe("fake-approved-value");
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
