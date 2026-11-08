import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useNow } from "./useNow";
import type { KeyView } from "./types";

// 4-W-01: KeyDetail passed a new array to useNow on every render. The effect then ran after
// every render and set the state again, so a moving clock made an endless render loop
// ("Maximum update depth exceeded"). The hook must depend on the due times, not on array identity.

afterEach(() => { cleanup(); vi.useRealTimers(); });

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Loop proof", username: null, issuer: null, project: null, environment: null,
    scopes: [], envVar: null, resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: "2026-06-10T12:00:00Z", rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

test("a new array with the same due times on every render does not restart the clock or loop", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-06-01T12:00:00Z"));
  let renders = 0;
  function Probe({ item }: { item: KeyView }) {
    renders += 1;
    if (renders > 50) throw new Error("render loop");
    useNow([item]);
    vi.setSystemTime(Date.now() + 1);
    return null;
  }
  const item = key();
  await act(async () => { render(<Probe item={item} />); });
  await act(async () => { vi.advanceTimersByTime(10); });
  expect(renders).toBeLessThan(6);
});

test("the clock still follows a change of the due times", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-06-01T12:00:00Z"));
  const seen: number[] = [];
  function Probe({ item }: { item: KeyView }) {
    seen.push(useNow([item]));
    return null;
  }
  const view = render(<Probe item={key({ expiresAt: "2026-06-01T12:00:05Z" })} />);
  await act(async () => { vi.advanceTimersByTime(6_000); });
  const before = seen.length;
  view.rerender(<Probe item={key({ expiresAt: "2026-06-20T12:00:00Z" })} />);
  await act(async () => { vi.advanceTimersByTime(61_000); });
  expect(seen.length).toBeGreaterThan(before);
  expect(seen.at(-1)).toBe(Date.parse("2026-06-01T12:01:06Z"));
});
