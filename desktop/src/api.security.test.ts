import { afterEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { acknowledgeCompletions, COMPLETION_EVENT, invalidateSession, retainedCompletions, SESSION_LOSS_EVENT } from "./sessionBoundary";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
afterEach(() => { vi.resetAllMocks(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); });

test("a mutation reply overtaken by lock emits a secret-free receipt, not a retryable failure", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const notice = vi.fn();
  const lost = vi.fn();
  window.addEventListener(COMPLETION_EVENT, notice);
  window.addEventListener(SESSION_LOSS_EVENT, lost);
  try {
    const outcome = api.confirmImport("fake-single-use-token").catch((error: unknown) => error);
    invalidateSession();
    resolve({ added: 1, updated: 0 });
    expect(await outcome).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
    expect(notice).toHaveBeenCalledTimes(2); // Pending and settlement update one retained record.
    expect(lost).toHaveBeenCalledTimes(1);
    expect(retainedCompletions()).toHaveLength(1);
    expect(JSON.stringify(notice.mock.calls)).not.toContain("fake-single-use-token");
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
  const error = await api.confirmImport("fake-private-token").catch((cause: unknown) => cause);
  expect(error).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
  const records = retainedCompletions();
  expect(records).toHaveLength(1);
  expect(Object.keys(records[0]).sort()).toEqual(["generation", "id", "operation", "outcome"]);
  expect(records[0]).toMatchObject({ operation: "import", outcome: "completedWhileLocked" });
  for (const privateText of ["fake-private-token", "FAKE_PRIVATE_NATIVE_ERROR", "export.env", "fake-secret"]) {
    expect(JSON.stringify(error)).not.toContain(privateText);
    expect(JSON.stringify(records)).not.toContain(privateText);
  }
});
