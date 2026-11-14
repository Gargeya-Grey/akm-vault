import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { reads } from "./native";
import { retainedApplicationWarnings, retainedOperationNotices } from "./operationNotices";
import { acknowledgeCompletions, beginOutcome, retainedCompletions, sessionGeneration } from "./sessionBoundary";

const native = vi.hoisted(() => ({ active: true, owner: "", path: "C:/fake/a.kdbx", onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, callback: typeof native.onLock) => { if (name !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked region</p> }));
beforeEach(() => { native.owner = crypto.randomUUID(); });
afterEach(async () => {
  cleanup();
  native.invoke.mockResolvedValue(undefined);
  await api.dismissWarnings([...retainedOperationNotices(), ...retainedApplicationWarnings()].map(({ id }) => id));
});

for (const delayed of [false, true]) {
  test(`A's ${delayed ? "delayed" : "undrained"} warning acknowledges its saved operation without being attributed to active B`, async () => {
    native.active = true;
    native.path = "C:/fake/a.kdbx";
    let resolveWarning!: (value: unknown) => void;
    const report = { application: [], operations: [{ id: `notice-a-${delayed}`, originSession: "native-a", kind: "journal" }] };
    const status = () => ({ sessionId: native.active ? native.owner : null, path: native.path, exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false });
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return status();
      if (command === "vault_warnings") return delayed ? new Promise((resolve) => { resolveWarning = resolve; }) : report;
      if (command === "vault_prepare_open") { native.owner = crypto.randomUUID(); return native.owner; }
      if (command === "vault_unlock") { native.active = true; native.path = "C:/fake/b.kdbx"; return status(); }
      if (command === "vault_activity") return;
      throw new Error(`Unexpected command ${command}`);
    });
    render(<App />);
    await screen.findByText("Unlocked region");
    const oldReply = delayed ? reads.warnings().catch((error: unknown) => error) : undefined;
    act(() => { native.active = false; native.onLock?.({ payload: { sessionId: native.owner } }); });
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-for-b" } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
    await screen.findByText("Unlocked region");
    await act(async () => { if (delayed) { resolveWarning(report); await oldReply; } else { await reads.warnings(); } });
    expect(screen.getByRole("region", { name: "Saved-operation notices" }).textContent).toContain("A vault change was saved");
    expect(screen.getByText("Unlocked region")).toBeTruthy();
    expect(native.active).toBe(true);
    expect(native.path).toBe("C:/fake/b.kdbx");
    expect(document.body.textContent).not.toContain("C:/fake/a.kdbx");
    act(() => native.onLock?.({ payload: { sessionId: native.owner } }));
    expect(screen.getByRole("region", { name: "Saved-operation notices" })).toBeTruthy();
  });
}

test("a delayed notice arriving between root mounts is replayed without locking B", async () => {
  let deliver!: (report: unknown) => void;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  const first = render(<App />);
  await screen.findByText("Unlocked region");
  native.invoke.mockImplementationOnce(() => new Promise((done) => { deliver = done; }));
  const pending = reads.warnings();
  first.unmount();
  await act(async () => {
    deliver({ application: [], operations: [{ id: "between-mounts", originSession: "A", kind: "journal" }] });
    await pending;
  });
  render(<App />);
  await screen.findByText("Unlocked region");
  expect(screen.getByRole("region", { name: "Saved-operation notices" }).textContent).toContain("could not be confirmed");
  expect(screen.getByText("Unlocked region")).toBeTruthy();
});

test("a one-shot application safety warning delivered between roots remains until explicit dismissal", async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity" || command === "vault_dismiss_warnings") return;
    throw new Error(command);
  });
  const a = render(<App />);
  await screen.findByText("Unlocked region");
  let deliver!: (report: unknown) => void;
  native.invoke.mockImplementationOnce(() => new Promise((done) => { deliver = done; }));
  const pending = reads.warnings();
  a.unmount();
  await act(async () => { deliver({ application: [{ id: "app-monitor-warning", message: "Fake Windows monitor safety warning" }], operations: [] }); await pending; });
  render(<App />);
  await screen.findByText("Unlocked region");
  expect(screen.getByText("Fake Windows monitor safety warning")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss warnings" }));
  await act(async () => {});
  cleanup();
  render(<App />);
  await screen.findByText("Unlocked region");
  expect(screen.queryByText("Fake Windows monitor safety warning")).toBeNull();
});

test("application-warning dismissal retains new recurrence IDs and rejects stale poll replay", async () => {
  const first = { id: "app-old-id", message: "Fake recurring monitor warning" };
  const recurrence = { id: "app-new-id", message: first.message };
  let application = [first];
  let failDismiss = true;
  native.invoke.mockImplementation(async (command: string, args?: { ids: string[] }) => {
    if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
    if (command === "vault_warnings") return { application, operations: [] };
    if (command === "vault_dismiss_warnings") {
      expect(args?.ids).toEqual([first.id]);
      if (failDismiss) throw new Error("fake failed acknowledgment");
      application = [recurrence];
      return;
    }
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  render(<App />);
  await screen.findByText(first.message);
  fireEvent.click(screen.getByRole("button", { name: "Dismiss warnings" }));
  await screen.findByText(/Could not dismiss these warnings/);
  let deliver!: (report: unknown) => void;
  native.invoke.mockImplementationOnce(() => new Promise((done) => { deliver = done; }));
  const oldPoll = reads.warnings();
  failDismiss = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dismiss warnings" })); });
  await act(async () => { deliver({ application: [first], operations: [] }); await oldPoll; });
  expect(screen.queryByText(first.message)).toBeNull();
  await act(() => reads.warnings());
  expect(screen.getByText(first.message)).toBeTruthy();
  expect(retainedApplicationWarnings()).toEqual([recurrence]);
  expect(screen.getByText("Unlocked region")).toBeTruthy();
});

for (const kinds of [["backupRefresh"], ["journal"], ["backupRefresh", "journal"], ["backupPrune"], ["backupRetention"], ["backupRetention", "backupRefresh", "journal"]]) {
  test(`fixed notices distinguish ${kinds.join(" and ")} without claiming a known missing record`, async () => {
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
      if (command === "vault_warnings") return { application: [], operations: kinds.map((kind) => ({ id: `fixed-${kinds.join("-")}-${kind}`, originSession: "A", kind })) };
      if (command === "vault_activity") return;
      throw new Error(command);
    });
    render(<App />);
    await screen.findByText("Unlocked region");
    await act(() => reads.warnings());
    const notice = screen.getByRole("region", { name: "Saved-operation notices" }).textContent!;
    expect(notice.includes("activity entry could not be confirmed")).toBe(kinds.includes("journal"));
    expect(notice.includes("backup could not be refreshed")).toBe(kinds.includes("backupRefresh"));
    expect(notice.includes("old backups could not be pruned")).toBe(kinds.includes("backupPrune"));
    expect(notice.includes("retention of its pre-change encrypted backup could not be confirmed")).toBe(kinds.includes("backupRetention"));
    expect(notice).not.toContain("saved with a retained");
    expect(notice).not.toContain("entry was not recorded");
    expect(notice).toContain("Do not repeat");
    expect(document.body.textContent).not.toContain("C:/fake/a.kdbx");
    expect(screen.getByText("Unlocked region")).toBeTruthy();
  });
}

const unlockedB = { sessionId: "", path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
function serve(operations: { id: string; originSession: string; kind: string }[]) {
  native.invoke.mockImplementation(async (command: string, args?: { ids: string[] }) => {
    if (command === "vault_status") return { ...unlockedB, sessionId: native.owner };
    if (command === "vault_warnings") return { application: [], operations };
    if (command === "vault_dismiss_warnings") { operations = operations.filter(({ id }) => !args?.ids.includes(id)); return; }
    if (command === "vault_activity") return;
    throw new Error(command);
  });
}
const savedNotices = () => screen.getByRole("region", { name: "Saved-operation notices" });

// FR-05 and FR-12: each native warning has its own text, and neither claims what the old shared one did.
test("a file-permission notice says who can copy the vault and claims nothing about backup retention", async () => {
  serve([{ id: "fixed-permissions", originSession: "A", kind: "filePermissions" }]);
  render(<App />);
  await screen.findByText("Unlocked region");
  await act(() => reads.warnings());
  const text = savedNotices().textContent!;
  expect(text).toContain("AKM could not make its files private to your account");
  expect(text).toContain("Anyone who can read its folder can copy the encrypted vault and its backups");
  expect(text).not.toContain("retention");
  expect(document.body.textContent).not.toContain("C:/fake/a.kdbx");
});

test("a lineage notice says that the vault changed outside AKM and does not claim that a change was saved", async () => {
  serve([{ id: "fixed-lineage", originSession: "A", kind: "journalLineageUnconfirmed" }]);
  render(<App />);
  await screen.findByText("Unlocked region");
  await act(() => reads.warnings());
  const text = savedNotices().textContent!;
  expect(text).toContain("A vault file changed outside AKM, or the journal mapping of this device was started again, so AKM cannot confirm that it is the same vault");
  // FV-01: after a mapping recovery the vault starts a new journal, so "the existing journal" would be false.
  expect(text).toContain("continues in the journal that AKM keeps for this path");
  expect(text).not.toContain("existing journal");
  expect(text).not.toContain("was saved");
  expect(text).not.toContain("activity entry could not be confirmed");
});

// After a reload the native record is the only truth about a terminal launch.
const TERMINAL_RECORDS = [
  ["terminalStarted", true],
  ["terminalSessionEnded", true],
  ["terminalFailed", false],
  ["terminalDeclined", false],
] as const;

for (const [kind, holdsKeys] of TERMINAL_RECORDS) {
  test(`after a reload a ${kind} record names the terminal and says ${holdsKeys ? "that it keeps the keys" : "that none runs"}`, async () => {
    serve([{ id: `terminal-${kind}`, originSession: "A", kind }]);
    render(<App />);
    await screen.findByText("Unlocked region");
    await act(() => reads.warnings());
    const text = savedNotices().textContent!;
    expect(text).toMatch(/^A terminal /);
    expect(text.includes("keeps the keys until you close it, even after AKM locks")).toBe(holdsKeys);
    expect(text.includes("No terminal from that request runs")).toBe(!holdsKeys);
    expect(screen.getByText("Unlocked region")).toBeTruthy();
  });
}

test("terminal notices are dismissed by their IDs and an old poll cannot bring them back", async () => {
  const records = TERMINAL_RECORDS.map(([kind]) => ({ id: `dismiss-${kind}`, originSession: "A", kind }));
  serve(records);
  const root = render(<App />);
  await screen.findByText("Unlocked region");
  await act(() => reads.warnings());
  expect(savedNotices().querySelectorAll(".notice").length).toBe(4);
  let deliver!: (value: unknown) => void;
  const answer = native.invoke.getMockImplementation()!;
  native.invoke.mockImplementationOnce(() => new Promise((done) => { deliver = done; }));
  const oldPoll = reads.warnings();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dismiss saved-operation notices" })); });
  expect(native.invoke).toHaveBeenCalledWith("vault_dismiss_warnings", { ids: records.map(({ id }) => id) });
  await act(async () => { deliver({ application: [], operations: records }); await oldPoll; });
  expect(screen.queryByRole("region", { name: "Saved-operation notices" })).toBeNull();
  native.invoke.mockImplementation(answer);
  root.unmount();
  render(<App />);
  await screen.findByText("Unlocked region");
  await act(() => reads.warnings());
  expect(screen.queryByText(/A terminal /)).toBeNull();
});

// Contract 5 at the App: a terminal record from session A settles A's receipt and leaves B open.
for (const [origin, staysOpen] of [["an older", true], ["the current", false]] as const) {
  test(`a lost terminal reply from ${origin} session, settled as session-ended, ${staysOpen ? "leaves the newer session open" : "locks the page"}`, async () => {
    serve([]);
    render(<App />);
    await screen.findByText("Unlocked region");
    const operation = crypto.randomUUID();
    act(() => { beginOutcome("terminal", staysOpen ? sessionGeneration() - 1 : sessionGeneration(), operation); });
    serve([{ id: operation, originSession: "A", kind: "terminalSessionEnded" }]);
    await act(() => reads.warnings());
    expect(screen.getByRole("region", { name: "Operation outcomes" }).textContent).toContain("A terminal opened with keys. It keeps them until you close it, even after AKM locks.");
    expect(screen.queryByRole("region", { name: "Saved-operation notices" })).toBeNull();
    expect(screen.queryByText("Unlocked region") !== null).toBe(staysOpen);
    acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  });
}

test("only explicitly dismissed IDs disappear; an old poll cannot resurrect them or erase a new notice", async () => {
  const first = { id: "dismiss-first", originSession: "A", kind: "journal" };
  const second = { id: "dismiss-new", originSession: "B", kind: "backupRefresh" };
  let notices = [first];
  let failDismiss = true;
  native.invoke.mockImplementation(async (command: string, args?: { ids: string[] }) => {
    if (command === "vault_status") return { sessionId: native.owner, path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };
    if (command === "vault_warnings") return { application: [], operations: notices };
    if (command === "vault_dismiss_warnings") {
      if (failDismiss) throw new Error("fake acknowledgment failure");
      expect(args?.ids).toEqual([first.id]);
      notices = [second]; // Arrived after the visible snapshot; must not be acknowledged with it.
      return;
    }
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  const root = render(<App />);
  await screen.findByText(/A vault change was saved, but its activity entry could not be confirmed/);
  fireEvent.click(screen.getByRole("button", { name: "Dismiss saved-operation notices" }));
  await screen.findByText(/Could not dismiss these notices/);
  expect(screen.getByText(/its activity entry could not be confirmed/)).toBeTruthy();
  let deliver!: (value: unknown) => void;
  native.invoke.mockImplementationOnce(() => new Promise((done) => { deliver = done; }));
  const oldPoll = reads.warnings();
  failDismiss = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dismiss saved-operation notices" })); });
  await act(async () => { deliver({ application: [], operations: [first] }); await oldPoll; await reads.warnings(); });
  expect(screen.queryByText(/its activity entry could not be confirmed/)).toBeNull();
  expect(screen.getByText(/backup could not be refreshed/)).toBeTruthy();
  root.unmount();
  render(<App />);
  await screen.findByText("Unlocked region");
  expect(screen.queryByText(/its activity entry could not be confirmed/)).toBeNull();
  expect(screen.getByText(/backup could not be refreshed/)).toBeTruthy();
});
