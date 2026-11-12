import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeysView } from "./KeysView";
import { RecycleBin } from "./RecycleBin";
import { cutsOf } from "./glyphs";
import type { KeyView, RecycledKey } from "../types";

// Fault classes M (two different things drawn the same) and H. A row, its look-alike check, its
// accessible description and every dialog that names it draw from one function (rowFacts.ts).

const native = vi.hoisted(() => ({ invoke: vi.fn(), sent: [] as { command: string; id?: string }[] }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const NOW = new Date("2026-06-01T12:00:00Z");
const inDays = (days: number) => new Date(NOW.getTime() + days * 86_400_000).toISOString();

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Twin", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

function serve(keys: KeyView[], recycled: RecycledKey[] = []) {
  native.sent.length = 0;
  native.invoke.mockImplementation(async (command: string, args?: { id?: string }) => {
    native.sent.push({ command, id: args?.id });
    if (command === "keys_list") return keys;
    if (command === "recycled_list") return recycled;
    if (command === "key_remove" || command === "key_purge") return undefined;
    if (command === "vault_status") return { sessionId: "fake", path: "C:/fake/a.kdbx", exists: true, unlocked: true, keyCount: keys.length, vaultName: "A", needsUpgrade: false };
    throw new Error(`unexpected command ${command}`);
  });
}

async function showKeys(keys: KeyView[]) {
  serve(keys);
  await act(async () => { render(<KeysView />); });
  await act(async () => {});
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  native.invoke.mockReset();
});

// Any drawn text in a row that starts with "ID", whatever element holds it.
const shownIds = () => [...document.querySelectorAll(".key-row *")].filter((node) => node.children.length === 0 && /^ID /.test(node.textContent ?? "")).map((node) => node.textContent);
const descriptions = () => screen.getAllByRole("button", { name: /^Open / }).map((row) => document.getElementById(row.getAttribute("aria-describedby") ?? "")?.textContent ?? "");
/** What a screen reader hears about each row: its name, then its description. */
const spoken = () => screen.getAllByRole("button", { name: /^Open / }).map((row, index) => `${row.getAttribute("aria-label")} ${descriptions()[index]}`);
/** What each row draws, with the group heading that names its project. */
const drawings = () => [...document.querySelectorAll(".key-row")].map((row) => `${row.closest(".key-group")?.querySelector(".group-head")?.textContent ?? ""} ${row.textContent}`);

// Each drawn fact, and a key that differs from the base key in that fact alone.
const VARIATIONS: [string, Partial<KeyView>][] = [
  ["title", { title: "Other title" }],
  ["issuer", { issuer: "Acme" }],
  ["scope count", { scopes: ["read"] }],
  ["variable", { resolvedEnvVar: "OTHER_KEY" }],
  ["environment", { environment: "prod" }],
  ["project", { project: "other-project" }],
  ["renewal", { rotationDays: 90, secretChange: "known", secretChangedAt: inDays(-30), rotationDueAt: inDays(60) }],
  ["expiry", { expiresAt: inDays(200) }],
];
// The state is not in this table: it comes from the expiry and the renewal, which are.

for (const [fact, change] of VARIATIONS) {
  test(`two rows that differ only in ${fact} show no ID, and differ on screen and to a screen reader`, async () => {
    await showKeys([key({ id: "first-unique-id" }), key({ id: "second-unique-id", ...change })]);
    expect(shownIds()).toEqual([]);
    const drawn = drawings();
    expect(drawn).toHaveLength(2);
    expect(drawn[0]).not.toBe(drawn[1]);
    const [one, two] = spoken();
    expect(one).not.toBe(two);
  });
}

test("two rows that match in every drawn fact both show their short ID, and read it", async () => {
  await showKeys([key({ id: "first-unique-id" }), key({ id: "second-unique-id" })]);
  expect(shownIds().sort()).toEqual(["ID first-un", "ID second-u"]);
  const [one, two] = descriptions();
  expect(one).not.toBe(two);
  expect([one, two].join(" ")).toMatch(/ID first-un\..*ID second-u\.|ID second-u\..*ID first-un\./);
});

// An unset fact draws its fallback. A literal value that reads like the fallback draws the same,
// so the two rows are look-alikes and need their IDs (LOOK-R4-01).
const FALLBACKS: [string, keyof KeyView, string][] = [
  ["issuer", "issuer", "No issuer"],
  ["environment", "environment", "No environment"],
  ["project", "project", "No project"],
];

for (const [name, field, fallback] of FALLBACKS) {
  test(`an unset ${name} and the literal ${name} "${fallback}" draw the same, so both rows show their ID`, async () => {
    await showKeys([key({ id: "unset-value-id", [field]: null }), key({ id: "literal-value-id", [field]: fallback })]);
    const [one, two] = drawings().map((text) => text.replace(/ID \S+/, ""));
    expect(one).toBe(two);
    expect(shownIds().sort()).toEqual(["ID literal-", "ID unset-va"]);
  });
}

test("an unset project and the literal project \"No project\" share one shelf and one group heading", async () => {
  await showKeys([key({ id: "unset-value-id", project: null }), key({ id: "literal-value-id", project: "No project" })]);
  expect(screen.getAllByRole("button", { name: /^No project/ })).toHaveLength(1);
  expect(screen.getAllByRole("heading", { name: /^No project/ })).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: /^No project/ }));
  expect(screen.getAllByRole("button", { name: "Open Twin" })).toHaveLength(2);
});

test("a search for a fallback finds the rows that draw it", async () => {
  await showKeys([key({ id: "unset-value-id", title: "Unset", issuer: null }), key({ id: "set-value-id", title: "Set", issuer: "Acme" })]);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "no issuer" } });
  expect(screen.getAllByRole("button", { name: /^Open / }).map((row) => row.getAttribute("aria-label"))).toEqual(["Open Unset"]);
});

test("the key glyph is not a fact: rows with one glyph and rows with two glyphs both show IDs", async () => {
  const base = "fake-glyph-0";
  let twin = 1;
  while (cutsOf(`fake-glyph-${twin}`).join() !== cutsOf(base).join()) twin += 1;
  await showKeys([key({ id: base }), key({ id: `fake-glyph-${twin}` })]);
  expect(shownIds()).toHaveLength(2);
  cleanup();
  await showKeys([key({ id: "fake-glyph-a" }), key({ id: "fake-glyph-b" })]);
  expect(cutsOf("fake-glyph-a").join()).not.toBe(cutsOf("fake-glyph-b").join());
  expect(shownIds()).toHaveLength(2);
});

test("the delete confirmation of a look-alike key names its facts and its ID, and removes that key", async () => {
  await showKeys([key({ id: "first-unique-id" }), key({ id: "second-unique-id" })]);
  fireEvent.click(screen.getByRole("button", { name: "Open Twin", description: /ID second-u\./ }));
  const drawer = screen.getByRole("dialog", { name: "Twin" });
  expect(within(drawer).getByText("ID", { selector: ".fact-key" }).nextElementSibling?.textContent).toBe("second-u");
  fireEvent.click(within(drawer).getByRole("button", { name: "Move to Recycle Bin" }));
  const confirm = screen.getByRole("dialog", { name: "Move this key to the Recycle Bin?" });
  expect(confirm.getAttribute("aria-describedby")).toBeTruthy();
  const described = document.getElementById(confirm.getAttribute("aria-describedby") ?? "")?.textContent ?? "";
  for (const text of ["demo", "dev", "FAKE_KEY", "No issuer", "second-u"]) expect(described).toContain(text);
  expect(described).not.toContain("first-un");
  await act(async () => { fireEvent.click(within(confirm).getByRole("button", { name: "Move to Recycle Bin" })); });
  expect(native.sent.filter((call) => call.command === "key_remove").map((call) => call.id)).toEqual(["second-unique-id"]);
});

test("the delete confirmation of a key that looks like no other names its facts and no ID", async () => {
  await showKeys([key({ id: "only-unique-id", issuer: "Acme", scopes: ["read", "write"] })]);
  fireEvent.click(screen.getByRole("button", { name: "Open Twin" }));
  fireEvent.click(screen.getByRole("button", { name: "Move to Recycle Bin" }));
  const facts = screen.getByRole("dialog", { name: "Move this key to the Recycle Bin?" }).querySelector(".fact-list")?.textContent ?? "";
  for (const text of ["demo", "dev", "FAKE_KEY", "Acme", "2 scopes"]) expect(facts).toContain(text);
  expect(facts).not.toContain("ID");
});

test("the rotate dialog of a look-alike key names its facts and its ID", async () => {
  await showKeys([key({ id: "first-unique-id" }), key({ id: "second-unique-id" })]);
  fireEvent.click(screen.getByRole("button", { name: "Open Twin", description: /ID first-un\./ }));
  fireEvent.click(screen.getByRole("button", { name: "Rotate secret" }));
  const dialog = screen.getByRole("dialog", { name: "Rotate secret" });
  expect(dialog.querySelector(".fact-list")?.textContent).toContain("first-un");
  expect(dialog.querySelector(".fact-list")?.textContent).not.toContain("second-u");
});

function binned(id: string): RecycledKey {
  return { deletedAt: "2026-06-01T10:00:00Z", key: key({ id, issuer: "Acme" }) };
}

test("two Recycle Bin entries with one issuer that differ only by ID: each confirmation names its own ID, and the chosen one is deleted", async () => {
  serve([], [binned("bin-entry-aaaa-1"), binned("bin-entry-bbbb-2")]);
  await act(async () => { render(<RecycleBin />); });
  await act(async () => {});
  for (const [id, other] of [["bin-entry-aaaa-1", "bin-entry-bbbb-2"], ["bin-entry-bbbb-2", "bin-entry-aaaa-1"]]) {
    const button = screen.getByRole("button", { name: "Delete Twin for good", description: new RegExp(`ID ${id},`) });
    expect(screen.getByRole("button", { name: "Restore Twin", description: new RegExp(`ID ${id},`) })).toBeTruthy();
    fireEvent.click(button);
    const dialog = screen.getByRole("dialog", { name: "Delete this key for good?" });
    const described = document.getElementById(dialog.getAttribute("aria-describedby") ?? "")?.textContent ?? "";
    expect(described).toContain(id);
    expect(described).toContain("Acme");
    expect(described).not.toContain(other);
    if (id === "bin-entry-aaaa-1") fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  }
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Delete for good" })); });
  expect(native.sent.filter((call) => call.command === "key_purge").map((call) => call.id)).toEqual(["bin-entry-bbbb-2"]);
});
