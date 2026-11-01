import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { api } from "./api";
import { beginNativeOpen, endNativeOpen, nativeOwner, NATIVE_PROTOCOL_ERROR } from "./nativeSession";
import { acknowledgeCompletions, retainedCompletions } from "./sessionBoundary";
import type { Status } from "./types";

const native = vi.hoisted(() => ({ invoke: vi.fn(), onLock: undefined as undefined | ((event: unknown) => void) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Protected current vault</p> }));
afterEach(() => { cleanup(); acknowledgeCompletions(retainedCompletions().map(({ id }) => id)); vi.restoreAllMocks(); });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const flush = () => act(async () => {});
const emit = (sessionId: string) => native.onLock?.({ payload: { sessionId } });
const calls = (command: string) => native.invoke.mock.calls.filter(([name]) => name === command);

// Models native Opening separately from installed Active and holds IPC delivery separately
// from both. No timer/visibility/OS/real-vault/clipboard operation is used here.
function fixture() {
  const a = crypto.randomUUID();
  const b = crypto.randomUUID();
  const prepare = deferred<unknown>();
  const reply = deferred<Status>();
  let owner: string | null = a;
  let active = true;
  const status = (): Status => ({ sessionId: active ? owner : null, path: "C:/fake/current.kdbx", exists: true, unlocked: active, keyCount: active ? 1 : 0, vaultName: active ? "Current" : null, needsUpgrade: false });
  const completed: Status = { ...status(), sessionId: b, path: "C:/fake/private-b.kdbx", vaultName: "Private B" };
  native.invoke.mockReset().mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "vault_status") return status();
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    if (command === "vault_lock") {
      if (args?.sessionId !== owner) throw { kind: "locked", message: "A different native owner is current." };
      owner = null;
      active = false;
      return;
    }
    if (command === "vault_prepare_open") { owner = b; active = false; return prepare.promise; }
    if (command === "vault_unlock" || command === "vault_create") {
      if (args?.sessionId !== b || owner !== b) throw { kind: "locked", message: "Reservation was invalidated." };
      return reply.promise;
    }
    throw new Error(command);
  });
  return {
    a, b, prepare, reply, status, completed,
    active: () => active,
    installed: () => { owner = b; active = true; },
    invalidate: () => { owner = null; active = false; emit(b); },
    retired: () => { owner = null; active = false; },
  };
}

async function beginB(f: ReturnType<typeof fixture>, mode: "unlock" | "create") {
  const root = render(<App />);
  await screen.findByText("Protected current vault");
  fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
  await flush();
  if (mode === "create") fireEvent.click(screen.getByRole("button", { name: "Create a new vault instead" }));
  fireEvent.change(screen.getByLabelText("Vault file"), { target: { value: f.completed.path } });
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-current-password" } });
  if (mode === "create") fireEvent.change(screen.getByLabelText("Confirm password"), { target: { value: "fake-current-password" } });
  fireEvent.click(screen.getByRole("button", { name: mode === "unlock" ? "Unlock vault" : "Create vault" }));
  expect(calls("vault_prepare_open")).toHaveLength(1);
  expect(calls("vault_prepare_open")[0][1]).toBeUndefined();
  expect(calls(`vault_${mode}`)).toHaveLength(0); // No credentials in reservation IPC.
  return root;
}

async function acknowledge(f: ReturnType<typeof fixture>, mode: "unlock" | "create") {
  await act(async () => f.prepare.resolve(f.b));
  expect(calls(`vault_${mode}`)).toHaveLength(1);
  expect(calls(`vault_${mode}`)[0][1]).toMatchObject({ sessionId: f.b, password: "fake-current-password" });
}

for (const mode of ["unlock", "create"] as const) {
  for (const stage of ["before-reservation-ack", "during-kdf", "before-reply", "after-open"] as const) {
    test(`old A and duplicate events ${stage} preserve B ${mode}, while a current B event still locks`, async () => {
      const f = fixture();
      await beginB(f, mode);
      if (stage !== "before-reservation-ack") await acknowledge(f, mode);
      if (stage === "before-reply" || stage === "after-open") f.installed();
      if (stage === "after-open") await act(async () => f.reply.resolve(f.completed));
      act(() => { emit(f.a); emit(f.a); });
      expect(screen.queryByText(NATIVE_PROTOCOL_ERROR)).toBeNull();
      if (stage !== "after-open") {
        expect((screen.getByLabelText("Master password") as HTMLInputElement).value).toBe("fake-current-password");
        expect(screen.getByRole("button", { name: "Working…" })).toHaveProperty("disabled", true);
      }
      if (stage === "before-reservation-ack") await acknowledge(f, mode);
      if (stage !== "after-open") {
        f.installed();
        await act(async () => f.reply.resolve(f.completed));
      }
      expect(screen.getByText("Protected current vault")).toBeTruthy();
      expect(nativeOwner()).toBe(f.b);
      expect(f.active()).toBe(true);
      act(f.invalidate);
      expect(screen.queryByText("Protected current vault")).toBeNull();
      const password = screen.getByLabelText("Master password") as HTMLInputElement;
      expect(password.value).toBe("");
      fireEvent.change(password, { target: { value: "fake-next-input" } });
      act(() => { emit(f.b); emit(f.a); emit(f.b); });
      expect((screen.getByLabelText("Master password") as HTMLInputElement).value).toBe("fake-next-input");
    });

    test(`a genuine B lock ${stage} fences the ${mode} result and never reopens protected UI`, async () => {
      const f = fixture();
      await beginB(f, mode);
      if (stage !== "before-reservation-ack") await acknowledge(f, mode);
      if (stage === "before-reply" || stage === "after-open") f.installed();
      if (stage === "after-open") await act(async () => f.reply.resolve(f.completed));
      act(f.invalidate);
      if (stage === "before-reservation-ack") {
        await act(async () => f.prepare.resolve(f.b));
        expect(calls(`vault_${mode}`)).toHaveLength(0); // Tombstone predates reservation delivery.
      } else if (stage === "during-kdf") {
        await act(async () => f.reply.reject({ kind: "locked", message: "private obsolete KDF error" }));
      } else if (stage === "before-reply") {
        await act(async () => f.reply.resolve(f.completed)); // Native reply already computed before lock.
      }
      expect(f.active()).toBe(false);
      expect(screen.queryByText("Protected current vault")).toBeNull();
      expect(document.body.textContent).not.toContain("Private B");
      expect(document.body.textContent).not.toContain("private obsolete");
      expect((screen.getByLabelText("Master password") as HTMLInputElement).value).toBe("");
      if (mode === "create" && stage === "before-reply") expect(retainedCompletions()).toHaveLength(1);
    });
  }

  test(`a current ${mode} error survives old A and duplicate retired B events`, async () => {
    const f = fixture();
    await beginB(f, mode);
    await acknowledge(f, mode);
    f.retired(); // Failed native opening retires silently, preserving the terminal error.
    await act(async () => f.reply.reject({ kind: "open", message: "That fake password does not open this vault." }));
    expect(screen.getByText("That fake password does not open this vault.")).toBeTruthy();
    act(() => { emit(f.a); emit(f.a); emit(f.b); emit(f.b); });
    expect(screen.getByText("That fake password does not open this vault.")).toBeTruthy();
    expect(screen.queryByText("Protected current vault")).toBeNull();
  });
}

for (const between of [true, false]) {
  test(`a native invalidation ${between ? "before" : "after"} a new root's bootstrap reply rejects that owner's metadata`, async () => {
    const f = fixture();
    const root = render(<App />);
    await screen.findByText("Protected current vault");
    const oldListener = native.onLock;
    root.unmount();
    f.installed(); // Frontend still remembers A; native B has not been adopted by any root.
    const pending = deferred<Status>();
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return pending.promise;
      if (command === "vault_warnings") return { application: [], operations: [] };
      if (command === "vault_activity") return;
      throw new Error(command);
    });
    await act(async () => { render(<App />); });
    // The disposed listener must not mutate a replacement root. Its new listener
    // still records a current native notice, even before status adoption.
    act(() => oldListener?.({ payload: { sessionId: crypto.randomUUID() } }));
    if (between) act(() => emit(f.b));
    await act(async () => pending.resolve({ ...f.completed, path: "C:/fake/private-a.kdbx", vaultName: "Private A" }));
    if (!between) {
      expect(screen.getByText("Protected current vault")).toBeTruthy();
      act(() => emit(f.b));
    }
    expect(screen.queryByText("Protected current vault")).toBeNull();
    expect(document.body.textContent).not.toContain("Private A");
    if (between) expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });
}

test("a tombstoned native owner cannot be resurrected by status after root teardown", async () => {
  const f = fixture();
  const root = render(<App />);
  await screen.findByText("Protected current vault");
  act(() => emit(f.a));
  root.unmount();
  render(<App />); // Deliberately stale status still reports native-active A.
  await screen.findByRole("button", { name: "Try again" });
  expect(screen.queryByText("Protected current vault")).toBeNull();
});

for (const invalidated of [true, false]) {
  test(`an unknown-owner bootstrap error ${invalidated ? "overtaken by a native notice is sanitized" : "without a native notice remains actionable"}`, async () => {
    const f = fixture();
    const token = beginNativeOpen();
    endNativeOpen(token); // New document-like bootstrap with no adopted owner.
    const pending = deferred<Status>();
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return pending.promise;
      if (command === "vault_warnings") return { application: [], operations: [] };
      throw new Error(command);
    });
    await act(async () => { render(<App />); });
    if (invalidated) act(() => emit(f.a));
    await act(async () => pending.reject({ kind: "io", message: "private bootstrap metadata error" }));
    expect(document.body.textContent?.includes("private bootstrap metadata error")).toBe(!invalidated);
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(screen.queryByText("Protected current vault")).toBeNull();
  });
}

test("an unrelated old notice during unknown bootstrap does not discard a valid B status", async () => {
  const f = fixture();
  const token = beginNativeOpen();
  endNativeOpen(token);
  const pending = deferred<Status>();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return pending.promise;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  await act(async () => { render(<App />); });
  act(() => { emit(f.a); emit(f.a); });
  await act(async () => pending.resolve(f.completed));
  expect(screen.getByText("Protected current vault")).toBeTruthy();
  expect(nativeOwner()).toBe(f.b);
});

for (const present of [true, false]) {
  test(`a locked status with ${present ? "a nonnull" : "a missing"} owner is rejected explicitly`, async () => {
    const f = fixture();
    const invalid: Record<string, unknown> = { ...f.status(), unlocked: false };
    if (!present) delete invalid.sessionId;
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return invalid;
      if (command === "vault_warnings") return { application: [], operations: [] };
      throw new Error(command);
    });
    render(<App />);
    await screen.findByText(NATIVE_PROTOCOL_ERROR);
    expect(screen.queryByText("Protected current vault")).toBeNull();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });
}

for (const stage of ["reservation", "credentials"] as const) {
  test(`root teardown during ${stage} cannot adopt an old owner over a newer root`, async () => {
    const f = fixture();
    const root = await beginB(f, "unlock");
    if (stage === "credentials") await acknowledge(f, "unlock");
    root.unmount();
    const c = crypto.randomUUID();
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "vault_status") return { ...f.completed, sessionId: c, vaultName: "New root" };
      if (command === "vault_warnings") return { application: [], operations: [] };
      if (command === "vault_activity") return;
      throw new Error(command);
    });
    render(<App />);
    await screen.findByText("Protected current vault");
    if (stage === "reservation") await act(async () => f.prepare.resolve(f.b));
    else await act(async () => f.reply.resolve(f.completed));
    act(() => { emit(f.a); emit(f.b); });
    expect(nativeOwner()).toBe(c);
    expect(screen.getByText("Protected current vault")).toBeTruthy();
    if (stage === "reservation") expect(calls("vault_unlock")).toHaveLength(0);
  });
}

for (const payload of [undefined, null, {}, { sessionId: null }, { sessionId: 9_007_199_254_740_992 }, { sessionId: "private-path-not-a-uuid" }]) {
  test(`malformed native lock payload ${JSON.stringify(payload)} is explicit and attempts only the owned native lock`, async () => {
    const f = fixture();
    render(<App />);
    await screen.findByText("Protected current vault");
    await act(async () => native.onLock?.({ payload }));
    expect(screen.queryByText("Protected current vault")).toBeNull();
    expect(screen.getByText(NATIVE_PROTOCOL_ERROR)).toBeTruthy();
    expect(calls("vault_lock")[0][1]).toEqual({ sessionId: f.a });
    expect(f.active()).toBe(false);
    expect(document.body.textContent).not.toContain("private-path-not-a-uuid");
  });
}

test("malformed notification plus a refused owned lock never claims native safety", async () => {
  fixture();
  render(<App />);
  await screen.findByText("Protected current vault");
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_lock") throw { kind: "locked", message: "A different native owner is active." };
    throw new Error(command);
  });
  await act(async () => native.onLock?.({ payload: {} }));
  expect(screen.queryByText("Protected current vault")).toBeNull();
  expect(document.body.textContent).toContain("Could not confirm the native lock. Close AKM");
});

for (const reservation of [undefined, 9_007_199_254_740_992, "not-a-native-uuid"]) {
  test(`invalid reservation ${String(reservation)} never receives credentials`, async () => {
    const f = fixture();
    await beginB(f, "unlock");
    await act(async () => f.prepare.resolve(reservation));
    expect(calls("vault_unlock")).toHaveLength(0);
    expect(screen.getByText(NATIVE_PROTOCOL_ERROR)).toBeTruthy();
    expect(screen.queryByText("Protected current vault")).toBeNull();
  });
}

for (const mode of ["unlock", "create"] as const) {
  for (const id of [null, "malformed", "foreign"] as const) {
    test(`a ${mode} status with ${String(id)} owner is rejected without losing a known create receipt`, async () => {
      const f = fixture();
      await beginB(f, mode);
      await acknowledge(f, mode);
      f.installed();
      await act(async () => f.reply.resolve({ ...f.completed, sessionId: id === "foreign" ? crypto.randomUUID() : id }));
      expect(screen.queryByText("Protected current vault")).toBeNull();
      expect(screen.getByText(NATIVE_PROTOCOL_ERROR)).toBeTruthy();
      expect(retainedCompletions()).toHaveLength(mode === "create" ? 1 : 0);
      expect(document.body.textContent).not.toContain("Private B");
    });
  }
}

for (const success of [true, false]) {
  test(`a delayed A owned-lock ${success ? "success" : "refusal"} cannot invalidate B`, async () => {
    const f = fixture();
    render(<App />);
    await screen.findByText("Protected current vault");
    const held = deferred<void>();
    native.invoke.mockImplementationOnce((command: string, args: unknown) => {
      expect(command).toBe("vault_lock");
      expect(args).toEqual({ sessionId: f.a });
      return held.promise;
    });
    const oldLock = api.lock().catch((error: unknown) => error);
    let opening!: Promise<Status>;
    act(() => { opening = api.unlock(f.completed.path, "fake-current-password"); });
    await acknowledge(f, "unlock");
    f.installed();
    await act(async () => { f.reply.resolve(f.completed); await opening; });
    if (success) await act(async () => held.resolve());
    else await act(async () => held.reject({ kind: "locked", message: "private obsolete lock refusal" }));
    expect(await oldLock).toMatchObject({ kind: "locked" });
    expect(nativeOwner()).toBe(f.b);
    expect(f.active()).toBe(true);
  });
}
