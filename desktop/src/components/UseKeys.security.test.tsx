import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { UseKeys } from "./UseKeys";
import { adoptReservation, PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { acknowledgeCompletions, invalidateSession, retainedCompletions } from "../sessionBoundary";

const mocks = vi.hoisted(() => ({
  keys: vi.fn().mockResolvedValue([]),
  importEnv: vi.fn().mockResolvedValue({ added: 1, updated: 1 }),
  exportEnv: vi.fn(),
  copyEnv: vi.fn(),
  saveEnv: vi.fn(),
  launchTerminal: vi.fn(),
}));

// The real api, boundary and receipts run here, because the receipts own the wait of a dispatched
// operation (U2-R1-03). The Tauri IPC sends each command to its mock.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    switch (command) {
      case "keys_list": return mocks.keys();
      case "env_import": return mocks.importEnv(args.text, args.project, args.environment);
      case "env_launch_terminal": return mocks.launchTerminal(args.project, args.environment);
      case "env_export": return mocks.exportEnv(args.project, args.environment);
      case "env_copy": return mocks.copyEnv(args.project, args.environment);
      case "env_save": return mocks.saveEnv(args.project, args.environment);
      case "vault_dismiss_warnings": return Promise.resolve();
      default: return Promise.reject(new Error(`Unexpected command ${command}`));
    }
  },
}));

beforeEach(() => { vi.clearAllMocks(); adoptReservation(crypto.randomUUID()); });
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  // A request that never answered keeps its receipt, so the next test starts a new session.
  invalidateSession();
  acknowledgeCompletions(retainedCompletions().map(({ id }) => id));
});


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

test("a failed list read can be tried again, and its error goes away after success", async () => {
  mocks.keys.mockRejectedValueOnce({ message: "fake read failure" });
  await act(async () => { render(<UseKeys />); });
  await act(async () => {});
  expect(document.body.textContent).toContain("fake read failure");
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "dev" }]);
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Try again" })); });
  await act(async () => {});
  expect(document.body.textContent).not.toContain("fake read failure");
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getAllByRole("option", { name: "demo" }).length).toBeGreaterThan(0);
});

test("export controls stay locked until a specific project and environment are selected", () => {
  render(<UseKeys />);
  for (const name of ["Save .env…", "Preview", "Copy to clipboard"]) {
    expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  }
});

test("import sends one request with the chosen scope and has no in-page confirm step", async () => {
  render(<UseKeys />);
  fillImport("OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret");
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Imported: 1 added, 1 updated.");
  expect(mocks.importEnv).toHaveBeenCalledExactlyOnceWith("OPENAI_API_KEY=dev-secret\nSTRIPE_KEY=second-secret", "demo", "dev");
  expect(screen.queryByRole("button", { name: "Confirm import" })).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.body.textContent).not.toContain("dev-secret");
});

test("import refuses a missing target scope before asking the backend", async () => {
  render(<UseKeys />);
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
  fireEvent.change(screen.getByLabelText("Paste the .env contents"), { target: { value: "KEY=secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findByText("Choose the target project and environment. Imports never search other scopes.");
  expect(mocks.importEnv).not.toHaveBeenCalled();
});

test("while the native confirmation is open the form cannot be edited, cancelled or closed from the page", async () => {
  let resolve!: (value: unknown) => void;
  mocks.importEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  render(<UseKeys />);
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
  render(<UseKeys />);
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
  render(<UseKeys />);
  fillImport();
  fireEvent.click(screen.getByRole("button", { name: "Review import" }));
  await screen.findAllByText("The vault changed while the import waited for confirmation. Start it again.");
  expect((screen.getByRole("button", { name: "Review import" }) as HTMLButtonElement).disabled).toBe(false);
});

for (const how of ["hide", "native view expiry"] as const) {
  test(`${how} while the native confirmation is open closes the form and ignores the late reply`, async () => {
    let resolve!: (value: unknown) => void;
    mocks.importEnv.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<UseKeys />);
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
  const view = render(<UseKeys />);
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
  await act(async () => { render(<UseKeys />); });
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
  await act(async () => { render(<UseKeys />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  hide();
  await act(async () => resolve("TOKEN=fake-hidden-secret\n"));
  expect(screen.queryByText("TOKEN=fake-hidden-secret")).toBeNull();
});

const TERMINAL = "Open terminal with keys…";
const terminalButton = () => screen.getByRole("button", { name: TERMINAL }) as HTMLButtonElement;

async function chooseScope(project = "demo", environment = "dev") {
  mocks.keys.mockResolvedValueOnce([{ project, environment }]);
  await act(async () => { render(<UseKeys />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: project } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: environment } });
}

test("the terminal button stays disabled until a specific project and environment are chosen", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "dev" }]);
  await act(async () => { render(<UseKeys />); });
  expect(terminalButton().disabled).toBe(true);
  expect(terminalButton().className).toContain("btn-secondary");
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  expect(terminalButton().disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
  expect(terminalButton().disabled).toBe(false);
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "" } });
  expect(terminalButton().disabled).toBe(true);
});

test("opening a terminal sends only the scope and says how many keys the window holds", async () => {
  mocks.launchTerminal.mockResolvedValueOnce(2);
  await chooseScope();
  fireEvent.click(terminalButton());
  await screen.findByText("Opened a terminal with 2 keys. That window keeps them until you close it, even after AKM locks.");
  expect(mocks.launchTerminal).toHaveBeenCalledExactlyOnceWith("demo", "dev");
  expect(document.body.textContent).not.toContain("fake-");
  expect(terminalButton().disabled).toBe(false);
});

test("a cancel in a native window says that nothing was opened", async () => {
  mocks.launchTerminal.mockResolvedValueOnce(null);
  await chooseScope();
  fireEvent.click(terminalButton());
  await screen.findByText("Nothing was opened. You cancelled in the folder window or the confirmation window.");
  expect(screen.queryByText(/Opened a terminal/)).toBeNull();
});

test("a native error is shown and the button can be used again", async () => {
  mocks.launchTerminal.mockRejectedValueOnce({ kind: "tooLarge", message: "This scope has more than 25 keys." });
  await chooseScope();
  fireEvent.click(terminalButton());
  await screen.findByText("This scope has more than 25 keys.");
  expect(terminalButton().disabled).toBe(false);
});

test("while the native windows are open every export control is locked and a second launch is impossible", async () => {
  let resolve!: (value: unknown) => void;
  mocks.launchTerminal.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  await chooseScope();
  fireEvent.click(terminalButton());
  expect(terminalButton().getAttribute("aria-busy")).toBe("true");
  for (const name of ["Save .env…", "Preview", "Copy to clipboard", TERMINAL]) {
    expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(true);
  }
  expect((screen.getByLabelText("Project") as HTMLSelectElement).disabled).toBe(true);
  expect(screen.getByText(/Choose the folder and answer the confirmation window/)).toBeTruthy();
  fireEvent.click(terminalButton());
  expect(mocks.launchTerminal).toHaveBeenCalledTimes(1);
  await act(async () => resolve(1));
  await screen.findByText(/Opened a terminal with 1 key\./);
});

for (const how of ["hide", "native view expiry", "scope change", "unmount"] as const) {
  test(`${how} while the native windows are open ignores the late reply`, async () => {
    let resolve!: (value: unknown) => void;
    mocks.launchTerminal.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "dev" }, { project: "demo", environment: "prod" }]);
    let view!: ReturnType<typeof render>;
    await act(async () => { view = render(<UseKeys />); });
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
    fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "dev" } });
    fireEvent.click(terminalButton());
    if (how === "hide") hide();
    else if (how === "native view expiry") act(() => { window.dispatchEvent(new CustomEvent(PROTECTED_VIEW_EXPIRY_EVENT)); });
    else if (how === "scope change") fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
    else view.unmount();
    await act(async () => resolve(3));
    expect(document.body.textContent).not.toContain("Opened a terminal");
    if (how !== "unmount") {
      expect(terminalButton().getAttribute("aria-busy")).toBeNull();
      expect(screen.queryByText(/Choose the folder and answer/)).toBeNull();
    }
  });
}

test("a failed launch after a hide shows no error either", async () => {
  let reject!: (reason: unknown) => void;
  mocks.launchTerminal.mockReturnValueOnce(new Promise((_, no) => { reject = no; }));
  await chooseScope();
  fireEvent.click(terminalButton());
  hide();
  await act(async () => reject({ kind: "launch", message: "The terminal could not be started." }));
  expect(screen.queryByText("The terminal could not be started.")).toBeNull();
});

test("a preview that the user declined in the native window shows a cancel message and no values", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }]);
  mocks.exportEnv.mockResolvedValueOnce(null);
  await act(async () => { render(<UseKeys />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Preview" })); });
  expect(document.body.textContent).toContain("Cancelled. No values were shown.");
  expect(document.body.textContent).not.toContain("Previewed");
  expect(screen.queryByText("Hide preview")).toBeNull();
});

test("a copy that the user declined in the native window says that nothing was copied", async () => {
  mocks.keys.mockResolvedValueOnce([{ project: "demo", environment: "prod" }]);
  mocks.copyEnv.mockResolvedValueOnce(null);
  await act(async () => { render(<UseKeys />); });
  fireEvent.change(screen.getByLabelText("Project"), { target: { value: "demo" } });
  fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "prod" } });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Copy to clipboard" })); });
  expect(document.body.textContent).toContain("Cancelled. Nothing was copied.");
  expect(document.body.textContent).not.toContain("clipboard in");
});
