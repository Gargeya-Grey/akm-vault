import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { retainedApplicationWarnings, retainedOperationNotices } from "./operationNotices";

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
    const oldReply = delayed ? api.warnings().catch((error: unknown) => error) : undefined;
    act(() => { native.active = false; native.onLock?.({ payload: { sessionId: native.owner } }); });
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-for-b" } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
    await screen.findByText("Unlocked region");
    await act(async () => { if (delayed) { resolveWarning(report); await oldReply; } else { await api.warnings(); } });
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
  const pending = api.warnings();
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
  const pending = api.warnings();
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
  const oldPoll = api.warnings();
  failDismiss = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dismiss warnings" })); });
  await act(async () => { deliver({ application: [first], operations: [] }); await oldPoll; });
  expect(screen.queryByText(first.message)).toBeNull();
  await act(() => api.warnings());
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
    await act(() => api.warnings());
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
  const oldPoll = api.warnings();
  failDismiss = false;
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dismiss saved-operation notices" })); });
  await act(async () => { deliver({ application: [], operations: [first] }); await oldPoll; await api.warnings(); });
  expect(screen.queryByText(/its activity entry could not be confirmed/)).toBeNull();
  expect(screen.getByText(/backup could not be refreshed/)).toBeTruthy();
  root.unmount();
  render(<App />);
  await screen.findByText("Unlocked region");
  expect(screen.queryByText(/its activity entry could not be confirmed/)).toBeNull();
  expect(screen.getByText(/backup could not be refreshed/)).toBeTruthy();
});
