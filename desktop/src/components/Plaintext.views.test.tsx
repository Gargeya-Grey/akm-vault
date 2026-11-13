import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { KeyDetail } from "./KeyDetail";
import { UseKeys } from "./UseKeys";
import { api } from "../api";
import { adoptReservation } from "../nativeSession";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "../sessionBoundary";
import type { KeyView } from "../types";

// U2-R1-04, contract 4: plaintext shows only for the request that asked for it, while its
// session, the vault data and its scope hold. The real api, boundary and sealed values run here;
// only the Tauri IPC is mocked, and each command answers from a queue the test controls.

type Deferred = { resolve: (value: unknown) => void; reject: (reason: unknown) => void };
const native = vi.hoisted(() => ({ queues: new Map<string, Deferred[]>(), auto: new Map<string, unknown>(), calls: [] as string[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) => {
    native.calls.push(command);
    if (native.auto.has(command)) return Promise.resolve(native.auto.get(command));
    if (command === "vault_dismiss_warnings") return Promise.resolve();
    return new Promise((resolve, reject) => {
      const queue = native.queues.get(command) ?? [];
      queue.push({ resolve, reject });
      native.queues.set(command, queue);
    });
  }),
}));

beforeEach(() => {
  native.queues.clear();
  native.auto.clear();
  native.calls = [];
  adoptReservation(crypto.randomUUID());
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  invalidateSession();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
});

async function answer(command: string, how: "resolve" | "reject", value: unknown) {
  const next = native.queues.get(command)?.shift();
  if (!next) throw new Error(`no pending ${command}`);
  await act(async () => { next[how](value); });
  await act(async () => {});
}
const SCOPE = [{ id: "fake-key", title: "Fake", project: "demo", environment: "dev" }];
const OLD = "FAKE_TOKEN=fake-old-plaintext";
const shows = (text: string) => (document.body.textContent ?? "").includes(text);
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

async function previewShown() {
  await act(async () => { render(<UseKeys />); });
  await answer("keys_list", "resolve", SCOPE);
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  await act(async () => { fireEvent.click(button("Preview")); });
  await answer("env_export", "resolve", `${OLD}\n`);
  expect(shows(OLD)).toBe(true);
}
async function importOnce() {
  fireEvent.click(button("Import .env…"));
  for (const [label, value] of [["Paste the .env contents", "FAKE_TOKEN=fake-new-plaintext"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  await act(async () => { fireEvent.click(button("Review import")); });
}

test("the review's sequence: an import, a failed list read and a retry never bring the old preview back", async () => {
  await previewShown();
  await importOnce();
  expect(shows(OLD)).toBe(false);
  await answer("env_import", "resolve", { added: 0, updated: 1 });
  await answer("keys_list", "reject", { kind: "io", message: "fake list failure" });
  expect(shows(OLD)).toBe(false);
  await act(async () => { fireEvent.click(button("Try again")); });
  await answer("keys_list", "resolve", SCOPE);
  expect(shows(OLD)).toBe(false);
  expect(native.calls.filter((command) => command === "env_export")).toHaveLength(1);
  expect(screen.getByText("No preview yet.")).toBeTruthy();
});

test("a preview reply that was asked for before an import is dropped", async () => {
  await act(async () => { render(<UseKeys />); });
  await answer("keys_list", "resolve", SCOPE);
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  await act(async () => { fireEvent.click(button("Preview")); });
  // The import is a change of vault data, made through the api while the preview waits.
  native.auto.set("env_import", { added: 0, updated: 1 });
  await act(async () => { await api.importEnv("FAKE_TOKEN=fake-new-plaintext", "demo", "dev"); });
  await answer("env_export", "resolve", `${OLD}\n`);
  expect(shows(OLD)).toBe(false);
  expect(screen.getByText("No preview yet.")).toBeTruthy();
});

test("a change of vault data in another scope hides the preview, and it stays hidden", async () => {
  await previewShown();
  native.auto.set("env_import", { added: 0, updated: 1 });
  await act(async () => { await api.importEnv("FAKE_OTHER=fake", "other", "dev"); });
  expect(shows(OLD)).toBe(false);
  await act(async () => {});
  expect(shows(OLD)).toBe(false);
});

test("a preview asked for while an import is on its way is dropped when the import ends", async () => {
  await act(async () => { render(<UseKeys />); });
  await answer("keys_list", "resolve", SCOPE);
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  let imported!: Promise<unknown>;
  act(() => { imported = api.importEnv("FAKE_TOKEN=fake-new-plaintext", "demo", "dev"); });
  await act(async () => { fireEvent.click(button("Preview")); });
  await answer("env_import", "resolve", { added: 0, updated: 1 });
  await act(async () => { await imported; });
  await answer("env_export", "resolve", `${OLD}\n`);
  expect(shows(OLD)).toBe(false);
  expect(screen.getByText("No preview yet.")).toBeTruthy();
  expect(screen.getByText(/The keys changed before the preview arrived/)).toBeTruthy();
});

test("the preview hides after 30 seconds, and only a new preview shows values again", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  await previewShown();
  await act(async () => { vi.advanceTimersByTime(29_000); });
  expect(shows(OLD)).toBe(true);
  await act(async () => { vi.advanceTimersByTime(1_000); });
  expect(shows(OLD)).toBe(false);
  expect(screen.getByText("No preview yet.")).toBeTruthy();
});

test("a lock hides the preview", async () => {
  await previewShown();
  act(() => { invalidateSession(); });
  expect(shows(OLD)).toBe(false);
});

// The revealed secret of a key.

function key(): KeyView {
  return {
    id: "fake-key", title: "Fake", username: null, issuer: null, project: "demo", environment: "dev", scopes: [],
    envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [], expiresAt: null, rotationDays: null,
    createdAt: null, updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
  };
}
const SECRET = "fake-revealed-secret";
async function detail() {
  await act(async () => { render(<KeyDetail current item={key()} onClose={() => {}} onEdit={() => {}} onDelete={() => {}} onRotate={() => {}} />); });
}
const rotate = () => api.rotate("fake-key", "fake-new-secret");

test("a revealed secret hides when a rotation of the key leaves, and stays hidden after its reply", async () => {
  await detail();
  await act(async () => { fireEvent.click(button("Reveal")); });
  await answer("key_reveal", "resolve", SECRET);
  expect(shows(SECRET)).toBe(true);
  let rotation!: Promise<unknown>;
  act(() => { rotation = rotate(); });
  expect(shows(SECRET)).toBe(false);
  await answer("key_rotate", "resolve", { key: key(), changedAt: "2026-11-13T10:00:00Z", previousChangedAt: null });
  await act(async () => { await rotation; });
  expect(shows(SECRET)).toBe(false);
  expect(button("Reveal")).toBeTruthy();
});

test("a reveal reply that was asked for before a rotation is dropped", async () => {
  await detail();
  await act(async () => { fireEvent.click(button("Reveal")); });
  native.auto.set("key_rotate", { key: key(), changedAt: "2026-11-13T10:00:00Z", previousChangedAt: null });
  await act(async () => { await rotate(); });
  await answer("key_reveal", "resolve", SECRET);
  expect(shows(SECRET)).toBe(false);
  expect(button("Reveal").disabled).toBe(false);
});

test("a reveal asked for while a rotation is on its way is dropped when the rotation ends", async () => {
  await detail();
  let rotation!: Promise<unknown>;
  act(() => { rotation = rotate(); });
  await act(async () => { fireEvent.click(button("Reveal")); });
  await answer("key_rotate", "resolve", { key: key(), changedAt: "2026-11-13T10:00:00Z", previousChangedAt: null });
  await act(async () => { await rotation; });
  await answer("key_reveal", "resolve", SECRET);
  expect(shows(SECRET)).toBe(false);
  expect(button("Reveal").disabled).toBe(false);
});

test("a lock hides a revealed secret", async () => {
  await detail();
  await act(async () => { fireEvent.click(button("Reveal")); });
  await answer("key_reveal", "resolve", SECRET);
  act(() => { invalidateSession(); });
  expect(shows(SECRET)).toBe(false);
});
