import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";

const native = vi.hoisted(() => ({ active: true, path: "C:/fake/a.kdbx", owner: "", onLock: undefined as undefined | ((event: { payload: { sessionId: string } }) => void), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (event: string, callback: typeof native.onLock) => { if (event !== "vault-locked") return () => {}; native.onLock = callback; return () => { native.onLock = undefined; }; }) }));
vi.mock("./theme", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("./components/KeysView", () => ({ KeysView: () => <p>Active vault view</p> }));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function emit(owner: string) { native.onLock?.({ payload: { sessionId: owner } }); }

for (const timing of ["after-open", "before-open-reply"] as const) {
  test(`an old A native event ${timing} cannot false-lock B`, async () => {
    native.active = true;
    native.path = "C:/fake/a.kdbx";
    const a = crypto.randomUUID();
    const b = crypto.randomUUID();
    native.owner = a;
    let finish!: (value: unknown) => void;
    const status = () => ({ path: native.path, exists: true, unlocked: native.active, keyCount: 0, vaultName: native.active ? "Fake" : null, needsUpgrade: false, sessionId: native.active ? native.owner : null });
    native.invoke.mockReset().mockImplementation(async (command: string, args?: { path?: string; sessionId?: string }) => {
      if (command === "vault_status") return status();
      if (command === "vault_warnings") return { application: [], operations: [] };
      if (command === "vault_activity") return;
      if (command === "vault_lock") { if (args?.sessionId && args.sessionId !== native.owner) throw { kind: "locked" }; native.active = false; return; }
      if (command === "vault_prepare_open") { native.owner = b; return b; }
      if (command === "vault_unlock") {
        native.path = args?.path ?? native.path;
        native.owner = b;
        native.active = true;
        if (timing === "before-open-reply") return new Promise((resolve) => { finish = resolve; });
        return status();
      }
      throw new Error(command);
    });
    render(<App />);
    await screen.findByText("Active vault view");
    fireEvent.click(screen.getByRole("button", { name: "Lock now" }));
    expect(native.active).toBe(false);
    fireEvent.change(screen.getByLabelText("Vault file"), { target: { value: "C:/fake/b.kdbx" } });
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: "fake-password-b" } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock vault" }));
    await act(async () => {});
    expect(native.active).toBe(true);
    if (timing === "after-open") await screen.findByText("Active vault view");
    act(() => emit(a));
    if (timing === "before-open-reply") await act(async () => finish(status()));
    expect(screen.getByText("Active vault view")).toBeTruthy();
    expect(native.active).toBe(true);
    expect(native.path).toBe("C:/fake/b.kdbx");
    // A genuine B notification must still close the protected view immediately.
    act(() => { native.active = false; emit(b); });
    expect(screen.queryByText("Active vault view")).toBeNull();
    expect(screen.getByRole("button", { name: "Unlock vault" })).toBeTruthy();
  });
}
