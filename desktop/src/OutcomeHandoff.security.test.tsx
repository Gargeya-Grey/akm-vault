import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { acknowledgeCompletions, COMPLETION_MESSAGE, retainedCompletions } from "./sessionBoundary";

const native = vi.hoisted(() => ({ active: true, owner: "", onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Unlocked keys region</p> }));
afterEach(() => {
  cleanup();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  vi.restoreAllMocks();
});

type Operation = "import" | "export";
type Reply = "success" | "error" | "cancel";
type Handoff = "normal" | "hide" | "navigate" | "return" | "between-roots" | "after-remount" | "lock-b";

async function dispatch(operation: Operation) {
  native.invoke.mockReset();
  native.active = true;
  native.owner = crypto.randomUUID();
  let settle!: (reply: Reply) => void;
  const status = () => ({ sessionId: native.active ? native.owner : null, path: "C:/fake/current.kdbx", exists: true, unlocked: native.active, keyCount: 1, vaultName: "Fake", needsUpgrade: false });
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status();
    if (command === "vault_activity" || command === "vault_dismiss_warnings") return;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_prepare_open") { native.owner = crypto.randomUUID(); return native.owner; }
    if (command === "vault_unlock") { native.active = true; return status(); }
    if (command === "keys_list") return [{ id: "fake-key", title: "Fixture key", project: "demo", environment: "dev" }];
    if (command === "backup_info") return { path: "C:/fake/current.bak", exists: false, bytes: 0, modified: null };
    if (command === (operation === "import" ? "env_import" : "env_save")) {
      return new Promise((resolve, reject) => {
        settle = (reply) => reply === "error"
          ? reject({ kind: "io", message: "Error for C:/fake/private-origin-a-export.env: FAKE_DETAIL_MARKER" })
          : resolve(reply === "cancel" ? null : operation === "import" ? { added: 1, updated: 0 } : "C:/fake/private-origin-a-export.env");
      });
    }
    throw new Error(`Unexpected command ${command}`);
  });
  const root = render(<App />);
  await screen.findByText("Unlocked keys region");
  fireEvent.click(screen.getByRole("button", { name: "Use keys" }));
  if (operation === "import") {
    fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
    for (const [label, value] of [["Paste the .env contents", "PRIVATE_TOKEN=fake-private-import-value"], ["Target project", "private-scope-a"], ["Target environment", "private-env-a"]]) {
      fireEvent.change(screen.getByLabelText(label), { target: { value } });
    }
    fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  } else {
    await screen.findByRole("option", { name: "demo" });
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
    fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
    fireEvent.click(screen.getByRole("button", { name: "Save .env…" }));
  }
  return { root, settle };
}

function hideAndRestore() {
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
}

for (const operation of ["import", "export"] as const) {
  for (const reply of ["success", "error"] as const) {
    for (const handoff of ["normal", "hide", "navigate", "return", "between-roots", "after-remount", "lock-b"] as Handoff[]) {
      test(`${operation} ${reply}: retained once through ${handoff}`, async () => {
        const { root, settle } = await dispatch(operation);
        if (handoff !== "normal") hideAndRestore();
        if (handoff === "between-roots" || handoff === "after-remount") {
          root.unmount();
          if (handoff === "after-remount") { render(<App />); await screen.findByText("Unlocked keys region"); }
        } else if (handoff === "lock-b") {
          act(() => { native.active = false; native.onLock?.({ payload: { sessionId: native.owner } }); });
          fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-for-b" } });
          fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
          await screen.findByText("Unlocked keys region");
        } else if (handoff !== "normal" && handoff !== "hide") {
          fireEvent.click(screen.getByRole("button", { name: "Keys" }));
          await screen.findByText("Unlocked keys region");
          if (handoff === "return") fireEvent.click(screen.getByRole("button", { name: "Use keys" }));
        }
        await act(async () => settle(reply));
        if (handoff === "between-roots") { render(<App />); await screen.findByText("Unlocked keys region"); }
        const changed = ["between-roots", "after-remount", "lock-b"].includes(handoff);
        const expected = reply === "success" ? (changed ? "completedWhileLocked" : "completed") : (changed ? "unconfirmed" : "reportedError");
        const records = retainedCompletions();
        expect(records).toHaveLength(1);
        expect(records[0]).toMatchObject({ operation, outcome: expected });
        const region = screen.getByRole("region", { name: "Operation outcomes" });
        expect(within(region).getAllByRole(reply === "success" ? "status" : "alert")).toHaveLength(1);
        if (reply === "success" && changed) expect(within(region).getByText(COMPLETION_MESSAGE)).toBeTruthy();
        for (const forbidden of ["private-scope-a", "private-env-a", "PRIVATE_TOKEN", "fake-private-import-value", "private-origin-a-export.env", "FAKE_DETAIL_MARKER", "fake-private-review-token"]) {
          expect(region.textContent).not.toContain(forbidden);
          expect(JSON.stringify(records)).not.toContain(forbidden);
          if (handoff !== "normal") expect(document.body.textContent).not.toContain(forbidden);
        }
        if (handoff === "normal") {
          if (reply === "error") expect(screen.getAllByText(/FAKE_DETAIL_MARKER/).length).toBeGreaterThan(0);
          else if (operation === "import") expect(screen.getByText("Imported: 1 added, 0 updated.")).toBeTruthy();
          else expect(screen.getByText(/Saved plaintext secrets to C:\/fake\/private-origin-a-export.env/)).toBeTruthy();
        }
        expect(native.invoke.mock.calls.filter(([command]) => command === (operation === "import" ? "env_import" : "env_save"))).toHaveLength(1);
        if (!["normal", "hide", "return"].includes(handoff)) expect(screen.getByText("Unlocked keys region")).toBeTruthy();
        fireEvent.click(screen.getByRole("button", { name: "Dismiss operation outcomes" }));
        expect(retainedCompletions()).toHaveLength(0);
        cleanup();
        render(<App />);
        await screen.findByText("Unlocked keys region");
        expect(screen.queryByRole("region", { name: "Operation outcomes" })).toBeNull();
      });
    }
  }
}

test("a cancelled file dialog after navigation is not a saved export", async () => {
  const { settle } = await dispatch("export");
  hideAndRestore();
  fireEvent.click(screen.getByRole("button", { name: "Keys" }));
  await act(async () => settle("cancel"));
  expect(retainedCompletions()).toHaveLength(1);
  expect(retainedCompletions()[0]).toMatchObject({ operation: "export", outcome: "cancelled" });
  const region = screen.getByRole("region", { name: "Operation outcomes" });
  expect(region.textContent).toContain("cancelled");
  expect(region.textContent).toContain("No file was saved by this request.");
  expect(region.textContent).not.toContain("completed");
});

test("pending outcomes cannot be dismissed; a later settled snapshot does not erase an unseen request", async () => {
  const { settle } = await dispatch("import");
  const pending = retainedCompletions();
  expect(pending).toHaveLength(1);
  expect(pending[0]).toMatchObject({ operation: "import", outcome: "pending" });
  act(() => acknowledgeCompletions(pending.map(({ id }) => id)));
  expect(retainedCompletions()).toHaveLength(1);
  await act(async () => settle("success"));
  expect(retainedCompletions()[0].id).toBe(pending[0].id);
  const settledIds = retainedCompletions().map(({ id }) => id);
  // A different request can start while the first historical outcome remains visible.
  cleanup();
  const next = await dispatch("export");
  act(() => acknowledgeCompletions(settledIds));
  expect(retainedCompletions()).toHaveLength(1);
  expect(retainedCompletions()[0]).toMatchObject({ operation: "export", outcome: "pending" });
  await act(async () => next.settle("cancel"));
});
