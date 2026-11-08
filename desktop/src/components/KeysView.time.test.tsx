import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeysView } from "./KeysView";
import type { KeyView } from "../types";

// P2-TIME-REFRESH: the expired and rotation states follow the clock while Keys stays open.
// The refresh reads the wall clock only. It calls nothing native, so it is not user activity.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Clock proof", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

const state = (title: string) => screen.getByRole("button", { name: `Open ${title}` }).querySelector(".state")?.textContent;

async function show(keys: KeyView[]) {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "keys_list") return keys;
    throw new Error(`unexpected command ${command}`);
  });
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
  screen.getByRole("button", { name: `Open ${keys[0].title}` });
  native.invoke.mockClear();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-06-01T12:00:00Z"));
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  native.invoke.mockReset();
});

test("an expiry that passes while the list is open flips the state without a reload", async () => {
  await show([key({ expiresAt: "2026-06-01T12:00:10Z" })]);
  expect(state("Clock proof")).toBe("Expires soon");
  expect(screen.getByText("need attention").previousSibling?.textContent).toBe("1");
  await act(async () => { vi.advanceTimersByTime(9_000); });
  expect(state("Clock proof")).toBe("Expires soon");
  await act(async () => { vi.advanceTimersByTime(2_000); });
  expect(state("Clock proof")).toBe("Expired");
  expect(screen.getByText("need attention").previousSibling?.textContent).toBe("1");
});

test("a rotation that becomes due while the list is open shows overdue, and the day count drops", async () => {
  await show([key({ rotationDays: 30, secretChange: "known", secretChangedAt: "2026-05-02T12:00:00Z", rotationDueAt: "2026-06-03T12:00:00Z" })]);
  expect(screen.getByRole("button", { name: "Open Clock proof" }).textContent).toContain("rotate in 2 days");
  await act(async () => { vi.advanceTimersByTime(86_400_000 + 1_000); });
  expect(screen.getByRole("button", { name: "Open Clock proof" }).textContent).toContain("rotate in 1 day");
  await act(async () => { vi.advanceTimersByTime(86_400_000); });
  expect(state("Clock proof")).toBe("Rotation overdue");
  expect(screen.getByRole("button", { name: "Open Clock proof" }).textContent).toContain("rotate overdue");
});

test("after the page was hidden, coming back shows the state of the new time at once", async () => {
  await show([key({ expiresAt: "2026-06-02T12:00:00Z" })]);
  expect(state("Clock proof")).toBe("Expires soon");
  Object.defineProperty(document, "hidden", { configurable: true, value: true });
  document.dispatchEvent(new Event("visibilitychange"));
  // A hidden page runs no timers. Two days pass.
  vi.setSystemTime(new Date("2026-06-03T12:00:00Z"));
  expect(state("Clock proof")).toBe("Expires soon");
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
  await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(state("Clock proof")).toBe("Expired");
  delete (document as { hidden?: boolean }).hidden;
});

test("a system clock moved forward or back is noticed within a minute", async () => {
  await show([key({ expiresAt: "2026-06-02T12:00:00Z" })]);
  expect(state("Clock proof")).toBe("Expires soon");
  vi.setSystemTime(new Date("2026-06-05T12:00:00Z"));
  await act(async () => { vi.advanceTimersByTime(60_000); });
  expect(state("Clock proof")).toBe("Expired");
  vi.setSystemTime(new Date("2026-05-10T12:00:00Z"));
  await act(async () => { vi.advanceTimersByTime(60_000); });
  expect(state("Clock proof")).toBe("Expires soon");
});

test("a refresh calls nothing native, so it is not user activity and cannot extend the idle deadline", async () => {
  await show([key({ expiresAt: "2026-06-01T12:00:10Z" })]);
  await act(async () => { vi.advanceTimersByTime(30 * 60_000); });
  expect(state("Clock proof")).toBe("Expired");
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
  await act(async () => { document.dispatchEvent(new Event("visibilitychange")); window.dispatchEvent(new Event("focus")); });
  expect(native.invoke).not.toHaveBeenCalled();
  delete (document as { hidden?: boolean }).hidden;
});
