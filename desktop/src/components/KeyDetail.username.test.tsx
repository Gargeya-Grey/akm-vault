import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyDetail } from "./KeyDetail";
import type { KeyView } from "../types";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
afterEach(cleanup);

// FV-12: the vault keeps a username, for example an access key ID. The drawer shows it as a fact.
const item: KeyView = {
  id: "fake-key", title: "Fake AWS", username: "AKIAFAKEEXAMPLE", issuer: "AWS", project: "demo",
  environment: "dev", scopes: [], envVar: "AWS_SECRET_ACCESS_KEY", resolvedEnvVar: "AWS_SECRET_ACCESS_KEY",
  url: null, notes: null, tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null,
  rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
};

function open(overrides: Partial<KeyView> = {}) {
  return render(<KeyDetail current item={{ ...item, ...overrides }} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />);
}

function factRow(name: string): HTMLElement | null {
  const key = [...document.querySelectorAll(".fact-key")].find((element) => element.textContent === name);
  return (key?.closest(".fact-row") as HTMLElement | null) ?? null;
}

test("a key with a username shows it in the drawer", () => {
  open();
  const row = factRow("Username");
  expect(row).not.toBeNull();
  expect(within(row!).getByText("AKIAFAKEEXAMPLE")).toBeTruthy();
});

test("a key without a username shows no username row", () => {
  open({ username: null });
  expect(factRow("Username")).toBeNull();
  expect(screen.queryByText("AKIAFAKEEXAMPLE")).toBeNull();
});

test("a username draws by the rule of the row facts, so a hidden character shows", () => {
  open({ username: "AKIA\u200BFAKE " });
  expect(within(factRow("Username")!).getByText("AKIA⟨U+200B⟩FAKE⟨U+0020⟩")).toBeTruthy();
});
