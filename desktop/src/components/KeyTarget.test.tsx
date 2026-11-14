import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { KeysView } from "./KeysView";
import { RecycleBin } from "./RecycleBin";
import { formatDate } from "../format";
import { invalidateSession } from "../sessionBoundary";
import type { KeyView, RecycledKey } from "../types";

// U2-R1-02: every dialog that acts on a key lists every fact of the key's row, expiry, renewal and
// ID included, from the row-facts function, and is described by them. Two keys that differ only in
// one fact get dialogs that differ in that fact, on screen and to a screen reader, and the action
// goes to the key that was chosen.

const native = vi.hoisted(() => ({ invoke: vi.fn(), sent: [] as { command: string; id?: unknown }[] }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const NOW = new Date("2026-06-01T12:00:00Z");
const inDays = (days: number) => new Date(NOW.getTime() + days * 86_400_000).toISOString();

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Twin", username: null, issuer: "Acme", project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

function serve(keys: KeyView[], recycled: RecycledKey[] = []) {
  native.sent.length = 0;
  native.invoke.mockImplementation(async (command: string, args?: { id?: unknown; draft?: object }) => {
    native.sent.push({ command, id: args?.id });
    const target = [...keys, ...recycled.map((entry) => entry.key)].find((each) => each.id === args?.id);
    if (command === "keys_list") return keys;
    if (command === "recycled_list") return recycled;
    if (command === "key_remove" || command === "key_purge") return undefined;
    if (command === "key_rotate") return { key: target, changedAt: NOW.toISOString(), previousChangedAt: null };
    if (command === "key_update") return { ...target, ...args?.draft };
    if (command === "key_restore") return target;
    if (command === "key_copy") return 30;
    if (command === "vault_status") return { sessionId: "fake", path: "C:/fake/a.kdbx", exists: true, unlocked: true, keyCount: keys.length, vaultName: "A", needsUpgrade: false };
    throw new Error(`unexpected command ${command}`);
  });
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
  invalidateSession();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  native.invoke.mockReset();
});

const described = (node: Element) => (node.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean).map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
const factOf = (scope: HTMLElement, label: string) => within(scope).getByText(label, { selector: ".fact-key" }).nextElementSibling?.textContent ?? "";

const ID1 = "0fa1c0de-0000-4000-8000-000000000001";
const ID2 = "0fa1c0de-0001-4000-8000-000000000019";

// Two keys that differ in one fact alone (and in their IDs, which every key has), the text that
// each row draws for that fact, and the label of the fact in a dialog.
const PAIRS: { fact: string; label: string; one: Partial<KeyView>; two: Partial<KeyView>; marks: [string, string] }[] = [
  { fact: "expiry", label: "Expiry", one: { expiresAt: "2026-12-17T00:00:00Z" }, two: { expiresAt: "2027-03-27T00:00:00Z" }, marks: [formatDate("2026-12-17T00:00:00Z"), formatDate("2027-03-27T00:00:00Z")] },
  {
    fact: "renewal", label: "Renewal",
    one: { rotationDays: 90, secretChange: "known", secretChangedAt: inDays(-30), rotationDueAt: inDays(60) },
    two: { rotationDays: 90, secretChange: "known", secretChangedAt: inDays(-70), rotationDueAt: inDays(20) },
    marks: ["rotate in 60 days", "rotate in 20 days"],
  },
  { fact: "ID", label: "ID", one: {}, two: {}, marks: ["ID 0fa1c0de-0000", "ID 0fa1c0de-0001"] },
];

interface Dialog {
  name: string;
  /** Opens the dialog from the row that draws `mark`, and returns it. */
  open: (mark: string) => Promise<HTMLElement>;
  confirm: (dialog: HTMLElement) => Promise<void>;
  command: string;
}

const rowWith = (mark: string) => screen.getAllByRole("button", { name: /^Open / }).find((row) => row.textContent?.includes(mark))!;
const drawer = async (mark: string) => {
  fireEvent.click(rowWith(mark));
  return screen.getByRole("dialog", { name: "Twin" });
};
const click = async (scope: HTMLElement, name: string) => { await act(async () => { fireEvent.click(within(scope).getByRole("button", { name })); }); };

const DIALOGS: Dialog[] = [
  {
    name: "the move to the Recycle Bin",
    open: async (mark) => { await click(await drawer(mark), "Move to Recycle Bin"); return screen.getByRole("dialog", { name: "Move this key to the Recycle Bin?" }); },
    confirm: (dialog) => click(dialog, "Move to Recycle Bin"),
    command: "key_remove",
  },
  {
    name: "the rotation",
    open: async (mark) => { await click(await drawer(mark), "Rotate secret"); return screen.getByRole("dialog", { name: "Rotate secret" }); },
    confirm: async (dialog) => {
      fireEvent.change(within(dialog).getByLabelText("New secret"), { target: { value: "fake-new-secret" } });
      await click(dialog, "Rotate secret");
    },
    command: "key_rotate",
  },
  {
    name: "the edit form",
    open: async (mark) => { await click(await drawer(mark), "Edit"); return screen.getByRole("dialog", { name: "Edit key" }); },
    confirm: (dialog) => click(dialog, "Save changes"),
    command: "key_update",
  },
  {
    name: "the key drawer",
    open: drawer,
    confirm: (dialog) => click(dialog, "Copy"),
    command: "key_copy",
  },
];

for (const dialog of DIALOGS) {
  for (const pair of PAIRS) {
    test(`${dialog.name} of two keys that differ only by ${pair.fact} lists that fact, differs on screen and to a screen reader, and acts on the chosen key`, async () => {
      const keys = [key({ id: ID1, ...pair.one }), key({ id: ID2, ...pair.two })];
      const seen: { fact: string; text: string; heard: string }[] = [];
      for (const [index, mark] of pair.marks.entries()) {
        serve(keys);
        await act(async () => { render(<KeysView />); });
        await act(async () => {});
        const shown = await dialog.open(mark);
        const heard = described(shown);
        const fact = factOf(shown, pair.label);
        expect(heard, "the dialog is described by its target").toContain(fact);
        for (const label of ["Project", "Environment", "Variable", "Issuer", "Renewal", "Expiry", "Status", "ID"]) expect(factOf(shown, label), label).not.toBe("");
        seen.push({ fact, text: shown.textContent ?? "", heard });
        if (index === 1) {
          await dialog.confirm(shown);
          expect(native.sent.filter((call) => call.command === dialog.command).map((call) => call.id)).toEqual([ID2]);
        }
        cleanup();
      }
      expect(seen[0].fact).not.toBe(seen[1].fact);
      expect(seen[0].text).not.toBe(seen[1].text);
      expect(seen[0].heard).not.toBe(seen[1].heard);
    });
  }
}

const binned = (overrides: Partial<KeyView>): RecycledKey => ({ deletedAt: null, key: key(overrides) });
const binRowWith = (mark: string) => [...document.querySelectorAll<HTMLElement>(".bin-row")].find((row) => row.textContent?.includes(mark))!;
const BIN_MARKS: Record<string, [string, string]> = { expiry: [formatDate("2026-12-17T00:00:00Z"), formatDate("2027-03-27T00:00:00Z")], renewal: ["rotate in 60 days", "rotate in 20 days"], ID: [`ID ${ID1}`, `ID ${ID2}`] };

for (const pair of PAIRS) {
  test(`the permanent-delete confirmation of two recycled keys that differ only by ${pair.fact} lists that fact, and deletes the chosen key`, async () => {
    const recycled = [binned({ id: ID1, ...pair.one }), binned({ id: ID2, ...pair.two })];
    const seen: { fact: string; heard: string }[] = [];
    for (const [index, mark] of BIN_MARKS[pair.fact].entries()) {
      serve([], recycled);
      await act(async () => { render(<RecycleBin />); });
      await act(async () => {});
      fireEvent.click(within(binRowWith(mark)).getByRole("button", { name: "Delete Twin for good" }));
      const shown = screen.getByRole("dialog", { name: "Delete this key for good?" });
      const fact = factOf(shown, pair.label);
      const heard = described(shown);
      expect(heard).toContain(fact);
      seen.push({ fact, heard });
      if (index === 1) {
        await click(shown, "Delete for good");
        expect(native.sent.filter((call) => call.command === "key_purge").map((call) => call.id)).toEqual([ID2]);
      }
      cleanup();
    }
    expect(seen[0].fact).not.toBe(seen[1].fact);
    expect(seen[0].heard).not.toBe(seen[1].heard);
  });

  test(`the restore buttons of two recycled keys that differ only by ${pair.fact} read that fact, and restore the chosen key`, async () => {
    serve([], [binned({ id: ID1, ...pair.one }), binned({ id: ID2, ...pair.two })]);
    await act(async () => { render(<RecycleBin />); });
    await act(async () => {});
    const [one, two] = BIN_MARKS[pair.fact].map((mark) => within(binRowWith(mark)).getByRole("button", { name: "Restore Twin" }));
    expect(described(one)).not.toBe(described(two));
    for (const [button, mark] of [[one, BIN_MARKS[pair.fact][0]], [two, BIN_MARKS[pair.fact][1]]] as const) expect(described(button)).toContain(mark.replace(/^ID /, ""));
    await click(binRowWith(BIN_MARKS[pair.fact][1]), "Restore Twin");
    expect(native.sent.filter((call) => call.command === "key_restore").map((call) => call.id)).toEqual([ID2]);
  });
}
