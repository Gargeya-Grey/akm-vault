import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "../App";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "../sessionBoundary";
import type { KeyView, RecycledKey } from "../types";
import { KeysView } from "./KeysView";
import { RecycleBin } from "./RecycleBin";

// D2B-04 and its class (B, a stale snapshot): native code committed a change, and its reply was
// lost, or a retry answered that nothing changed. The page must read again, so that every row, the
// drawer and the notice show what the vault holds, not the data from before the request.

const vault = vi.hoisted(() => ({
  session: "",
  keys: [] as unknown[],
  recycled: [] as unknown[],
  /** The answer of each request that changes the vault, in order. */
  replies: [] as (() => unknown)[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => {
    switch (command) {
      case "keys_list": return vault.keys;
      case "recycled_list": return vault.recycled;
      case "vault_status": return { sessionId: vault.session, path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: vault.keys.length, vaultName: "Fake", needsUpgrade: false };
      case "vault_warnings": return { application: [], operations: [] };
      case "vault_activity": return;
    }
    const reply = vault.replies.shift();
    if (!reply) throw new Error(`unexpected command ${command}`);
    return reply();
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../theme", () => ({ useTheme: () => ({ theme: "light", toggle: () => {} }) }));

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-key-0001", title: "Original", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}
const ORIGINAL = key();
const RENAMED = key({ title: "Renamed" });
const binned = (item: KeyView): RecycledKey => ({ deletedAt: "2026-06-01T10:00:00Z", key: item });
/** Native code commits the change, and the reply is lost in transport. */
const lost = (commit: () => void) => () => { commit(); throw "Fake channel closed."; };

const flush = () => act(async () => {});
async function click(name: string, scope: HTMLElement = document.body) {
  await act(async () => { fireEvent.click(within(scope).getByRole("button", { name })); });
  await flush();
}
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });
const dialog = (name: string) => screen.getByRole("dialog", { name });
const row = (title: string) => screen.queryByRole("button", { name: `Open ${title}` });

const CASES: { name: string; view: () => React.ReactElement; keys?: KeyView[]; recycled?: RecycledKey[]; replies: (() => unknown)[]; steps: () => Promise<void>; shows: () => void }[] = [
  {
    name: "an edit whose reply was lost, retried and answered that nothing changed (the review's sequence)",
    view: () => <KeysView />,
    keys: [ORIGINAL],
    replies: [lost(() => { vault.keys = [RENAMED]; }), () => null],
    steps: async () => {
      await click("Open Original");
      await click("Edit");
      type("Title", "Renamed");
      await click("Save changes");
      await click("Save changes");
    },
    shows: () => {
      expect(row("Renamed")).not.toBeNull();
      expect(row("Original")).toBeNull();
      expect(dialog("Renamed")).toBeTruthy();
      const notice = screen.getByText(/nothing was saved/).textContent ?? "";
      expect(notice).toContain("“Renamed”");
      expect(notice).not.toContain("Original");
    },
  },
  {
    name: "an edit whose reply was lost, then cancelled",
    view: () => <KeysView />,
    keys: [ORIGINAL],
    replies: [lost(() => { vault.keys = [RENAMED]; })],
    steps: async () => {
      await click("Open Original");
      await click("Edit");
      type("Title", "Renamed");
      await click("Save changes");
      await click("Cancel");
    },
    shows: () => {
      expect(row("Renamed")).not.toBeNull();
      expect(row("Original")).toBeNull();
    },
  },
  {
    name: "an add whose reply was lost, then cancelled",
    view: () => <KeysView />,
    keys: [ORIGINAL],
    replies: [lost(() => { vault.keys = [ORIGINAL, key({ id: "fake-key-0002", title: "Added" })]; })],
    steps: async () => {
      await click("Add key");
      type("Title", "Added");
      type("Secret", "fake-secret-value");
      await click("Add key", dialog("Add a key"));
      await click("Cancel");
    },
    shows: () => expect(row("Added")).not.toBeNull(),
  },
  {
    name: "a rotation whose reply was lost, then cancelled",
    view: () => <KeysView />,
    keys: [key({ rotationDays: 30, rotationDueAt: "2020-01-01T00:00:00Z" })],
    replies: [lost(() => { vault.keys = [key({ rotationDays: 30, rotationDueAt: "2099-01-01T00:00:00Z" })]; })],
    steps: async () => {
      expect(row("Original")?.textContent).toContain("overdue");
      await click("Open Original");
      await click("Rotate secret");
      type("New secret", "fake-new-secret");
      await click("Rotate secret", dialog("Rotate secret"));
      await click("Cancel", dialog("Rotate secret"));
    },
    shows: () => expect(row("Original")?.textContent).not.toContain("overdue"),
  },
  {
    name: "a move to the Recycle Bin whose reply was lost",
    view: () => <KeysView />,
    keys: [ORIGINAL],
    replies: [lost(() => { vault.keys = []; })],
    steps: async () => {
      await click("Open Original");
      await click("Move to Recycle Bin");
      await click("Move to Recycle Bin");
    },
    shows: () => {
      expect(row("Original")).toBeNull();
      expect(screen.getByRole("heading", { name: "No keys yet." })).toBeTruthy();
    },
  },
  {
    name: "a restore whose reply was lost",
    view: () => <RecycleBin />,
    recycled: [binned(ORIGINAL)],
    replies: [lost(() => { vault.recycled = []; })],
    steps: () => click("Restore Original"),
    shows: () => expect(screen.getByRole("heading", { name: "The Recycle Bin is empty." })).toBeTruthy(),
  },
  {
    name: "a permanent delete whose reply was lost",
    view: () => <RecycleBin />,
    recycled: [binned(ORIGINAL)],
    replies: [lost(() => { vault.recycled = []; })],
    steps: async () => {
      await click("Delete Original for good");
      await click("Delete for good");
    },
    shows: () => expect(screen.getByRole("heading", { name: "The Recycle Bin is empty." })).toBeTruthy(),
  },
  {
    name: "a vault reload whose reply was lost",
    view: () => <App />,
    keys: [ORIGINAL],
    replies: [lost(() => { vault.keys = [RENAMED]; })],
    steps: async () => {
      await screen.findByRole("button", { name: "Open Original" });
      await click("Reload vault");
    },
    shows: () => {
      expect(row("Renamed")).not.toBeNull();
      expect(row("Original")).toBeNull();
    },
  },
];

beforeEach(() => { invalidateSession(); vault.session = crypto.randomUUID(); });
afterEach(() => {
  cleanup();
  invalidateSession();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
});

for (const { name, view, keys = [], recycled = [], replies, steps, shows } of CASES) {
  test(`after ${name}, the page shows what the vault holds`, async () => {
    Object.assign(vault, { keys, recycled, replies: [...replies] });
    await act(async () => { render(view()); });
    await flush();
    await steps();
    expect(vault.replies).toEqual([]);
    shows();
  });
}
