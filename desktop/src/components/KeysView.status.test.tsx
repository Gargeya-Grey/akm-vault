import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeysView } from "./KeysView";
import type { KeyView } from "../types";

// M12: each key row announces its expiry and rotation status to a screen reader.
// The button keeps its short name ("Open <title>"); the status is its description.

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Status proof", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

async function show(keys: KeyView[]) {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "keys_list") return keys;
    throw new Error(`unexpected command ${command}`);
  });
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
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

const row = (name: string, description: string, identity = "Project demo, environment dev, variable FAKE_KEY.") =>
  screen.getByRole("button", { name, description: `${identity} ${description}` });

test("two keys with one title differ in their accessible description", async () => {
  await show([key({ id: "a", title: "Twin", project: "alpha", environment: "dev", resolvedEnvVar: "TWIN_A" }), key({ id: "b", title: "Twin", project: null, environment: null, resolvedEnvVar: "TWIN_B" })]);
  row("Open Twin", "No expiry date. No rotation interval.", "Project alpha, environment dev, variable TWIN_A.");
  row("Open Twin", "No expiry date. No rotation interval.", "No project, no environment, variable TWIN_B.");
});

test("a key with no project names that when other keys have a project", async () => {
  await show([key({ id: "a", title: "Alpha", project: "alpha-project" }), key({ id: "b", title: "Loose", project: null })]);
  await act(async () => { fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Loose" } }); });
  expect(screen.getByRole("heading", { name: "No project" }).closest("section")?.textContent).toContain("Loose");
});

test("a key with no expiry date says so, and the shelf counts the projects", async () => {
  await show([key({ id: "a", title: "Alpha", project: "one" }), key({ id: "b", title: "Beta", project: "two" })]);
  expect(screen.getAllByRole("button", { name: /^Open / })[0].textContent).toContain("no expiry");
  expect(document.querySelector(".shelf-label")?.textContent).toBe("Projects 2");
});

test("a failed read does not say that the vault is empty, and it can be tried again", async () => {
  let fail = true;
  native.invoke.mockImplementation(async (command: string) => {
    if (command !== "keys_list") throw new Error(`unexpected command ${command}`);
    if (fail) throw { kind: "internal", message: "fake read failure" };
    return [key({ title: "Back again" })];
  });
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
  expect(screen.queryByText("No keys yet.")).toBeNull();
  expect(document.body.textContent).toContain("fake read failure");
  expect(screen.getByRole("heading", { name: "AKM could not read the keys." })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^All keys/ })).toBeNull();
  fail = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
  await act(async () => {});
  screen.getByRole("button", { name: "Open Back again" });
  expect(screen.queryByText("AKM could not read the keys.")).toBeNull();
});

test("a search that leaves one project still names that project", async () => {
  await show([key({ id: "a", title: "Alpha", project: "alpha-project" }), key({ id: "b", title: "Beta", project: "beta-project" })]);
  await act(async () => { fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Alpha" } }); });
  screen.getByRole("heading", { name: "alpha-project" });
  expect(screen.queryByRole("heading", { name: "beta-project" })).toBeNull();
});

test("a vault with no projects shows no project heading", async () => {
  await show([key({ title: "Loose", project: null })]);
  expect(screen.queryByRole("heading", { name: "No project" })).toBeNull();
});

test("an expired key says how long ago it expired", async () => {
  await show([key({ title: "Expired key", expiresAt: "2026-05-30T09:00:00Z" })]);
  row("Open Expired key", "Expired 2 days ago. No rotation interval.");
});

test("a key that expired within the last day says so", async () => {
  await show([key({ title: "Just expired", expiresAt: "2026-06-01T08:00:00Z" })]);
  row("Open Just expired", "Expired less than a day ago. No rotation interval.");
});

test("a key that expires soon says when", async () => {
  await show([key({ title: "Soon key", expiresAt: "2026-06-04T09:00:00Z" })]);
  row("Open Soon key", "Expires in 3 days. No rotation interval.");
});

test("an overdue rotation says by how much", async () => {
  await show([key({ title: "Overdue key", rotationDays: 30, secretChange: "known", secretChangedAt: "2026-04-25T12:00:00Z", rotationDueAt: "2026-05-25T12:00:00Z" })]);
  row("Open Overdue key", "No expiry date. Rotation overdue by 7 days.");
});

test("a rotation that is due within the hour says due now", async () => {
  await show([key({ title: "Due now key", rotationDays: 30, secretChange: "known", secretChangedAt: "2026-05-02T12:00:00Z", rotationDueAt: "2026-06-01T12:00:00Z" })]);
  row("Open Due now key", "No expiry date. Rotation due now.");
});

test("a key with an interval and no known change time says the rotation is unknown, never on schedule", async () => {
  await show([key({ title: "Unknown key", rotationDays: 30 })]);
  row("Open Unknown key", "No expiry date. Rotation unknown, no known secret-change time.");
});

test("a secret changed outside AKM is also unknown", async () => {
  await show([key({ title: "Outside key", rotationDays: 30, secretChange: "changed_outside" })]);
  row("Open Outside key", "No expiry date. Rotation unknown, no known secret-change time.");
});

test("a key on schedule says when each date comes", async () => {
  await show([key({ title: "Fine key", expiresAt: "2026-08-30T09:00:00Z", rotationDays: 90, secretChange: "known", secretChangedAt: "2026-05-22T12:00:00Z", rotationDueAt: "2026-07-11T12:00:00Z" })]);
  row("Open Fine key", "Expires in 90 days. Rotation due in 40 days.");
});

test("a key with neither date says so", async () => {
  await show([key({ title: "Plain key" })]);
  row("Open Plain key", "No expiry date. No rotation interval.");
});

test("the description follows the clock while the list stays open", async () => {
  await show([key({ title: "Clock key", expiresAt: "2026-06-01T12:00:10Z" })]);
  row("Open Clock key", "Expires in 1 day. No rotation interval.");
  await act(async () => { vi.advanceTimersByTime(11_000); });
  row("Open Clock key", "Expired less than a day ago. No rotation interval.");
});

test("every row of a longer list has its own description and no row reuses another's", async () => {
  await show([
    key({ id: "a", title: "Alpha", expiresAt: "2026-05-31T12:00:00Z" }),
    key({ id: "b", title: "Bravo", rotationDays: 10 }),
  ]);
  row("Open Alpha", "Expired 1 day ago. No rotation interval.");
  row("Open Bravo", "No expiry date. Rotation unknown, no known secret-change time.");
});
