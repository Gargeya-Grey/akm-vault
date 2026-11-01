import { afterEach, expect, test, vi } from "vitest";
import { api } from "./api";
import { COMPLETION_EVENT, invalidateSession } from "./sessionBoundary";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
afterEach(() => vi.resetAllMocks());

test("a mutation reply overtaken by lock emits a secret-free receipt, not a retryable failure", async () => {
  let resolve!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const notice = vi.fn();
  window.addEventListener(COMPLETION_EVENT, notice);
  try {
    const outcome = api.confirmImport("fake-single-use-token").catch((error: unknown) => error);
    invalidateSession();
    resolve({ added: 1, updated: 0 });
    expect(await outcome).toMatchObject({ kind: "completedWhileLocked", message: expect.stringContaining("Do not repeat") });
    expect(notice).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(notice.mock.calls)).not.toContain("fake-single-use-token");
  } finally { window.removeEventListener(COMPLETION_EVENT, notice); }
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
