import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { COMPLETION_MESSAGE, reportCompletion } from "./sessionBoundary";

const native = vi.hoisted(() => ({
  onLock: undefined as undefined | (() => void),
  status: vi.fn(),
  reload: vi.fn(),
  upgrade: vi.fn(),
  lock: vi.fn().mockResolvedValue(undefined),
  activity: vi.fn().mockResolvedValue(undefined),
  warnings: vi.fn().mockResolvedValue([]),
}));
vi.mock("./api", () => ({ api: native, errorText: (error: unknown) => String(error) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, callback: () => void) => {
    native.onLock = callback;
    return () => { native.onLock = undefined; };
  }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked secrets region</p> }));
vi.mock("./components/Unlock", () => ({ Unlock: () => <p>Unlock screen</p> }));

beforeEach(() => {
  vi.clearAllMocks();
  native.status.mockReset().mockResolvedValue({ path: "C:/test/vault.kdbx", unlocked: true, exists: true, keyCount: 1, vaultName: "Test", needsUpgrade: false });
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

test("a native vault lock removes unlocked views immediately", async () => {
  render(<App />);
  await screen.findByText("Unlocked secrets region");
  act(() => native.onLock?.());
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
  act(() => native.onLock?.());
  await act(async () => resolveStatus({ path: "C:/test/vault.kdbx", unlocked: true, exists: true, keyCount: 1, vaultName: "Test", needsUpgrade: false }));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.getByText("Unlock screen")).toBeTruthy();
  expect(native.activity).not.toHaveBeenCalled();
});

for (const operation of ["reload", "upgrade"] as const) {
  test(`a delayed ${operation} response cannot undo a newer native lock`, async () => {
    const status = { path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Fake", needsUpgrade: operation === "upgrade" };
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
    act(() => native.onLock?.());
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
  act(() => native.onLock?.());
  expect(screen.getByText(COMPLETION_MESSAGE)).toBeTruthy();
});

test("a lock during the initial status request leaves a usable locked screen", async () => {
  let resolve!: (value: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await act(async () => { render(<App />); });
  act(() => native.onLock?.());
  await act(async () => resolve({ path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 1, vaultName: "Private", needsUpgrade: false }));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.getByText("Unlock screen")).toBeTruthy();
});

test("a rejected initial status overtaken by lock leaves a retry instead of endless loading", async () => {
  let reject!: (cause: unknown) => void;
  native.status.mockReturnValueOnce(new Promise((_done, failed) => { reject = failed; }));
  await act(async () => { render(<App />); });
  act(() => native.onLock?.());
  await act(async () => reject("obsolete status failure"));
  expect(screen.queryByText("Unlocked secrets region")).toBeNull();
  expect(screen.queryByText("obsolete status failure")).toBeNull();
  const retry = screen.getByRole("button", { name: "Try again" });
  native.status.mockResolvedValueOnce({ path: "C:/fake/vault.kdbx", exists: true, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
  fireEvent.click(retry);
  await screen.findByText("Unlock screen");
});
