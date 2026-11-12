import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { acknowledgeCompletions, beginOutcome, COMPLETION_MESSAGE, reportCompletion, retainedCompletions, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { beginNativeOpen, endNativeOpen } from "./nativeSession";

const native = vi.hoisted(() => ({
  owner: "",
  onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void),
  status: vi.fn(),
  reload: vi.fn(),
  upgrade: vi.fn(),
  lock: vi.fn().mockResolvedValue(undefined),
  activity: vi.fn().mockResolvedValue(undefined),
  warnings: vi.fn().mockResolvedValue({ application: [], operations: [] }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (command: string) => {
  const commands = { vault_status: native.status, vault_reload: native.reload, vault_upgrade: native.upgrade, vault_lock: native.lock, vault_activity: native.activity, vault_warnings: native.warnings };
  const handler = commands[command as keyof typeof commands];
  if (!handler) throw new Error(command);
  return handler();
} }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: typeof native.onLock) => {
    if (event !== "vault-locked") return () => {};
    native.onLock = callback;
    return () => { native.onLock = undefined; };
  }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked secrets region</p> }));
vi.mock("./components/Unlock", () => ({ Unlock: () => <p>Unlock screen</p> }));

beforeEach(() => {
  vi.clearAllMocks();
  native.owner = crypto.randomUUID();
  const token = beginNativeOpen();
  endNativeOpen(token);
  native.status.mockReset().mockResolvedValue({ sessionId: native.owner, path: "C:/test/vault.kdbx", unlocked: true, exists: true, keyCount: 1, vaultName: "Test", needsUpgrade: false });
});
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); vi.useRealTimers(); });

test("a native vault lock removes unlocked views immediately", async () => {
  render(<App />);
  await screen.findByText("Unlocked secrets region");
  act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.getByText("Unlock screen")).toBeTruthy();
});

test("a delayed unlocked status response cannot resurrect a natively locked view", async () => {
  vi.useFakeTimers();
  await act(async () => { render(<App />); });
  expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
  let resolveStatus!: (value: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((resolve) => { resolveStatus = resolve; }));
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(native.status).toHaveBeenCalledTimes(2);
  act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
  await act(async () => resolveStatus({ sessionId: native.owner, path: "C:/test/vault.kdbx", unlocked: true, exists: true, keyCount: 1, vaultName: "Test", needsUpgrade: false }));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.getByText("Unlock screen")).toBeTruthy();
  expect(native.activity).not.toHaveBeenCalled();
});

for (const operation of ["reload", "upgrade"] as const) {
  test(`a delayed ${operation} response cannot undo a newer native lock`, async () => {
    const status = { sessionId: native.owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Fake", needsUpgrade: operation === "upgrade" };
    native.status.mockResolvedValueOnce(status);
    let resolve!: (value: unknown) => void;
    native[operation].mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<App />);
    await screen.findByText("Unlocked secrets region");
    if (operation === "upgrade") {
      fireEvent.click(screen.getByRole("button", { name: "Review format upgrade" }));
      fireEvent.click(screen.getByRole("button", { name: "Upgrade format" }));
    } else {
      fireEvent.click(screen.getByRole("button", { name: "Reload vault" }));
    }
    act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
    await act(async () => resolve(status));
    expect(screen.queryByText("Unlocked secrets region")).toBeNull();
    expect(screen.getByText("Unlock screen")).toBeTruthy();
  });
}

test("a completed-operation receipt stays visible without reopening views", async () => {
  render(<App />);
  await screen.findByText("Unlocked secrets region");
  act(reportCompletion);
  expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
  expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
});

test("the retained notice of a terminal from an older session names the terminal and leaves the newer session open", async () => {
  render(<App />);
  await screen.findByText("Unlocked secrets region");
  act(() => settleOutcome(beginOutcome("terminal", sessionGeneration() - 1), "completedWhileLocked"));
  expect(screen.getByText(/A terminal opened with keys\. It keeps them until you close it, even after AKM locks\./)).toBeTruthy();
  expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
});

// Each terminal outcome says only what is known: whether a terminal holds keys, may hold them, or none runs.
for (const [outcome, says, never] of [
  ["pending", /A terminal may already hold keys\./, /opened with keys/],
  ["completed", /A terminal opened with keys\. It keeps them until you close it/, /may|No terminal/],
  ["cancelled", /No terminal from that request runs\./, /opened with keys|may/],
  ["reportedError", /This does not confirm that no terminal runs\./, /opened with keys|No terminal from/],
  ["unconfirmed", /A terminal may hold keys\./, /opened with keys|No terminal from/],
] as const) {
  test(`a ${outcome} terminal receipt says only what is known`, async () => {
    render(<App />);
    await screen.findByText("Unlocked secrets region");
    let id = 0;
    act(() => { id = beginOutcome("terminal", sessionGeneration()); if (outcome !== "pending") settleOutcome(id, outcome); });
    const text = screen.getByRole("region", { name: "Operation outcomes" }).textContent!;
    expect(text).toMatch(says);
    expect(text).not.toMatch(never);
    expect(screen.getByText("Unlocked secrets region")).toBeTruthy();
    act(() => settleOutcome(id, "cancelled")); // A pending receipt cannot be dismissed, so settle it for the next test.
  });
}

test("a lock during the initial status request requires fresh metadata for a usable locked screen", async () => {
  let resolve!: (value: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await act(async () => { render(<App />); });
  act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
  await act(async () => resolve({ sessionId: native.owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Private", needsUpgrade: false }));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.queryByText("Unlock screen")).toBeNull();
  const retry = screen.getByRole("button", { name: "Try again" });
  native.status.mockResolvedValueOnce({ sessionId: null, path: "C:/fake/current.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  fireEvent.click(retry);
  await screen.findByText("Unlock screen");
});

test("a rejected initial status overtaken by lock leaves a retry instead of endless loading", async () => {
  let reject!: (cause: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((_done, failed) => { reject = failed; }));
  await act(async () => { render(<App />); });
  act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
  await act(async () => reject("obsolete status failure"));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.queryByText("obsolete status failure")).toBeNull();
  const retry = screen.getByRole("button", { name: "Try again" });
  native.status.mockResolvedValueOnce({ sessionId: null, path: "C:/fake/vault.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  fireEvent.click(retry);
  await screen.findByText("Unlock screen");
});
