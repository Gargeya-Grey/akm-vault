import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { App } from "./App";
import { KeyDetail } from "./components/KeyDetail";
import { nativeOwner } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions, sessionGeneration, settleOutcome } from "./sessionBoundary";
import type { KeyView, Status } from "./types";

const native = vi.hoisted(() => ({ invoke: vi.fn(), listeners: new Map<string, (event: { payload: unknown }) => void>() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name: string, callback: (event: { payload: unknown }) => void) => {
  native.listeners.set(name, callback);
  return () => { if (native.listeners.get(name) === callback) native.listeners.delete(name); };
}) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Current unlocked keys</p> }));
afterEach(() => {
  cleanup(); native.listeners.clear();
  // A failing red assertion may stop before this test's deferred reply is delivered.
  // Settle only the test-owned leftovers; production correctly refuses pending dismissal.
  for (const { id, outcome } of retainedCompletions()) if (outcome === "pending") settleOutcome(id, "unconfirmed");
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
  vi.restoreAllMocks();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const expire = (sessionId: string, hideId = crypto.randomUUID()) => native.listeners.get("vault-view-expired")?.({ payload: { sessionId, hideId } });
const flush = () => act(async () => {});

function fixture() {
  // Reproduce the seam that jsdom visibility mocks cannot establish: native
  // minimization notice while the renderer still reports DOM-visible.
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const owner = crypto.randomUUID();
  let active = true;
  const status = (): Status => ({ sessionId: active ? owner : null, path: "C:/fake/current.kdbx", exists: true, unlocked: active, keyCount: 1, vaultName: active ? "Fake" : null, needsUpgrade: false });
  const pending = deferred<unknown>();
  let held = "";
  native.invoke.mockReset().mockImplementation(async (command: string, args?: { sessionId?: string | null }) => {
    if (command === held) return pending.promise;
    if (command === "vault_status") return status();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") { expect(args?.sessionId).toBe(owner); active = false; return; }
    if (command === "keys_list") return [{ project: "demo", environment: "dev" }];
    if (command === "backup_info") return { path: "C:/fake/current.bak", exists: false, bytes: 0, modified: null };
    if (command === "env_export") return "TOKEN=fake-old-preview-value\n";
    throw new Error(command);
  });
  return { owner, status, pending, hold: (command: string) => { held = command; }, active: () => active };
}

async function settings() {
  const root = render(<App />);
  await screen.findByText("Current unlocked keys");
  fireEvent.click(screen.getByRole("button", { name: "Use keys" }));
  await screen.findByRole("option", { name: "demo" });
  return root;
}
function importInput() {
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [name, value] of [["Paste the .env contents", "TOKEN=fake-private-input"], ["Target project", "private-project"], ["Target environment", "private-environment"]]) {
    fireEvent.change(screen.getByLabelText(name), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
}
function scopeExport() {
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
}

for (const operation of ["import", "export"] as const) {
  for (const reply of ["success", "error", ...(operation === "export" ? ["cancel" as const] : [])] as const) {
    for (const handoff of ["same-view", "navigate", "remount"] as const) {
      test(`native expiry with DOM-visible ${operation} ${reply} retains one outcome through ${handoff}`, async () => {
        const f = fixture();
        f.hold(operation === "import" ? "env_import" : "env_save");
        const root = await settings();
        if (operation === "import") {
          importInput();
        } else {
          scopeExport();
          fireEvent.click(screen.getByRole("button", { name: "Save .env…" }));
        }
        expect(retainedCompletions()).toHaveLength(1);
        const id = retainedCompletions()[0].id;
        const generation = sessionGeneration();
        act(() => expire(f.owner));
        expect(document.hidden).toBe(false);
        expect(sessionGeneration()).toBe(generation); // Local view expiry is NOT session loss.
        if (operation === "import") {
          expect(screen.queryByRole("dialog")).toBeNull();
          expect(screen.getByRole("button", { name: "Import .env…" })).toHaveProperty("disabled", false);
        } else expect(screen.getByRole("button", { name: "Save .env…" })).toHaveProperty("disabled", false);
        if (handoff === "navigate") fireEvent.click(screen.getByRole("button", { name: "Keys" }));
        if (handoff === "remount") { root.unmount(); render(<App />); await screen.findByText("Current unlocked keys"); }
        await act(async () => {
          if (reply === "error") f.pending.reject({ kind: "io", message: "private-error-detail C:/private/export.env" });
          else f.pending.resolve(reply === "cancel" ? null : operation === "import" ? { added: 1, updated: 0 } : "C:/private/export.env");
        });
        const expected = reply === "cancel" ? "cancelled" : reply === "success" ? (handoff === "remount" ? "completedWhileLocked" : "completed") : (handoff === "remount" ? "unconfirmed" : "reportedError");
        expect(retainedCompletions()).toMatchObject([{ id, operation, outcome: expected }]);
        expect(retainedCompletions()).toHaveLength(1);
        const outcomes = screen.getByRole("region", { name: "Operation outcomes" });
        for (const privateText of ["private-project", "private-environment", "fake-private-input", "private-error-detail", "C:/private/export.env"]) {
          expect(outcomes.textContent).not.toContain(privateText);
          expect(document.body.textContent).not.toContain(privateText);
        }
        expect(f.active()).toBe(true);
        expect(nativeOwner()).toBe(f.owner);
        expect(native.invoke.mock.calls.filter(([command]) => command === "vault_lock")).toHaveLength(0);
        expect(native.invoke.mock.calls.filter(([command]) => command === (operation === "import" ? "env_import" : "env_save"))).toHaveLength(1);
      });
    }
  }
}

for (const pending of [true, false]) {
  test(`native expiry hides ${pending ? "pending" : "displayed"} plaintext preview without a DOM visibility event`, async () => {
    const f = fixture();
    if (pending) f.hold("env_export");
    await settings();
    scopeExport();
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    if (!pending) await screen.findByText("TOKEN=fake-old-preview-value");
    act(() => expire(f.owner));
    if (pending) await act(async () => f.pending.resolve("TOKEN=fake-late-preview-value\n"));
    expect(document.body.textContent).not.toContain("fake-old-preview-value");
    expect(document.body.textContent).not.toContain("fake-late-preview-value");
    expect(f.active()).toBe(true);
  });
}

test("native expiry ignores a late import reply and duplicate notices do not clear new input", async () => {
  const f = fixture();
  f.hold("env_import");
  await settings();
  importInput();
  const hide = crypto.randomUUID();
  act(() => expire(f.owner, hide));
  expect(screen.queryByRole("dialog")).toBeNull();
  await act(async () => f.pending.resolve({ added: 1, updated: 0 }));
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  const input = screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement;
  fireEvent.change(input, { target: { value: "TOKEN=fake-new-input" } });
  act(() => expire(f.owner, hide));
  expect(input.value).toBe("TOKEN=fake-new-input");
  expect(screen.queryByRole("region", { name: "Import review" })).toBeNull();
  act(() => expire(f.owner));
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("old A or disposed-root expiry never clears active B's newer Use keys input", async () => {
  const a = fixture();
  const first = await settings();
  const disposed = native.listeners.get("vault-view-expired");
  first.unmount();
  const b = fixture();
  await settings();
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake-b-input" } });
  act(() => { expire(a.owner); disposed?.({ payload: { sessionId: b.owner, hideId: crypto.randomUUID() } }); });
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("TOKEN=fake-b-input");
  expect(nativeOwner()).toBe(b.owner);
  act(() => expire(b.owner));
  expect(screen.queryByRole("dialog")).toBeNull();
});

for (const payload of [{}, { sessionId: "bad", hideId: "bad" }, { sessionId: crypto.randomUUID() }, { sessionId: null, hideId: 9_007_199_254_740_992 }]) {
  test(`malformed native view expiry ${JSON.stringify(payload)} is explicit and locks only the expected application owner`, async () => {
    const f = fixture();
    await settings();
    await act(async () => native.listeners.get("vault-view-expired")?.({ payload }));
    expect(document.body.textContent).toContain("Could not confirm the native lock");
    expect(native.invoke.mock.calls.filter(([command]) => command === "vault_lock")[0]?.[1]).toEqual({ sessionId: f.owner });
  });
}

test("an unknown-owner hide is coalesced across bootstrap adoption, not replayed onto a new form", async () => {
  const f = fixture();
  const response = deferred<Status>();
  const original = native.invoke.getMockImplementation()!;
  native.invoke.mockImplementation(async (command: string, args: unknown) => command === "vault_status" ? response.promise : original(command, args));
  await act(async () => { render(<App />); });
  const hide = crypto.randomUUID();
  act(() => expire(f.owner, hide));
  await act(async () => response.resolve(f.status()));
  fireEvent.click(screen.getByRole("button", { name: "Use keys" }));
  await screen.findByRole("option", { name: "demo" });
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake-after-bootstrap" } });
  act(() => expire(f.owner, hide));
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("TOKEN=fake-after-bootstrap");
  act(() => expire(f.owner));
  expect(screen.queryByRole("dialog")).toBeNull();
  await flush();
});

for (const phase of ["pending", "displayed", "error"] as const) {
  test(`the native expiry's direct disclosure consumer fences a ${phase} reveal result`, async () => {
    const f = fixture(); f.hold("key_reveal");
    const item: KeyView = { id: "fake-key", title: "Fake reveal fixture", username: null, issuer: null, project: "demo", environment: "dev", scopes: [], envVar: "TOKEN", resolvedEnvVar: "TOKEN", url: null, notes: null, tags: [], expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0 };
    render(<><App /><KeyDetail item={item} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} /></>);
    await screen.findByText("Current unlocked keys");
    fireEvent.click(screen.getByRole("button", { name: "Reveal" }));
    if (phase === "displayed") { await act(async () => f.pending.resolve("fake-reveal-value")); expect(screen.getByText("fake-reveal-value")).toBeTruthy(); }
    act(() => expire(f.owner));
    if (phase === "pending") await act(async () => f.pending.resolve("fake-reveal-value"));
    if (phase === "error") await act(async () => f.pending.reject({ kind: "io", message: "fake-private-reveal-error" }));
    expect(document.body.textContent).not.toContain("fake-reveal-value");
    expect(document.body.textContent).not.toContain("fake-private-reveal-error");
    expect(screen.getByRole("button", { name: "Reveal" })).toHaveProperty("disabled", false);
    expect(nativeOwner()).toBe(f.owner);
  });
}

test("a failed native expiry subscription fails closed before any status is shown and locks the expected owner", async () => {
  const f = fixture();
  const ordinary = vi.mocked(listen).getMockImplementation()!;
  vi.mocked(listen).mockImplementationOnce(ordinary).mockImplementationOnce(() => Promise.reject(new Error("fake-private-listener-failure")));
  render(<App />);
  await screen.findByText("AKM could not watch the session. Restart AKM.");
  expect(screen.queryByText("Current unlocked keys")).toBeNull();
  expect(f.active()).toBe(false);
  expect(native.invoke.mock.calls.filter(([command]) => command === "vault_lock")[0]?.[1]).toEqual({ sessionId: f.owner });
  expect(document.body.textContent).not.toContain("fake-private-listener-failure");
});

test("a disposed root's late expiry-subscription failure cannot lock its replacement owner", async () => {
  fixture();
  const registration = deferred<() => void>();
  const ordinary = vi.mocked(listen).getMockImplementation()!;
  vi.mocked(listen).mockImplementationOnce(ordinary).mockImplementationOnce(() => registration.promise);
  const first = render(<App />);
  await flush();
  first.unmount();
  const b = fixture();
  await settings();
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake-new-root" } });
  await act(async () => registration.reject(new Error("fake-disposed-listener-failure")));
  expect(b.active()).toBe(true);
  expect(nativeOwner()).toBe(b.owner);
  expect(native.invoke.mock.calls.filter(([command]) => command === "vault_lock")).toHaveLength(0);
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("TOKEN=fake-new-root");
  expect(document.body.textContent).not.toContain("fake-disposed-listener-failure");
});
