import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Settings } from "./Settings";

const mocks = vi.hoisted(() => ({
  backup: vi.fn().mockResolvedValue({ exists: false }),
  keys: vi.fn().mockResolvedValue([]),
  previewImport: vi.fn().mockResolvedValue({
    added: 1, updated: 1, unchanged: 0, token: "single-use-token",
    changes: [
      { name: "OPENAI_API_KEY", title: "OpenAI dev", action: "update" },
      { name: "STRIPE_KEY", title: "STRIPE_KEY", action: "add" },
    ],
  }),
  confirmImport: vi.fn().mockResolvedValue({ added: 1, updated: 1 }),
  exportEnv: vi.fn(),
}));

vi.mock("../api", () => ({
  api: mocks,
  errorText: (error: unknown) => String(error),
}));

beforeEach(() => vi.clearAllMocks());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const status = { sessionId: crypto.randomUUID(), path: "C:/test/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Test", needsUpgrade: false };

test("export controls stay locked until a specific project and environment are selected", () => {
  render(
    <Settings
      status={status}
      onLocked={() => {}}
    />,
  );
  for (const name of ["Save .env…", "Preview", "Copy to clipboard"]) {
    expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  }
});

test("import previews the chosen scope without writing, then confirms only the returned token", async () => {
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret" } });
  fireEvent.change(screen.getByLabelText("Target project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Target environment"), { target: { value: "dev" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("OpenAI dev");
  expect(mocks.previewImport).toHaveBeenCalledWith("OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret", "demo", "dev");
  expect(mocks.confirmImport).not.toHaveBeenCalled();
  expect(screen.queryByText("dev-secret")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Confirm import" }));
  await screen.findByText("Imported: 1 added, 1 updated.");
  expect(mocks.confirmImport).toHaveBeenCalledExactlyOnceWith("single-use-token");
});

test("import refuses a missing target scope before asking the backend for a preview", async () => {
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "KEY=secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Choose the target project and environment. Imports never search other scopes.");
  expect(mocks.previewImport).not.toHaveBeenCalled();
  expect(mocks.confirmImport).not.toHaveBeenCalled();
});

test("a preview response cannot bind an old prod token to an edited dev label", async () => {
  let resolve!: (value: unknown) => void;
  mocks.previewImport.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake" } });
  fireEvent.change(screen.getByLabelText("Target project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Target environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  expect(mocks.previewImport).toHaveBeenCalledWith("TOKEN=fake", "demo", "prod");
  fireEvent.change(screen.getByLabelText("Target environment"), { target: { value: "dev" } });
  await act(async () => resolve({ token: "prod-token", added: 1, updated: 0, unchanged: 0, changes: [] }));
  expect(screen.queryByRole("region", { name: "Import review" })).toBeNull();
  expect(mocks.confirmImport).not.toHaveBeenCalled();
});

test("changing export scope rejects an old plaintext preview response", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }, { project: "demo", environment: "dev" }]);
  let resolve!: (value: string) => void;
  mocks.exportEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await act(async () => { render(<Settings status={status} onLocked={() => {}} />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  expect(mocks.exportEnv).toHaveBeenCalledWith("demo", "prod");
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  await act(async () => resolve("TOKEN=fake-prod-secret\n"));
  expect(screen.queryByText("TOKEN=fake-prod-secret")).toBeNull();
});

test("cancelling an import preview invalidates its token even after reopening the dialog", async () => {
  let resolve!: (value: unknown) => void;
  mocks.previewImport.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "prod"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  expect((screen.getByLabelText("Target environment") as HTMLInputElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  await act(async () => resolve({ token: "cancelled-token", added: 1, updated: 0, unchanged: 0, changes: [] }));
  expect(screen.queryByRole("region", { name: "Import review" })).toBeNull();
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("");
  expect(mocks.confirmImport).not.toHaveBeenCalled();
});

test("hiding the window invalidates a plaintext preview that has not arrived yet", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }]);
  let resolve!: (value: string) => void;
  mocks.exportEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await act(async () => { render(<Settings status={status} onLocked={() => {}} />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  await act(async () => resolve("TOKEN=fake-hidden-secret\n"));
  expect(screen.queryByText("TOKEN=fake-hidden-secret")).toBeNull();
});

test("hiding invalidates a pending import review and its token", async () => {
  let resolve!: (value: unknown) => void;
  mocks.previewImport.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "prod"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  await act(async () => resolve({ token: "hidden-token", added: 1, updated: 0, unchanged: 0, changes: [] }));
  expect(screen.queryByRole("region", { name: "Import review" })).toBeNull();
  expect(mocks.confirmImport).not.toHaveBeenCalled();
});

test("hiding an already-dispatched confirmation expires local details without leaving its controls busy", async () => {
  let resolve!: (value: unknown) => void;
  mocks.confirmImport.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", "TOKEN=fake"], ["Target project", "demo"], ["Target environment", "dev"]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByRole("button", { name: "Confirm import" });
  fireEvent.click(screen.getByRole("button", { name: "Confirm import" }));
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
  await act(async () => resolve({ added: 1, updated: 0 }));
  expect(screen.queryByRole("dialog")).toBeNull();
  // The real API/App handoff suite checks retained acknowledgment. This mocked view owns no outcomes.
  expect(screen.queryByText("Imported: 1 added, 0 updated.")).toBeNull();
  expect((screen.getByRole("button", { name: "Import .env…" }) as HTMLButtonElement).disabled).toBe(false);
});
