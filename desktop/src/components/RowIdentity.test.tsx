import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { Activity } from "./Activity";
import { KeysView } from "./KeysView";
import { RecycleBin } from "./RecycleBin";
import { cutsOf } from "./glyphs";
import { invalidateSession } from "../sessionBoundary";
import type { KeyView, RecycledKey } from "../types";

// Fault class M (two different things drawn the same, U2-R1-01) and class H. Every key row, every
// place that names a key, and its accessible description show the key's short ID: the shortest
// prefix of 8 or more characters that no other key the page knows starts with. One function builds
// a row's facts (rowFacts.ts). A value draws so that it cannot imitate the structure of a row.

const native = vi.hoisted(() => ({ invoke: vi.fn(), sent: [] as { command: string; args?: Record<string, unknown> }[] }));
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

interface Served { keys?: KeyView[]; recycled?: RecycledKey[]; journal?: { keyTitle: string | null; keyId: string | null; project?: string | null; environment?: string | null }[] }

function serve({ keys = [], recycled = [], journal = [] }: Served) {
  native.sent.length = 0;
  native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    native.sent.push({ command, args });
    if (command === "keys_list") return keys;
    if (command === "recycled_list") return recycled;
    if (command === "audit_read") {
      const entries = journal.map((entry, index) => ({ seq: String(index + 1), ts: "2026-06-01T10:00:00Z", action: "add-completed", detail: null, project: null, environment: null, ...entry }));
      return { path: "C:/fake/journal/j-0.jsonl", entries, state: "consistent", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: entries.length };
    }
    if (command === "key_update") return { ...keys.find((each) => each.id === args?.id)!, ...(args?.draft as object) };
    if (command === "vault_status") return { sessionId: "fake", path: "C:/fake/a.kdbx", exists: true, unlocked: true, keyCount: keys.length, vaultName: "A", needsUpgrade: false };
    throw new Error(`unexpected command ${command}`);
  });
}

async function show(view: React.ReactElement, served: Served) {
  serve(served);
  await act(async () => { render(view); });
  await act(async () => {});
}

const showKeys = (keys: KeyView[]) => show(<KeysView />, { keys });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
  // A new session: the page knows no key IDs yet.
  invalidateSession();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  native.invoke.mockReset();
});

const rows = () => screen.getAllByRole("button", { name: /^Open / });
const descriptionOf = (node: Element) => (node.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
/** What a screen reader hears about a row: its name, then its description. */
const spoken = (row: Element) => `${row.getAttribute("aria-label")} | ${descriptionOf(row)}`;
/** The short ID that a row draws, as the text on screen. */
const idOf = (row: Element) => row.querySelector(".key-id")?.textContent ?? null;

// Fake IDs whose key glyphs are the same (`cutsOf`), so only the drawn ID can tell the rows apart.
const ID = {
  a1: "7b3e9f20-0000-4000-8000-000000000085", a2: "7b3e9f21-0000-4000-8000-000000000066",
  b1: "9e8d7c6b-0000-4000-8000-000000000005", b2: "5a6b7c8d-0000-4000-8000-000000000027",
  c1: "2c3d4e5f-0000-4000-8000-000000000001", c2: "1a2b3c4d-0000-4000-8000-000000000085",
  d1: "3f4e5d6c-0000-4000-8000-000000000001", d2: "6d5c4b3a-0000-4000-8000-000000000027",
  e1: "0fa1c0de-0000-4000-8000-000000000001", e2: "0fa1c0de-0001-4000-8000-000000000019",
  f1: "c4d2e8a1-0000-4000-8000-000000000087", f2: "c4d2e8a1-1000-4000-8000-000000000002",
};

const PAIRS: [string, Partial<KeyView>, Partial<KeyView>, [string, string]][] = [
  ["issuers that differ by a repeated space (the review's first pair)", { id: ID.a1, issuer: "Acme  Cloud" }, { id: ID.a2, issuer: "Acme Cloud" }, ["7b3e9f20", "7b3e9f21"]],
  ["an issuer that spells a scope count, and a real scope (the review's second pair)", { id: ID.b1, issuer: "Acme · 1 scope" }, { id: ID.b2, issuer: "Acme", scopes: ["read"] }, ["9e8d7c6b", "5a6b7c8d"]],
  ["titles that differ by a look-alike letter", { id: ID.c1, title: "Acme key" }, { id: ID.c2, title: "\u0410cme key" }, ["2c3d4e5f", "1a2b3c4d"]],
  ["titles that differ by a zero-width space", { id: ID.d1, title: "Twin" }, { id: ID.d2, title: "Twin\u200B" }, ["3f4e5d6c", "6d5c4b3a"]],
  ["identical facts", { id: ID.e1 }, { id: ID.e2 }, ["0fa1c0de-0000", "0fa1c0de-0001"]],
  ["IDs that share their first 8 characters", { id: ID.f1 }, { id: ID.f2 }, ["c4d2e8a1-0", "c4d2e8a1-1"]],
];

test("the fixture pairs draw the same key glyph", () => {
  for (const [, one, two] of PAIRS) expect(cutsOf(one.id!).join(), `${one.id} ${two.id}`).toBe(cutsOf(two.id!).join());
});

for (const [name, one, two, [short1, short2]] of PAIRS) {
  test(`two keys with ${name}: each row draws its own short ID, and the rows differ on screen and to a screen reader`, async () => {
    await showKeys([key(one), key(two)]);
    expect(rows().map(idOf).sort()).toEqual([`ID ${short1}`, `ID ${short2}`].sort());
    const [first, second] = [short1, short2].map((short) => rows().find((row) => idOf(row) === `ID ${short}`)!);
    expect(first.textContent).not.toBe(second.textContent);
    expect(spoken(first)).not.toBe(spoken(second));
    expect(descriptionOf(first)).toContain(`, ID ${short1}.`);
    expect(descriptionOf(second)).toContain(`, ID ${short2}.`);
  });
}

test("a key that looks like no other still draws its short ID, and reads it", async () => {
  await showKeys([key({ id: ID.a1, issuer: "Acme", scopes: ["read", "write"] })]);
  const [row] = rows();
  expect(idOf(row)).toBe("ID 7b3e9f20");
  expect(descriptionOf(row)).toMatch(/^Project demo, environment dev, variable FAKE_KEY, issuer Acme, 2 scopes, ID 7b3e9f20\. /);
});

test("a value draws its invisible, directional and repeated-space characters as code points, and the row structure stays its own", async () => {
  await showKeys([
    key({ id: ID.a1, title: "Twin\u200B", issuer: "Acme  Cloud " }),
    key({ id: ID.b1, title: "Line\nbreak", issuer: "Acme · 1 scope", project: "\u202Edemo" }),
  ]);
  const twin = screen.getByRole("button", { name: "Open Twin⟨U+200B⟩" });
  expect(twin.querySelector(".key-title")?.textContent).toBe("Twin⟨U+200B⟩");
  expect(twin.querySelector(".key-sub")?.textContent).toBe("Acme ⟨U+0020⟩Cloud⟨U+0020⟩");
  const line = screen.getByRole("button", { name: "Open Line⟨U+000A⟩break" });
  expect(line.querySelector(".key-sub")?.textContent).toBe("Acme ⟨U+00B7⟩ 1 scope");
  expect(screen.getByRole("heading", { name: "Project ⟨U+202E⟩demo" })).toBeTruthy();
});

test("the edit form and the search use the stored value, not the drawn one", async () => {
  await showKeys([key({ id: ID.a1, title: "Spaced", issuer: "Acme  Cloud" }), key({ id: ID.a2, title: "Plain", issuer: "Acme Cloud" })]);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "acme  cloud" } });
  expect(rows().map((row) => row.getAttribute("aria-label"))).toEqual(["Open Spaced"]);
  fireEvent.click(rows()[0]);
  fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  expect((screen.getByLabelText("Issuer") as HTMLInputElement).value).toBe("Acme  Cloud");
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Save changes" })); });
  const update = native.sent.find((call) => call.command === "key_update");
  expect(update?.args?.id).toBe(ID.a1);
  expect((update?.args?.draft as { issuer: string }).issuer).toBe("Acme  Cloud");
});

test("the key drawer draws its title, issuer and details by the rule, and its notes keep their lines", async () => {
  await showKeys([key({ id: ID.a1, title: "Twin\u200B", issuer: "Acme  Cloud", url: "https://example.invalid/\u202Efdp", tags: ["tag "], scopes: ["read\u200B"], notes: "line one\nline\u200Btwo" })]);
  fireEvent.click(rows()[0]);
  const drawer = screen.getByRole("dialog", { name: "Twin⟨U+200B⟩" });
  expect(drawer.querySelector(".drawer-ident .label")?.textContent).toBe("Acme ⟨U+0020⟩Cloud");
  const detail = (label: string) => within(drawer).getByText(label, { selector: ".fact-key" }).nextElementSibling?.textContent;
  expect(detail("URL")).toBe("https://example.invalid/⟨U+202E⟩fdp");
  expect(detail("Tags")).toBe("tag⟨U+0020⟩");
  expect(detail("Scope names")).toBe("read⟨U+200B⟩");
  expect(detail("ID")).toBe("7b3e9f20");
  const notes = drawer.querySelector(".detail-notes p")!;
  expect(notes.querySelectorAll("br")).toHaveLength(1);
  expect(notes.textContent).toBe("line oneline⟨U+200B⟩two");
});

test("a recycled key that the page knows makes a live key's short ID longer", async () => {
  await show(<RecycleBin />, { recycled: [{ deletedAt: "2026-06-01T10:00:00Z", key: key({ id: ID.f2 }) }] });
  expect(screen.getByText(`ID ${ID.f2}`)).toBeTruthy();
  cleanup();
  await showKeys([key({ id: ID.f1 })]);
  expect(idOf(rows()[0])).toBe("ID c4d2e8a1-0");
});

test("a new session forgets the IDs of the last one", async () => {
  await show(<RecycleBin />, { recycled: [{ deletedAt: "2026-06-01T10:00:00Z", key: key({ id: ID.f2 }) }] });
  cleanup();
  invalidateSession();
  await showKeys([key({ id: ID.f1 })]);
  expect(idOf(rows()[0])).toBe("ID c4d2e8a1");
});

test("a short ID grows when the list gains a key that shares its start", async () => {
  await showKeys([key({ id: ID.f1 })]);
  expect(idOf(rows()[0])).toBe("ID c4d2e8a1");
  cleanup();
  await showKeys([key({ id: ID.f1 }), key({ id: ID.f2, title: "Other" })]);
  expect(idOf(screen.getByRole("button", { name: "Open Twin" }))).toBe("ID c4d2e8a1-0");
});

test("Activity names each key with its short ID, and draws the values of a record", async () => {
  await show(<Activity />, { journal: [
    { keyTitle: "Twin", keyId: ID.e1, project: "demo", environment: null },
    { keyTitle: "Twin", keyId: ID.e2, project: "demo\u200B", environment: null },
    { keyTitle: null, keyId: ID.f1 },
    { keyTitle: null, keyId: null },
  ] });
  const keys = [...document.querySelectorAll(".audit-key")].map((node) => node.textContent);
  // Newest first.
  expect(keys).toEqual([
    "Vault—",
    "KeyID c4d2e8a1—",
    "TwinID 0fa1c0de-0001demo⟨U+200B⟩",
    "TwinID 0fa1c0de-0000demo",
  ]);
});

test("the unlock summary names each key with its short ID", async () => {
  await showKeys([key({ id: ID.e1, expiresAt: inDays(-2) }), key({ id: ID.e2, expiresAt: inDays(-2) })]);
  const summary = screen.getByRole("region", { name: "Keys needing attention" });
  expect(within(summary).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["Expired (2): “Twin” (ID 0fa1c0de-0000), “Twin” (ID 0fa1c0de-0001)"]);
});

test("an environment option draws its value, and the filter keeps the stored value apart", async () => {
  await showKeys([key({ id: ID.a1, title: "Spaced", environment: "dev " }), key({ id: ID.a2, title: "Plain", environment: "dev" })]);
  const options = [...screen.getByLabelText("Environment").querySelectorAll("option")].map((option) => option.textContent);
  expect(options).toEqual(["All environments", "dev", "dev⟨U+0020⟩"]);
});

for (const shelf of ["All keys", "Needs attention"]) {
  test(`a project named "${shelf}" has a page title and a shelf button that differ from the "${shelf}" shelf, on screen and by name`, async () => {
    await showKeys([key({ id: ID.a1, title: "Named", project: shelf, expiresAt: inDays(-2) }), key({ id: ID.a2, title: "Other", project: "demo" })]);
    const nav = screen.getByRole("navigation", { name: "Projects" });
    fireEvent.click(shelf === "All keys" ? within(nav).getAllByRole("button")[0] : nav.querySelector(".shelf-attention")!);
    const fixed = screen.getByRole("heading", { level: 1 });
    expect(fixed.textContent).toMatch(new RegExp(`^${shelf} \\d+$`));
    const fixedName = `${fixed.textContent}`;
    fireEvent.click(within(nav).getByRole("button", { name: `Project ${shelf}` }));
    const title = screen.getByRole("heading", { level: 1 });
    expect(title.textContent).toBe(`Project ${shelf} 1`);
    expect(screen.getByRole("heading", { level: 1, name: `Project ${shelf} 1` })).toBe(title);
    expect(title.textContent).not.toBe(fixedName);
    expect(within(nav).queryAllByRole("button", { name: shelf })).toHaveLength(shelf === "All keys" ? 1 : 0);
  });
}

test("an unset project and the literal project \"No project\" share one shelf and one group heading, and their rows differ by ID", async () => {
  await showKeys([key({ id: ID.e1, project: null }), key({ id: ID.e2, project: "No project" })]);
  expect(screen.getAllByRole("button", { name: /^No project/ })).toHaveLength(1);
  expect(screen.getAllByRole("heading", { name: /^No project/ })).toHaveLength(1);
  const [one, two] = rows();
  expect(idOf(one)).not.toBe(idOf(two));
  expect(spoken(one)).not.toBe(spoken(two));
});

test("Recycle Bin rows keep the full ID, and each button reads it", async () => {
  await show(<RecycleBin />, { recycled: [{ deletedAt: "2026-06-01T10:00:00Z", key: key({ id: ID.e1 }) }, { deletedAt: "2026-06-01T10:00:00Z", key: key({ id: ID.e2 }) }] });
  for (const id of [ID.e1, ID.e2]) {
    expect(screen.getByText(`ID ${id}`)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Restore Twin", description: new RegExp(`ID ${id},`) })).toBeTruthy();
  }
});
