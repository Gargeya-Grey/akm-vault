import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeyDetail } from "./KeyDetail";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import type { KeyView } from "../types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));

const item: KeyView = {
  id: "fake-key", title: "Fake history fixture", username: null, issuer: null, project: "demo",
  environment: "dev", scopes: [], envVar: "TOKEN", resolvedEnvVar: "TOKEN", url: null, notes: null,
  tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
  secretChangedAt: null,
  secretChange: "unknown",
  historyCount: 2,
};

// The reply of the native command, with a field that a reply must never carry. The view must
// not render what it does not know.
const versions = [
  { index: 0, modifiedAt: "2026-10-30T12:00:00Z", changed: ["Password", "AKM_SECRET_CHANGED"], secretChanged: true, value: "fake-history-secret" },
  { index: 1, modifiedAt: "2026-10-01T09:30:00Z", changed: ["Notes", "Title"], secretChanged: false, value: "fake-older-secret" },
];

const SHOW = "Show earlier versions";

beforeEach(() => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "key_history") return versions;
    throw new Error(`unexpected command ${command}`);
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function open(overrides: Partial<KeyView> = {}) {
  const view = render(<KeyDetail current item={{ ...item, ...overrides }} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
  return view;
}

function hide() {
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
}

test("a key without earlier versions offers no history button", () => {
  open({ historyCount: 0 });
  expect(screen.queryByRole("button", { name: SHOW })).toBeNull();
  expect(screen.getByText("No earlier versions")).toBeTruthy();
});

test("the history lists each version's time and changed fields and never a value", async () => {
  open();
  expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  await act(async () => {});
  expect(native.invoke).toHaveBeenCalledWith("key_history", expect.objectContaining({ id: "fake-key" }));
  const list = screen.getByRole("list", { name: "Earlier versions" });
  const rows = Array.from(list.querySelectorAll("li"));
  expect(rows).toHaveLength(2);
  expect(rows[0].textContent).toContain("Changed: secret");
  expect(rows[1].textContent).toContain("Changed: notes, title");
  for (const row of rows) expect(row.querySelector(".mono")).not.toBeNull();
  expect(document.body.textContent).not.toContain("fake-history-secret");
  expect(document.body.textContent).not.toContain("fake-older-secret");
  expect(screen.queryByRole("button", { name: SHOW })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Hide earlier versions" }));
  expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
});

test("a version without a recorded time says so", async () => {
  native.invoke.mockResolvedValueOnce([{ index: 0, modifiedAt: null, changed: [], secretChanged: false }]);
  open({ historyCount: 1 });
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  await act(async () => {});
  expect(screen.getByRole("list", { name: "Earlier versions" }).textContent).toContain("Time unknown");
});

for (const how of ["native view expiry", "hide"] as const) {
  test(`${how} clears a loaded history`, async () => {
    open();
    fireEvent.click(screen.getByRole("button", { name: SHOW }));
    await act(async () => {});
    expect(screen.getByRole("list", { name: "Earlier versions" })).toBeTruthy();
    if (how === "hide") hide();
    else act(() => { window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT)); });
    expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
    expect(screen.getByRole("button", { name: SHOW })).toBeTruthy();
  });

  test(`a history reply that arrives after ${how} is ignored`, async () => {
    let resolve!: (value: unknown) => void;
    native.invoke.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    open();
    fireEvent.click(screen.getByRole("button", { name: SHOW }));
    if (how === "hide") hide();
    else act(() => { window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT)); });
    await act(async () => {
      resolve(versions);
      await new Promise((done) => setTimeout(done, 0));
    });
    expect(native.invoke).toHaveBeenCalledWith("key_history", expect.anything());
    expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
    expect(screen.getByRole("button", { name: SHOW }).getAttribute("aria-busy")).toBeNull();
  });
}

test("a history that belongs to one key is gone when the drawer shows another key", async () => {
  const view = open();
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  await act(async () => {});
  expect(screen.getByRole("list", { name: "Earlier versions" })).toBeTruthy();
  view.rerender(<KeyDetail current item={{ ...item, id: "fake-other-key" }} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
  expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
});

test("a failed history request shows the error, and a retry that succeeds replaces it with the list", async () => {
  native.invoke.mockRejectedValueOnce({ kind: "internal", message: "fake history failure" });
  open();
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  await act(async () => {});
  expect(screen.getByText("fake history failure")).toBeTruthy();
  expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await act(async () => {});
  expect(screen.queryByText("fake history failure")).toBeNull();
  expect(screen.getByRole("list", { name: "Earlier versions" })).toBeTruthy();
});

test("while the history is read, the drawer says so and shows no list", async () => {
  native.invoke.mockReturnValueOnce(new Promise(() => {}));
  open();
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  expect(screen.getByRole("status").textContent).toBe("Reading the earlier versions…");
  expect(screen.queryByRole("list", { name: "Earlier versions" })).toBeNull();
});

test("a reply to a history list that was closed never shows in the list opened after it", async () => {
  let older!: (value: unknown) => void;
  native.invoke.mockReturnValueOnce(new Promise((done) => { older = done; }));
  open();
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  fireEvent.click(screen.getByRole("button", { name: "Hide earlier versions" }));
  fireEvent.click(screen.getByRole("button", { name: SHOW }));
  await act(async () => {});
  await act(async () => { older([{ index: 9, modifiedAt: null, changed: [], secretChanged: false }]); });
  expect(screen.getByRole("list", { name: "Earlier versions" }).querySelectorAll("li")).toHaveLength(2);
});
