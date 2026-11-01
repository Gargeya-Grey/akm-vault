import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import React from "react";
import { App } from "W:/API Keys Manager/desktop/src/App.tsx";
import { api } from "W:/API Keys Manager/desktop/src/api.ts";
import { Activity } from "W:/API Keys Manager/desktop/src/components/Activity.tsx";
import { COMPLETION_MESSAGE } from "W:/API Keys Manager/desktop/src/sessionBoundary.ts";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("W:/API Keys Manager/desktop/src/theme.ts", () => ({ useTheme: () => ({ theme: "light", toggle: vi.fn() }) }));
vi.mock("W:/API Keys Manager/desktop/src/components/KeysView.tsx", () => ({ KeysView: () => <p>Active B region</p> }));
afterEach(cleanup);

const status = { path: "C:/fake/b.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "B", needsUpgrade: false };

test("a drained application warning delivered between roots is lost while an operation notice is replayed", async () => {
  let resolveOld!: (value: unknown) => void;
  let warningCalls = 0;
  const notice = { id: "fake-notice", originSession: "A", kind: "journal" };
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status;
    if (command === "vault_warnings") {
      warningCalls++;
      if (warningCalls === 1) return new Promise((resolve) => { resolveOld = resolve; });
      return { application: [], operations: [notice] };
    }
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  const first = render(<App />);
  await screen.findByText("Active B region");
  first.unmount();
  await act(async () => {
    resolveOld({ application: ["FAKE application safety warning"], operations: [notice] });
  });
  render(<App />);
  await screen.findByText("Active B region");
  expect(screen.getByText(/activity entry could not be confirmed/)).toBeTruthy();
  expect(screen.queryByText("FAKE application safety warning")).toBeNull();
});

test("a completed-while-locked result delivered between roots loses the completion receipt", async () => {
  let rejectSave!: (error: unknown) => void;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "vault_status") return status;
    if (command === "vault_warnings") return { application: [], operations: [] };
    if (command === "env_save") return new Promise((_resolve, reject) => { rejectSave = reject; });
    if (command === "vault_activity") return;
    throw new Error(command);
  });
  const first = render(<App />);
  await screen.findByText("Active B region");
  const completed = api.saveEnv("fake-project", "fake-env").catch((error: unknown) => error);
  first.unmount();
  let result: any;
  await act(async () => {
    rejectSave({ kind: "completedWhileLocked", message: "fake late completion" });
    result = await completed;
  });
  expect(result.kind).toBe("completedWhileLocked");
  render(<App />);
  await screen.findByText("Active B region");
  expect(screen.queryByText(COMPLETION_MESSAGE)).toBeNull();
});

test("a valid unrecognized action named __proto__ crashes Activity instead of showing unknown outcome", async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "audit_read") return { path: "C:/fake/journal", state: "consistent", brokenAt: null, problemLine: null, totalEntries: 1, entries: [{ seq: 1, ts: "2026-10-31T00:00:00Z", action: "__proto__", keyTitle: null, project: null, environment: null, detail: null }] };
    throw new Error(command);
  });
  class Boundary extends React.Component<{ children: React.ReactNode }, { failed: boolean }> {
    state = { failed: false };
    static getDerivedStateFromError() { return { failed: true }; }
    render() { return this.state.failed ? <p>Activity render failed</p> : this.props.children; }
  }
  const errors = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    render(<Boundary><Activity /></Boundary>);
    await screen.findByText("Activity render failed");
    expect(screen.queryByText("Legacy __proto__ (outcome unknown)")).toBeNull();
  } finally {
    errors.mockRestore();
  }
});
