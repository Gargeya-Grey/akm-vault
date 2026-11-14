import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { adoptReservation } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import type { KeyView } from "./types";

// Unit 4: the unlock summary. It lists the keys that are expired, overdue for rotation, due soon
// or unknown, with counts and titles only. It is dismissible, does not come back until the next
// unlock, and a lock removes it. It uses keyState, the same function as the Keys screen.

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: typeof native.onLock) => {
    if (event === "vault-locked") native.onLock = callback;
    return () => {};
  }),
}));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/Unlock", () => ({
  Unlock: ({ onUnlocked }: { onUnlocked: (status: unknown) => void }) => (
    <button type="button" onClick={() => onUnlocked(status())}>Fake unlock</button>
  ),
}));

let owner = "";
let keys: KeyView[] = [];
const status = () => ({ sessionId: owner, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: keys.length, vaultName: "Fake", needsUpgrade: false });

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Plain key", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

const NOW = "2026-06-01T12:00:00Z";
const mixed = () => [
  key({ id: "1", title: "Expired key", expiresAt: "2026-05-30T09:00:00Z", notes: "NOTE-SHOULD-NOT-APPEAR", url: "https://private.example/path" }),
  key({ id: "2", title: "Overdue key", rotationDays: 30, secretChange: "known", secretChangedAt: "2026-04-25T12:00:00Z", rotationDueAt: "2026-05-25T12:00:00Z" }),
  key({ id: "3", title: "Soon key", expiresAt: "2026-06-11T12:00:00Z" }),
  key({ id: "4", title: "Unknown key", rotationDays: 30 }),
  key({ id: "5", title: "Fine key", expiresAt: "2027-06-01T12:00:00Z" }),
];

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(NOW));
  adoptReservation(crypto.randomUUID());
  owner = crypto.randomUUID();
  keys = [];
  native.onLock = undefined;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "keys_list") return keys;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "backup_info") return { path: "C:/fake/vault.kdbx.bak", exists: false, bytes: 0, modified: null };
    throw new Error(command);
  });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  native.invoke.mockReset();
});

const summary = () => screen.queryByRole("region", { name: "Keys needing attention" });
async function open() {
  await act(async () => { render(<App />); });
  await screen.findByRole("button", { name: /^Open / }).catch(() => undefined);
  await act(async () => {});
}

test("after unlock the summary lists expired, overdue, due soon and unknown keys by title, short ID and count only", async () => {
  keys = mixed();
  await open();
  const box = summary()!;
  expect(box).toBeTruthy();
  expect(box.querySelector("h2")?.textContent).toBe("4 keys need attention");
  expect([...box.querySelectorAll("li")].map((item) => item.textContent)).toEqual([
    "Expired (1): “Expired key” (ID 1)",
    "Rotation overdue (1): “Overdue key” (ID 2)",
    "Due soon (1): “Soon key” (ID 3)",
    "Rotation unknown (1): “Unknown key” (ID 4)",
  ]);
  expect(box.textContent).not.toContain("Fine key");
  for (const private_ of ["NOTE-SHOULD-NOT-APPEAR", "private.example", "FAKE_KEY", "demo"]) expect(box.textContent).not.toContain(private_);
});

test("one key that needs attention is worded in the singular", async () => {
  keys = [key({ title: "Only one", expiresAt: "2026-05-01T12:00:00Z" })];
  await open();
  expect(summary()!.querySelector("h2")?.textContent).toBe("1 key needs attention");
});

test("a key counts once, under its main state", async () => {
  keys = [key({ title: "Both", expiresAt: "2026-05-01T12:00:00Z", rotationDays: 30 })];
  await open();
  expect([...summary()!.querySelectorAll("li")].map((item) => item.textContent)).toEqual(["Expired (1): “Both” (ID fake-id)"]);
});

test("a long list names five titles per state and counts the rest", async () => {
  keys = Array.from({ length: 7 }, (_, index) => key({ id: `k${index}`, title: `Old ${index}`, expiresAt: "2026-05-01T12:00:00Z" }));
  await open();
  expect(summary()!.querySelector("li")?.textContent).toBe("Expired (7): “Old 0” (ID k0), “Old 1” (ID k1), “Old 2” (ID k2), “Old 3” (ID k3), “Old 4” (ID k4), and 2 more");
});

test("no keys and no key that needs attention show no summary", async () => {
  keys = [];
  await open();
  expect(summary()).toBeNull();
  cleanup();
  keys = [key({ title: "Fine", expiresAt: "2027-06-01T12:00:00Z", rotationDays: 90, secretChange: "known", secretChangedAt: "2026-05-20T12:00:00Z", rotationDueAt: "2026-08-18T12:00:00Z" })];
  await open();
  expect(screen.getByRole("button", { name: "Open Fine" })).toBeTruthy();
  expect(summary()).toBeNull();
});

test("dismissing removes the summary, moves focus to the search field and keeps it away across views", async () => {
  keys = mixed();
  const user = userEvent.setup({ delay: null });
  await open();
  const dismiss = screen.getByRole("button", { name: "Dismiss summary" });
  dismiss.focus();
  await user.keyboard("{Enter}");
  expect(summary()).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("searchbox", { name: "Search keys" }));
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(screen.getByRole("button", { name: "Keys" }));
  await screen.findByRole("button", { name: "Open Expired key" });
  expect(summary()).toBeNull();
});

test("leaving the Keys view without dismissing does not dismiss; the summary is still there on return", async () => {
  keys = mixed();
  const user = userEvent.setup({ delay: null });
  await open();
  await user.click(screen.getByRole("button", { name: "Settings" }));
  expect(summary()).toBeNull();
  await user.click(screen.getByRole("button", { name: "Keys" }));
  await screen.findByRole("button", { name: "Open Expired key" });
  expect(summary()).toBeTruthy();
});

test("a lock while the summary is open removes it at once, and the next unlock shows it again even after a dismissal", async () => {
  keys = mixed();
  await open();
  expect(summary()).toBeTruthy();
  act(() => native.onLock?.({ payload: { sessionId: owner } }));
  expect(summary()).toBeNull();
  expect(document.body.textContent).not.toContain("Expired key");
  owner = crypto.randomUUID();
  adoptReservation(owner);
  fireEvent.click(screen.getByRole("button", { name: "Fake unlock" }));
  await screen.findByRole("button", { name: "Open Expired key" });
  expect(summary()).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss summary" }));
  expect(summary()).toBeNull();
  act(() => native.onLock?.({ payload: { sessionId: owner } }));
  owner = crypto.randomUUID();
  adoptReservation(owner);
  fireEvent.click(screen.getByRole("button", { name: "Fake unlock" }));
  await screen.findByRole("button", { name: "Open Expired key" });
  expect(summary()).toBeTruthy();
});

test("a hidden window neither dismisses the summary nor shows it again", async () => {
  keys = mixed();
  await open();
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(summary()).toBeTruthy();
  hidden.mockReturnValue(false);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  fireEvent.click(screen.getByRole("button", { name: "Dismiss summary" }));
  hidden.mockReturnValue(true);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  hidden.mockReturnValue(false);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(summary()).toBeNull();
  hidden.mockRestore();
});

test("the summary follows the clock while it is open", async () => {
  keys = [key({ title: "Clock key", expiresAt: "2026-06-04T12:00:00Z" })];
  await open();
  expect(summary()!.querySelector("li")?.textContent).toBe("Due soon (1): “Clock key” (ID fake-id)");
  vi.setSystemTime(new Date("2026-06-05T12:00:00Z"));
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(summary()!.querySelector("li")?.textContent).toBe("Expired (1): “Clock key” (ID fake-id)");
});

test("the Keys screen count of keys that need attention equals the summary count", async () => {
  keys = mixed();
  await open();
  expect(screen.getByText("need attention").previousSibling?.textContent).toBe("4");
});
