import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeyDetail } from "./KeyDetail";
import type { KeyView } from "../types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));

const item: KeyView = {
  id: "fake-key", title: "Fake timer fixture", username: null, issuer: null, project: "demo",
  environment: "dev", scopes: [], envVar: "TOKEN", resolvedEnvVar: "TOKEN", url: null, notes: null,
  tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, expired: false,
  rotationDueIn: null,
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

function bar() {
  return document.querySelector<HTMLElement>(".secret-timer");
}

function scale() {
  const match = /scaleX\(([^)]+)\)/.exec(bar()?.style.transform ?? "");
  return match ? Number(match[1]) : Number.NaN;
}

async function reveal() {
  fireEvent.click(screen.getByRole("button", { name: "Reveal" }));
  await act(async () => {});
}

test("the remaining-time bar follows the same countdown as the text and resets on each reveal", async () => {
  render(<KeyDetail item={item} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} />);
  await reveal();
  expect(screen.getByText("fake-timer-value")).toBeTruthy();
  expect(scale()).toBe(1);

  act(() => vi.advanceTimersByTime(10_000));
  expect(document.body.textContent).toContain("Hides in 20 seconds");
  expect(scale()).toBeCloseTo(20 / 30, 5);

  act(() => vi.advanceTimersByTime(20_000));
  expect(document.body.textContent).not.toContain("fake-timer-value");
  expect(bar()).toBeNull();

  await reveal();
  expect(scale()).toBe(1);
  expect(document.body.textContent).toContain("Hides in 30 seconds");
});
