import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Settings } from "./Settings";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";

const mocks = vi.hoisted(() => ({
  backup: vi.fn().mockResolvedValue({ exists: false }),
  keys: vi.fn().mockResolvedValue([]),
  importEnv: vi.fn().mockResolvedValue({ added: 1, updated: 1 }),
  exportEnv: vi.fn(),
}));

vi.mock("../api", () => ({
  api: mocks,
  errorText: (error: unknown) => (error && typeof error === "object" && "message" in error ? String((error as { message: unknown }).message) : String(error)),
}));

beforeEach(() => vi.clearAllMocks());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const status = { sessionId: crypto.randomUUID(), path: "C:/test/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Test", needsUpgrade: false };

function fillImport(text = "TOKEN=fake", project = "demo", environment = "dev") {
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  for (const [label, value] of [["Paste the .env contents", text], ["Target project", project], ["Target environment", environment]]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
}
function hide() {
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  hidden.mockReturnValue(false);
}

test("export controls stay locked until a specific project and environment are selected", () => {
  render(<Settings status={status} onLocked={() => {}} />);
  for (const name of ["Save .env…", "Preview", "Copy to clipboard"]) {
    expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  }
});

test("import sends one request with the chosen scope and has no in-page confirm step", async () => {
  render(<Settings status={status} onLocked={() => {}} />);
  fillImport("OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret");
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Imported: 1 added, 1 updated.");
  expect(mocks.importEnv).toHaveBeenCalledExactlyOnceWith("OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret", "demo", "dev");
  expect(screen.queryByRole("button", { name: "Confirm import" })).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.body.textContent).not.toContain("dev-secret");
});

test("import refuses a missing target scope before asking the backend", async () => {
  render(<Settings status={status} onLocked={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "KEY=secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Choose the target project and environment. Imports never search other scopes.");
  expect(mocks.importEnv).not.toHaveBeenCalled();
});

test("while the native confirmation is open the form cannot be edited, cancelled or closed from the page", async () => {
  let resolve!: (value: unknown) => void;
  mocks.importEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<Settings status={status} onLocked={() => {}} />);
  fillImport();
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  expect(screen.getByText(/Waiting for your answer in the AKM confirmation window/)).toBeTruthy();
  for (const label of ["Paste the .env contents", "Target project", "Target environment"]) {
    expect((screen.getByLabelText(label) as HTMLInputElement).disabled).toBe(true);
  }
  expect((screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.getByRole("dialog")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Waiting for confirmation…" }));
  expect(mocks.importEnv).toHaveBeenCalledTimes(1);
  await act(async () => resolve({ added: 2, updated: 0 }));
  await screen.findByText("Imported: 2 added, 0 updated.");
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("declining in the native window keeps the form for editing and reports that nothing was imported", async () => {
  mocks.importEnv.mockResolvedValueOnce(null);
  render(<Settings status={status} onLocked={() => {}} />);
  fillImport("TOKEN=fake-kept");
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Nothing was imported. You declined it in the confirmation window.");
  expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("TOKEN=fake-kept");
  expect((screen.getByRole("button", { name: "Review import" }) as HTMLButtonElement).disabled).toBe(false);
  expect(screen.queryByText(/Imported:/)).toBeNull();
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "TOKEN=fake-edited" } });
  expect(screen.queryByText(/declined it/)).toBeNull();
});

test("a native error is shown and the form can be used again", async () => {
  mocks.importEnv.mockRejectedValueOnce({ kind: "stalePreview", message: "The vault changed while the import waited for confirmation. Start it again." });
  render(<Settings status={status} onLocked={() => {}} />);
  fillImport();
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findAllByText("The vault changed while the import waited for confirmation. Start it again.");
  expect((screen.getByRole("button", { name: "Review import" }) as HTMLButtonElement).disabled).toBe(false);
});

for (const how of ["hide", "native view expiry"] as const) {
  test(`${how} while the native confirmation is open closes the form and ignores the late reply`, async () => {
    let resolve!: (value: unknown) => void;
    mocks.importEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<Settings status={status} onLocked={() => {}} />);
    fillImport("TOKEN=fake-private-text");
    fireEvent.click(screen.getByRole("button", { name: "Review import" }));
    if (how === "hide") hide();
    else act(() => { window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT)); });
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(async () => resolve({ added: 1, updated: 0 }));
    // The API layer owns the outcome record. This view must show nothing of the late reply.
    expect(screen.queryByText("Imported: 1 added, 0 updated.")).toBeNull();
    expect((screen.getByRole("button", { name: "Import .env…" }) as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
    expect((screen.getByLabelText("Paste the .env contents") as HTMLTextAreaElement).value).toBe("");
    expect(mocks.importEnv).toHaveBeenCalledTimes(1);
  });
}

test("unmounting while the native confirmation is open leaves nothing to update", async () => {
  let resolve!: (value: unknown) => void;
  mocks.importEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const view = render(<Settings status={status} onLocked={() => {}} />);
  fillImport();
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  view.unmount();
  await act(async () => resolve({ added: 1, updated: 0 }));
  expect(mocks.keys).toHaveBeenCalledTimes(1); // the initial load only
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

test("hiding the window invalidates a plaintext preview that has not arrived yet", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }]);
  let resolve!: (value: string) => void;
  mocks.exportEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await act(async () => { render(<Settings status={status} onLocked={() => {}} />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  hide();
  await act(async () => resolve("TOKEN=fake-hidden-secret\n"));
  expect(screen.queryByText("TOKEN=fake-hidden-secret")).toBeNull();
});
