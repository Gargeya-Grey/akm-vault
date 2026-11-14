import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { ReactElement } from "react";
import { Activity } from "./components/Activity";
import { KeysView } from "./components/KeysView";
import { RecycleBin } from "./components/RecycleBin";
import { Settings } from "./components/Settings";
import { UseKeys } from "./components/UseKeys";
import { beginOutcome, invalidateSession, sessionGeneration, settleOutcome } from "./sessionBoundary";
import { plaintextScope } from "./plaintext";
import { seal, stampFor } from "./sealed";
import { held, type Held } from "./testing/held";
import type { KeyView } from "./types";

// Fault classes K (a late result replaces a newer one) and L (a failed read shown as empty), in
// every view that reads native state. The reads are held, so each test orders the replies.

const api = vi.hoisted(() => ({
  keys: vi.fn(), recycled: vi.fn(), audit: vi.fn(), backup: vi.fn(),
  importEnv: vi.fn(), history: vi.fn(),
}));
// The views read through native.ts; the import goes through api.ts. One mock serves both.
vi.mock("./native", () => ({ reads: api }));
vi.mock("./api", () => ({
  api,
  errorText: (error: unknown) => (error && typeof error === "object" && "message" in error ? String((error as { message: unknown }).message) : String(error)),
}));

function key(overrides: Partial<KeyView> = {}): KeyView {
  return {
    id: "fake-id", title: "Fake key", username: null, issuer: null, project: "demo", environment: "dev",
    scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
    secretChangedAt: null, secretChange: "unknown", historyCount: 0, ...overrides,
  };
}

const status = { sessionId: "fake-session", path: "C:/fake/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Fake", needsUpgrade: false };
const journal = { path: "C:/fake/journal/j-0.jsonl", entries: [], state: "empty", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: 0 };

/** Each call of a mocked read returns a new held reply, in call order. */
function holdReads(read: ReturnType<typeof vi.fn>) {
  const calls: Held<unknown>[] = [];
  read.mockImplementation(() => { const reply = held<unknown>(); calls.push(reply); return reply.promise; });
  return calls;
}

beforeEach(() => {
  vi.clearAllMocks();
  api.keys.mockResolvedValue([]);
  api.recycled.mockResolvedValue([]);
  api.audit.mockResolvedValue(journal);
  api.backup.mockResolvedValue({ exists: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const text = () => document.body.textContent ?? "";

// Every view, its read, the text it shows when it read an empty list, and its failure heading.
const VIEWS: { name: string; view: () => ReactElement; read: ReturnType<typeof vi.fn>; empty: RegExp; failed: string }[] = [
  { name: "Keys", view: () => <KeysView />, read: api.keys, empty: /No keys yet|All keys\s*0|need attention|in the vault, none shown/, failed: "AKM could not read the keys." },
  { name: "Use keys", view: () => <UseKeys />, read: api.keys, empty: /key[s]? selected in|Choose a project and environment to enable export/, failed: "AKM could not read the keys." },
  { name: "Recycle Bin", view: () => <RecycleBin />, read: api.recycled, empty: /The Recycle Bin is empty/, failed: "AKM could not read the Recycle Bin." },
  { name: "Activity", view: () => <Activity />, read: api.audit, empty: /No records displayed|No records available/, failed: "AKM could not read the journal." },
  { name: "Settings", view: () => <Settings status={status} onLocked={() => {}} />, read: api.keys, empty: /\d+ in this vault/, failed: "AKM could not count the keys." },
];

for (const { name, view, read, empty, failed } of VIEWS) {
  test(`${name}: while the first read is in flight, the view shows no empty state and no count`, async () => {
    holdReads(read);
    render(view());
    await act(async () => {});
    expect(text()).not.toMatch(empty);
    expect(text()).not.toContain(failed);
  });

  test(`${name}: a first read that fails says so and never says that the list is empty`, async () => {
    read.mockRejectedValueOnce({ message: "fake read failure" });
    render(view());
    await act(async () => {});
    expect(text()).toContain(failed);
    expect(text()).toContain("fake read failure");
    expect(text()).not.toMatch(empty);
  });

  test(`${name}: a later success after a failure clears the error`, async () => {
    read.mockRejectedValueOnce({ message: "fake read failure" });
    render(view());
    await act(async () => {});
    const retry = screen.queryByRole("button", { name: "Try again" }) ?? screen.getByRole("button", { name: "Refresh" });
    await act(async () => { fireEvent.click(retry); });
    await act(async () => {});
    expect(text()).not.toContain(failed);
    expect(text()).not.toContain("fake read failure");
  });

  test(`${name}: a reply from an earlier session never shows`, async () => {
    const calls = holdReads(read);
    render(view());
    await act(async () => {});
    invalidateSession();
    await act(async () => { for (const call of calls) call.reject({ message: "fake old session failure" }); });
    expect(text()).not.toContain("fake old session failure");
    expect(text()).not.toContain(failed);
  });
}

// LOOK-R4-02: Use keys had no request identity on its list read.
function importOnce() {
  // As native.ts does: the import has a receipt, and its reply settles it (U2-R1-03).
  api.importEnv.mockImplementationOnce(() => {
    const receipt = beginOutcome("import", sessionGeneration());
    const reply = Promise.resolve({ added: 1, updated: 0 }).then((report) => { settleOutcome(receipt, "completed"); return report; });
    return Object.assign(reply, { receipt });
  });
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
}
const projectOptions = () => [...(screen.getByLabelText("Project") as HTMLSelectElement).options].map((option) => option.value).filter(Boolean);

for (const older of ["succeeds", "fails"] as const) {
  test(`Use keys: a retry that ${older} after the newer post-import read changes nothing`, async () => {
    const calls = holdReads(api.keys);
    render(<UseKeys />);
    await act(async () => calls[0].reject({ message: "fake read failure" }));
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
    expect(calls).toHaveLength(2);
    importOnce();
    await act(async () => {});
    expect(calls).toHaveLength(3);
    await act(async () => calls[2].resolve([key({ project: "newer" })]));
    expect(projectOptions()).toEqual(["newer"]);
    await act(async () => (older === "succeeds" ? calls[1].resolve([key({ project: "older" })]) : calls[1].reject({ message: "fake older failure" })));
    expect(projectOptions()).toEqual(["newer"]);
    expect(text()).not.toContain("fake older failure");
    expect(text()).not.toContain("AKM could not read the keys.");
  });
}

test("Use keys: while a retry is in flight, its control is disabled and says that it is busy", async () => {
  const calls = holdReads(api.keys);
  render(<UseKeys />);
  await act(async () => calls[0].reject({ message: "fake read failure" }));
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
  const retry = screen.getByRole("button", { name: "Trying again…" }) as HTMLButtonElement;
  expect(retry.disabled).toBe(true);
  expect(retry.getAttribute("aria-busy")).toBe("true");
  fireEvent.click(retry);
  expect(calls).toHaveLength(2);
});

test("Use keys: a scope chosen before a refresh that removes it no longer enables export", async () => {
  const calls = holdReads(api.keys);
  render(<UseKeys />);
  await act(async () => calls[0].resolve([key({ project: "demo", environment: "dev" })]));
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  expect((screen.getByRole("button", { name: "Preview" }) as HTMLButtonElement).disabled).toBe(false);
  importOnce();
  await act(async () => {});
  expect(calls).toHaveLength(2);
  await act(async () => calls[1].resolve([key({ project: "other", environment: "dev" })]));
  expect((screen.getByRole("button", { name: "Preview" }) as HTMLButtonElement).disabled).toBe(true);
});

test("Use keys: a preview stays hidden once a refresh removes its scope", async () => {
  const calls = holdReads(api.keys);
  // As native.ts does: the preview arrives sealed under the scope of its request (U2-R1-04).
  (api as Record<string, ReturnType<typeof vi.fn>>).exportEnv = vi.fn(async (project: string, environment: string) => seal("FAKE_KEY=fake-preview-value", stampFor(plaintextScope.export(project, environment))));
  render(<UseKeys />);
  await act(async () => calls[0].resolve([key({ project: "demo", environment: "dev" })]));
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Preview" })); });
  expect(text()).toContain("fake-preview-value");
  importOnce();
  await act(async () => {});
  await act(async () => calls[1].resolve([key({ project: "other", environment: "dev" })]));
  expect(text()).not.toContain("fake-preview-value");
});

test("Use keys: a list reply that arrives after the view closed renders nothing and raises nothing", async () => {
  const calls = holdReads(api.keys);
  const errors = vi.spyOn(console, "error");
  const view = render(<UseKeys />);
  view.unmount();
  await act(async () => calls[0].resolve([key()]));
  expect(errors).not.toHaveBeenCalled();
  expect(calls).toHaveLength(1);
});

test("Activity: a refresh in flight disables Refresh, and a failed refresh drops the old records", async () => {
  const calls = holdReads(api.audit);
  render(<Activity />);
  await act(async () => calls[0].resolve(journal));
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Refresh" })); });
  const refresh = screen.getByRole("button", { name: "Reading…" }) as HTMLButtonElement;
  expect(refresh.disabled).toBe(true);
  await act(async () => calls[1].reject({ message: "fake refresh failure" }));
  expect(text()).toContain("AKM could not read the journal.");
  expect(text()).not.toMatch(/No records displayed|No records available/);
});

test("Activity: a journal recovery that ends after the view closed starts no read", async () => {
  const recovery = held<unknown>();
  api.audit.mockResolvedValue({ ...journal, state: "corrupt", problemLine: 1, damage: "malformedJson" });
  (api as Record<string, ReturnType<typeof vi.fn>>).recoverJournal = vi.fn(() => recovery.promise);
  const view = render(<Activity />);
  await act(async () => {});
  fireEvent.click(screen.getByRole("button", { name: "Start a new journal…" }));
  view.unmount();
  await act(async () => recovery.resolve(null));
  expect(api.audit).toHaveBeenCalledTimes(1);
});

test("Recycle Bin: a failed first read offers a retry, and a retry that succeeds shows the entries", async () => {
  const calls = holdReads(api.recycled);
  render(<RecycleBin />);
  await act(async () => calls[0].reject({ message: "fake read failure" }));
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
  await act(async () => calls[1].resolve([{ deletedAt: "2026-06-01T12:00:00Z", key: key({ title: "Binned" }) }]));
  expect(screen.getByRole("button", { name: "Restore Binned" })).toBeTruthy();
  expect(text()).not.toContain("fake read failure");
});
