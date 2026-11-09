import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { RecycleBin } from "./RecycleBin";
import type { KeyView, RecycledKey } from "../types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

afterEach(() => { cleanup(); native.invoke.mockReset(); });

function recycled(id: string, overrides: Partial<KeyView>): RecycledKey {
  return {
    deletedAt: "2026-06-01T12:00:00Z",
    key: {
      id, title: "Twin", username: null, issuer: null, project: "demo", environment: "dev",
      scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
      expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
      secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
    },
  };
}

async function show(items: RecycledKey[]) {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "recycled_list") return items;
    throw new Error(`unexpected command ${command}`);
  });
  await act(async () => { render(<RecycleBin />); });
  await act(async () => {});
}

test("two recycled keys with one title show what tells them apart, also to a screen reader", async () => {
  await show([
    recycled("a", { project: "shared-long-prefix-alpha", environment: "prod", resolvedEnvVar: "TWIN_A" }),
    recycled("b", { project: "shared-long-prefix-beta", environment: "test", resolvedEnvVar: "TWIN_B" }),
  ]);
  const purge = screen.getAllByRole("button", { name: "Delete Twin for good" });
  expect(purge).toHaveLength(2);
  for (const [button, facts] of [[purge[0], /shared-long-prefix-alpha.*prod.*TWIN_A/], [purge[1], /shared-long-prefix-beta.*test.*TWIN_B/]] as const) {
    const id = button.getAttribute("aria-describedby");
    expect(id).toBeTruthy();
    expect(document.getElementById(id ?? "")?.textContent).toMatch(facts);
  }
  screen.getByRole("button", { name: "Restore Twin", description: /shared-long-prefix-beta/ });
});

test("the delete confirmation names the project, environment and variable of the chosen key", async () => {
  await show([
    recycled("a", { project: "alpha", environment: "prod", resolvedEnvVar: "TWIN_A" }),
    recycled("b", { project: null, environment: null, resolvedEnvVar: "TWIN_B" }),
  ]);
  fireEvent.click(screen.getAllByRole("button", { name: "Delete Twin for good" })[1]);
  const facts = document.querySelector(".confirm .fact-list");
  expect(facts?.textContent).toContain("TWIN_B");
  expect(facts?.textContent).toContain("No project");
  expect(facts?.textContent).toContain("No environment");
  expect(facts?.textContent).not.toContain("TWIN_A");
});

test("a failed read does not say that the Recycle Bin is empty, and it can be tried again", async () => {
  let fail = true;
  native.invoke.mockImplementation(async (command: string) => {
    if (command !== "recycled_list") throw new Error(`unexpected command ${command}`);
    if (fail) throw { kind: "internal", message: "fake read failure" };
    return [recycled("a", {})];
  });
  await act(async () => { render(<RecycleBin />); });
  await act(async () => {});
  expect(screen.queryByText("The Recycle Bin is empty.")).toBeNull();
  expect(screen.getByRole("heading", { name: "AKM could not read the Recycle Bin." })).toBeTruthy();
  fail = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
  await act(async () => {});
  screen.getByRole("button", { name: "Restore Twin" });
});
