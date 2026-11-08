import { afterEach, expect, test, vi } from "vitest";
import cases from "../../crates/akm-core/tests/fixtures/time/cases.json";
import { expiryLabel, formatDate, isExpired, keyState, rotationLabel } from "./format";
import { dateOnlyExpiry, isPast, nextChangeAt, remainingOf, statusOf } from "./schedule";
import type { KeyView } from "./types";

// The same table drives crates/akm-core/tests/schedule.rs, so Rust and the page cannot drift (L6).

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Time fixture", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

afterEach(() => vi.unstubAllEnvs());

test("a date-only expiry is the last second of that UTC date, as in the CLI", () => {
  for (const { input, instant } of cases.date_only_expiry) {
    expect(dateOnlyExpiry(input)).toBe(instant);
  }
});

test("the remaining time rounds up for display and never hides an overdue key", () => {
  for (const row of cases.remaining) {
    const expected = row.state === "now" ? { state: "now" } : { state: row.state, days: row.days };
    expect(remainingOf(Date.parse(row.due), Date.parse(row.now))).toEqual(expected);
    expect(isPast(remainingOf(Date.parse(row.due), Date.parse(row.now)))).toBe(row.state !== "in");
  }
});

test("the expired state flips at the exact last second of the date", () => {
  for (const row of cases.expired_at) {
    const item = key({ expiresAt: row.expires });
    expect(isExpired(item, Date.parse(row.now))).toBe(row.expired);
    expect(keyState(item, Date.parse(row.now)).label === "Expired").toBe(row.expired);
  }
});

test("the date shown for a date-only expiry is the entered date in every time zone", () => {
  for (const zone of ["UTC", "America/Los_Angeles", "Pacific/Kiritimati", "Asia/Kolkata", "Pacific/Auckland", "Etc/GMT+12"]) {
    vi.stubEnv("TZ", zone);
    const local = new Date("2026-11-10T23:59:59Z").getTimezoneOffset();
    if (zone === "Pacific/Kiritimati") expect(local).toBe(-840);
    if (zone === "America/Los_Angeles") expect(local).toBe(480);
    for (const { input } of cases.date_only_expiry) {
      const iso = dateOnlyExpiry(input);
      const month = new Date(iso).toLocaleDateString("en-US", { month: "short", timeZone: "UTC" });
      const label = formatDate(iso);
      expect(label, `${zone} ${input}`).toContain(month);
      expect(label, `${zone} ${input}`).toContain(input.slice(8, 10));
      expect(expiryLabel(key({ expiresAt: iso })), `${zone} ${input}`).toBe(label);
    }
  }
});

test("the rotation label is computed from the due time and the clock, not from a stored count", () => {
  const item = key({ rotationDays: 30, rotationDueAt: "2026-01-31T00:00:00Z", secretChange: "known", secretChangedAt: "2026-01-01T00:00:00Z" });
  const at = (iso: string) => rotationLabel(item, Date.parse(iso));
  expect(at("2026-01-01T00:00:00Z")).toBe("in 30 days");
  expect(at("2026-01-30T12:00:00Z")).toBe("in 1 day");
  expect(at("2026-01-31T00:00:00Z")).toBe("due now");
  expect(at("2026-01-31T12:00:00Z")).toBe("overdue");
  expect(rotationLabel(key({ rotationDays: 30 }), Date.now())).toBe("unknown");
  expect(rotationLabel(key(), Date.now())).toBe("—");
});

test("the next change is the moment a day count drops or a due time passes", () => {
  const item = key({ expiresAt: "2026-02-01T00:00:00Z" });
  const now = Date.parse("2026-01-30T12:00:00Z");
  expect(nextChangeAt([item], now)).toBe(Date.parse("2026-01-31T00:00:00Z"));
  expect(nextChangeAt([item], Date.parse("2026-01-31T00:00:00Z"))).toBe(Date.parse("2026-02-01T00:00:00Z"));
  expect(nextChangeAt([item], Date.parse("2026-02-01T00:00:00Z"))).toBe(Date.parse("2026-02-01T00:00:00.001Z"));
  expect(nextChangeAt([item], Date.parse("2026-02-01T12:00:00Z"))).toBe(Date.parse("2026-02-02T00:00:00Z"));
  expect(nextChangeAt([key()], now)).toBeNull();
  expect(statusOf(key(), now)).toEqual({ expiry: null, rotation: null });
});
