import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { UseKeys } from "./UseKeys";
import { reads } from "../native";
import { adoptReservation, PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { acknowledgeNotices, retainedOperationNotices } from "../operationNotices";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "../sessionBoundary";

// U2-R1-03, fault class "one state owned by two places". The receipts in sessionBoundary.ts own
// the state of a dispatched operation. The form derives its wait and its message from them. Each
// test is one row of the state table in docs/design/frontend-reads-and-rows.md. The real api,
// boundary and receipts run here; only the Tauri IPC is mocked.

type Deferred = { resolve: (value: unknown) => void; reject: (reason: unknown) => void };
const native = vi.hoisted(() => ({
  calls: [] as { command: string; args: Record<string, unknown> }[],
  pending: new Map<string, Deferred[]>(),
  records: [] as { id: string; originSession: string; kind: string }[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string, args: Record<string, unknown> = {}) => {
    native.calls.push({ command, args });
    if (command === "keys_list") return [{ id: "fake-key", title: "Fake", project: "demo", environment: "dev" }];
    if (command === "vault_warnings") return { application: [], operations: native.records };
    if (command === "vault_dismiss_warnings") {
      native.records = native.records.filter(({ id }) => !(args.ids as string[]).includes(id));
      return;
    }
    return new Promise((resolve, reject) => {
      const queue = native.pending.get(command) ?? [];
      queue.push({ resolve, reject });
      native.pending.set(command, queue);
    });
  }),
}));

beforeEach(() => {
  native.calls = [];
  native.pending.clear();
  native.records = [];
  adoptReservation(crypto.randomUUID());
});
afterEach(() => {
  cleanup();
  invalidateSession();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  acknowledgeNotices(retainedOperationNotices().map(({ id }) => id));
});

const TERMINAL = "Open terminal with keys…";
const WAITING_TERMINAL = /Choose the folder and answer the confirmation window/;
const WAITING_IMPORT = /Waiting for your answer in the AKM confirmation window|An import is waiting for your answer in the AKM confirmation window/;
const button = (name: string | RegExp) => screen.getByRole("button", { name }) as HTMLButtonElement;
const callsOf = (command: string) => native.calls.filter((call) => call.command === command);
const operationOf = (command: string, index = -1) => callsOf(command).at(index)!.args.operationId as string;
const dismissed = () => callsOf("vault_dismiss_warnings").flatMap((call) => call.args.ids as string[]);

async function settle(command: string, how: "resolve" | "reject", value: unknown) {
  const next = native.pending.get(command)?.shift();
  if (!next) throw new Error(`no pending ${command}`);
  await act(async () => { next[how](value); });
  await act(async () => {});
}
async function record(command: string, kind: string, index = -1) {
  native.records = [...native.records, { id: operationOf(command, index), originSession: "A", kind }];
  await act(async () => { await reads.warnings(); });
  await act(async () => {});
}
async function mountScoped() {
  await act(async () => { render(<UseKeys />); });
  await act(async () => {});
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
}
async function launch() {
  await act(async () => { fireEvent.click(button(TERMINAL)); });
}
function terminalWaits() {
  return button(TERMINAL).disabled && screen.queryByText(WAITING_TERMINAL) !== null;
}
function hide() {
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => { document.dispatchEvent(new Event("visibilitychange")); });
  hidden.mockReturnValue(false);
  hidden.mockRestore();
}
function expire(how: "hide" | "native view expiry") {
  if (how === "hide") hide();
  else act(() => { window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT)); });
}

// The terminal launch.

test("terminal row 1, the reply: the form waits until the reply, shows its count and acknowledges the record", async () => {
  await mountScoped();
  await launch();
  expect(terminalWaits()).toBe(true);
  await settle("env_launch_terminal", "resolve", 3);
  expect(terminalWaits()).toBe(false);
  expect(screen.getByText("Opened a terminal with 3 keys. That window keeps them until you close it, even after AKM locks.")).toBeTruthy();
  expect(dismissed()).toEqual([operationOf("env_launch_terminal")]);
});

for (const [kind, text] of [
  ["terminalStarted", "A terminal opened with keys. That window keeps them until you close it, even after AKM locks."],
  ["terminalSessionEnded", "A terminal opened with keys, but its session ended before the result arrived. That window keeps them until you close it, even after AKM locks."],
  ["terminalFailed", "The terminal launch failed. No terminal from that request runs."],
  ["terminalDeclined", "Nothing was opened. You cancelled in the folder window or the confirmation window."],
] as const) {
  test(`terminal row 2, the ${kind} record and no reply: the record ends the wait and the form shows its outcome`, async () => {
    await mountScoped();
    await launch();
    await record("env_launch_terminal", kind);
    expect(terminalWaits()).toBe(false);
    expect(screen.queryByText(WAITING_TERMINAL)).toBeNull();
    expect(button(TERMINAL).getAttribute("aria-busy")).toBeNull();
    expect(screen.getByText(text)).toBeTruthy();
    expect(dismissed()).toEqual([operationOf("env_launch_terminal")]);
  });
}

test("terminal row 3, a reply after the record changes nothing", async () => {
  await mountScoped();
  await launch();
  await record("env_launch_terminal", "terminalStarted");
  await settle("env_launch_terminal", "resolve", 5);
  expect(screen.queryByText(/Opened a terminal with 5 keys/)).toBeNull();
  expect(screen.getByText("A terminal opened with keys. That window keeps them until you close it, even after AKM locks.")).toBeTruthy();
  expect(terminalWaits()).toBe(false);
});

test("terminal row 4, a record after the reply is dropped, and the reply stays on the form", async () => {
  await mountScoped();
  await launch();
  await settle("env_launch_terminal", "resolve", 2);
  await record("env_launch_terminal", "terminalFailed");
  expect(screen.getByText(/Opened a terminal with 2 keys\./)).toBeTruthy();
  expect(screen.queryByText("The terminal launch failed. No terminal from that request runs.")).toBeNull();
  expect(retainedOperationNotices()).toEqual([]);
  expect(retainedCompletions()).toMatchObject([{ operation: "terminal", outcome: "completed" }]);
});

test("terminal row 5, neither: the form waits, and a second launch is impossible", async () => {
  await mountScoped();
  await launch();
  await launch();
  expect(callsOf("env_launch_terminal")).toHaveLength(1);
  expect(terminalWaits()).toBe(true);
});

test("terminal row 7, a transport failure: the form stops waiting, says that no answer came, and keeps the record", async () => {
  await mountScoped();
  await launch();
  await settle("env_launch_terminal", "reject", new Error("Fake channel closed."));
  expect(terminalWaits()).toBe(false);
  expect(screen.getByText(/A terminal launch sent no answer to this window\. A terminal may hold keys\./)).toBeTruthy();
  expect(dismissed()).toEqual([]);
});

test("terminal row 8, a newer session does not wait for an older launch, and the older record never settles the newer one", async () => {
  await mountScoped();
  await launch();
  cleanup();
  invalidateSession();
  adoptReservation(crypto.randomUUID());
  await mountScoped();
  expect(terminalWaits()).toBe(false);
  await launch();
  await record("env_launch_terminal", "terminalStarted", 0);
  expect(terminalWaits()).toBe(true);
  await settle("env_launch_terminal", "resolve", 1); // the older launch answers late
  expect(terminalWaits()).toBe(true);
  await settle("env_launch_terminal", "resolve", 1);
  expect(terminalWaits()).toBe(false);
  expect(screen.getByText(/Opened a terminal with 1 key\./)).toBeTruthy();
});

test("terminal row 9, a second launch waits for its own receipt, and a late reply of the first changes nothing", async () => {
  await mountScoped();
  await launch();
  await record("env_launch_terminal", "terminalDeclined");
  await launch();
  expect(callsOf("env_launch_terminal")).toHaveLength(2);
  expect(operationOf("env_launch_terminal", 0)).not.toBe(operationOf("env_launch_terminal", 1));
  expect(terminalWaits()).toBe(true);
  await settle("env_launch_terminal", "resolve", 9);
  expect(terminalWaits()).toBe(true);
  expect(screen.queryByText(/Opened a terminal with 9 keys/)).toBeNull();
  await settle("env_launch_terminal", "resolve", 4);
  expect(screen.getByText(/Opened a terminal with 4 keys\./)).toBeTruthy();
});

for (const how of ["hide", "native view expiry"] as const) {
  test(`terminal row 10, a ${how} while it waits keeps the button disabled until the receipt settles`, async () => {
    await mountScoped();
    await launch();
    expire(how);
    expect(button(TERMINAL).disabled).toBe(true);
    await settle("env_launch_terminal", "resolve", 3);
    expect(button(TERMINAL).disabled).toBe(false);
    expect(document.body.textContent).not.toContain("Opened a terminal");
  });
}

test("terminal row 10, a hide while it waits and then the record: the wait ends and the form shows nothing of the record", async () => {
  await mountScoped();
  await launch();
  hide();
  await record("env_launch_terminal", "terminalStarted");
  expect(terminalWaits()).toBe(false);
  expect(document.body.textContent).not.toContain("A terminal opened with keys");
});

test("terminal row 10, after a change of view the new view waits for the same launch", async () => {
  await mountScoped();
  await launch();
  cleanup();
  await mountScoped();
  expect(terminalWaits()).toBe(true);
  await record("env_launch_terminal", "terminalStarted");
  expect(terminalWaits()).toBe(false);
});

// The import.

async function startImport(text = "TOKEN=fake-import-value") {
  await act(async () => { render(<UseKeys />); });
  await act(async () => {});
  fireEvent.click(button("Import .env…"));
  for (const [label, value] of [["Paste the .env contents", text], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  await act(async () => { fireEvent.click(button("Review import")); });
}
const importWaits = () => screen.queryByText(WAITING_IMPORT) !== null;

test("import row 1, the reply: the drawer closes, the counts show, and the list reads again", async () => {
  await startImport();
  expect(importWaits()).toBe(true);
  await settle("env_import", "resolve", { added: 1, updated: 0 });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Imported: 1 added, 0 updated.")).toBeTruthy();
  expect(callsOf("keys_list")).toHaveLength(2);
  expect(dismissed()).toEqual([operationOf("env_import")]);
});

test("import row 2, the applied record and no reply: the drawer closes, its text is gone, and the list reads again", async () => {
  await startImport();
  await record("env_import", "importApplied");
  expect(importWaits()).toBe(false);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("The import was saved. This window did not receive the counts, so check the keys before you import again.")).toBeTruthy();
  expect(callsOf("keys_list")).toHaveLength(2);
  fireEvent.click(button("Import .env…"));
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("");
});

test("import row 2, the declined record and no reply: the form can be edited and says that nothing was imported", async () => {
  await startImport("TOKEN=fake-kept");
  await record("env_import", "importDeclined");
  expect(importWaits()).toBe(false);
  expect(screen.getByText("Nothing was imported. You declined it in the confirmation window.")).toBeTruthy();
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("TOKEN=fake-kept");
  expect(button("Review import").disabled).toBe(false);
  expect(callsOf("keys_list")).toHaveLength(1);
});

test("import row 2, the failed record and no reply: the form says that nothing is confirmed and can be used again", async () => {
  await startImport();
  await record("env_import", "importFailed");
  expect(importWaits()).toBe(false);
  expect(screen.getAllByText("The import reported an error. This does not confirm that nothing was saved. Check the keys before you try again.").length).toBeGreaterThan(0);
  expect(button("Review import").disabled).toBe(false);
});

test("import row 3, a reply after the record changes nothing", async () => {
  await startImport();
  await record("env_import", "importApplied");
  await settle("env_import", "resolve", { added: 7, updated: 0 });
  expect(screen.queryByText(/Imported: 7 added/)).toBeNull();
  expect(callsOf("keys_list")).toHaveLength(2);
});

test("import row 5, neither: the form waits and cannot send a second import", async () => {
  await startImport();
  await act(async () => { fireEvent.click(button("Waiting for confirmation…")); });
  expect(callsOf("env_import")).toHaveLength(1);
  expect(importWaits()).toBe(true);
});

test("import row 7, a transport failure: the form stops waiting, says that no answer came, and keeps the record", async () => {
  await startImport();
  await settle("env_import", "reject", "Fake channel closed.");
  expect(importWaits()).toBe(false);
  expect(screen.getAllByText(/The import sent no answer to this window\. This does not confirm that nothing was saved\./).length).toBeGreaterThan(0);
  expect(button("Review import").disabled).toBe(false);
  expect(dismissed()).toEqual([]);
});

for (const how of ["hide", "native view expiry"] as const) {
  test(`import row 10, a ${how} while it waits closes the drawer and keeps the import button disabled until the receipt settles`, async () => {
    await startImport("TOKEN=fake-private-text");
    expire(how);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(button("Import .env…").disabled).toBe(true);
    expect(importWaits()).toBe(true);
    await settle("env_import", "resolve", { added: 1, updated: 0 });
    expect(button("Import .env…").disabled).toBe(false);
    expect(screen.queryByText("Imported: 1 added, 0 updated.")).toBeNull();
    expect(callsOf("keys_list")).toHaveLength(2); // the vault changed, so the list reads again
    expect(document.body.textContent).not.toContain("fake-private-text");
  });
}

test("import row 10, a hide while it waits and then the applied record: the list reads again and the form shows nothing of it", async () => {
  await startImport();
  hide();
  await record("env_import", "importApplied");
  expect(button("Import .env…").disabled).toBe(false);
  expect(callsOf("keys_list")).toHaveLength(2);
  expect(document.body.textContent).not.toContain("The import was saved.");
});

test("import row 10, after a change of view the new view waits for the same import", async () => {
  await startImport();
  cleanup();
  await act(async () => { render(<UseKeys />); });
  expect(button("Import .env…").disabled).toBe(true);
  expect(importWaits()).toBe(true);
  await record("env_import", "importDeclined");
  expect(button("Import .env…").disabled).toBe(false);
});

for (const how of ["reply", "record"] as const) {
  test(`import row 10, after a change of view the ${how} of a saved import makes the new view read the list again (U2-R2-02)`, async () => {
    await startImport();
    cleanup();
    await act(async () => { render(<UseKeys />); });
    await act(async () => {});
    const before = callsOf("keys_list").length;
    if (how === "reply") await settle("env_import", "resolve", { added: 1, updated: 0 });
    else await record("env_import", "importApplied");
    expect(callsOf("keys_list")).toHaveLength(before + 1);
    expect(button("Import .env…").disabled).toBe(false);
  });
}

// The file export has a receipt but no native record (the sweep of the same class).

for (const how of ["hide", "native view expiry", "a change of view"] as const) {
  test(`export row 10, after ${how} while the save waits the save button stays disabled until the reply`, async () => {
    await mountScoped();
    await act(async () => { fireEvent.click(button("Save .env…")); });
    if (how === "a change of view") { cleanup(); await mountScoped(); } else expire(how);
    expect(button(/Saving…|Save \.env…/).disabled).toBe(true);
    await settle("env_save", "resolve", "C:/fake/export.env");
    expect(button("Save .env…").disabled).toBe(false);
  });
}
