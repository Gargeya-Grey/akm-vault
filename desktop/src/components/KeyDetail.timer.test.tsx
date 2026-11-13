import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeyDetail } from "./KeyDetail";
import type { KeyView } from "../types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));

const item: KeyView = {
  id: "fake-key", title: "Fake timer fixture", username: null, issuer: null, project: "demo",
  environment: "dev", scopes: [], envVar: "TOKEN", resolvedEnvVar: "TOKEN", url: null, notes: null,
  tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
  secretChangedAt: null,
  secretChange: "unknown",
  historyCount: 0,
};

beforeEach(() => {
  vi.useFakeTimers();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "key_reveal") return "fake-timer-value";
    throw new Error(`unexpected command ${command}`);
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function reveal() {
  fireEvent.click(screen.getByRole("button", { name: "Reveal" }));
  await act(async () => {});
}

// Owner decision, 1 Nov 2026: the "Hides in N seconds" text is the only timer. There is no bar.
test("the countdown text is the only timer, and it resets on each reveal", async () => {
  render(<KeyDetail current item={item} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
  await reveal();
  expect(screen.getByText("fake-timer-value")).toBeTruthy();
  expect(document.body.textContent).toContain("Hides in 30 seconds");
  expect(document.querySelector(".secret-timer")).toBeNull();

  act(() => vi.advanceTimersByTime(10_000));
  expect(document.body.textContent).toContain("Hides in 20 seconds");

  act(() => vi.advanceTimersByTime(20_000));
  expect(document.body.textContent).not.toContain("fake-timer-value");
  expect(document.body.textContent).not.toContain("Hides in");

  await reveal();
  expect(document.body.textContent).toContain("Hides in 30 seconds");
  expect(document.querySelector(".secret-timer")).toBeNull();
});

// P3-REVEAL-CLOCK: the countdown follows a monotonic clock, so a wall clock moved back does not
// keep a revealed secret on screen.
test("a wall clock moved back 40 seconds does not keep the secret past 30 seconds", async () => {
  render(<KeyDetail current item={item} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
  await reveal();
  expect(screen.getByText("fake-timer-value")).toBeTruthy();

  act(() => vi.advanceTimersByTime(10_000));
  expect(document.body.textContent).toContain("Hides in 20 seconds");
  vi.setSystemTime(Date.now() - 40_000);
  act(() => vi.advanceTimersByTime(1_000));
  expect(document.body.textContent).toContain("Hides in 19 seconds");

  act(() => vi.advanceTimersByTime(19_000));
  expect(document.body.textContent).not.toContain("fake-timer-value");
  expect(document.body.textContent).not.toContain("Hides in");
});

test("a wall clock moved forward does not hide the secret early", async () => {
  render(<KeyDetail current item={item} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
  await reveal();
  act(() => vi.advanceTimersByTime(5_000));
  vi.setSystemTime(Date.now() + 3_600_000);
  act(() => vi.advanceTimersByTime(1_000));
  expect(screen.getByText("fake-timer-value")).toBeTruthy();
  expect(document.body.textContent).toContain("Hides in 24 seconds");
});
