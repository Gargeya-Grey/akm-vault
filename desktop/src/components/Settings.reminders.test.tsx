import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Settings } from "./Settings";

// The key count and the backup details of Settings, and their failed and pending reads.

const mocks = vi.hoisted(() => ({
  backup: vi.fn().mockResolvedValue({ exists: false }),
  keys: vi.fn().mockResolvedValue([]),
}));
vi.mock("../native", () => ({ reads: mocks }));
vi.mock("../api", () => ({
  api: mocks,
  errorText: (error: unknown) => (error && typeof error === "object" && "message" in error ? String((error as { message: unknown }).message) : String(error)),
}));

const status = { sessionId: crypto.randomUUID(), path: "C:/test/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Test", needsUpgrade: false };

beforeEach(() => {
  vi.clearAllMocks();
  mocks.backup.mockResolvedValue({ exists: false });
  mocks.keys.mockResolvedValue([]);
});
afterEach(cleanup);

async function open() {
  await act(async () => { render(<Settings status={status} onLocked={() => {}} />); });
}

test("the key count comes from the current key list, not from the last status poll", async () => {
  mocks.keys.mockResolvedValue([{}, {}, {}]);
  await open();
  expect(screen.getByText("in this vault").textContent).toBe("3 in this vault");
});

test("a newer status count makes Settings read the list again and show the new count", async () => {
  mocks.keys.mockResolvedValueOnce([{}]).mockResolvedValueOnce([{}, {}]);
  const view = render(<Settings status={{ ...status, keyCount: 1 }} onLocked={() => {}} />);
  await act(async () => {});
  expect(screen.getByText("in this vault").textContent).toBe("1 in this vault");
  await act(async () => { view.rerender(<Settings status={{ ...status, keyCount: 2 }} onLocked={() => {}} />); });
  await act(async () => {});
  expect(screen.getByText("in this vault").textContent).toBe("2 in this vault");
});

test("an older list reply that arrives after a newer status changes nothing", async () => {
  let older: (keys: object[]) => void = () => {};
  mocks.keys.mockImplementationOnce(() => new Promise((resolve) => { older = resolve; })).mockResolvedValueOnce([{}, {}]);
  const view = render(<Settings status={{ ...status, keyCount: 1 }} onLocked={() => {}} />);
  await act(async () => { view.rerender(<Settings status={{ ...status, keyCount: 2 }} onLocked={() => {}} />); });
  await act(async () => {});
  await act(async () => { older([{}]); });
  expect(screen.getByText("in this vault").textContent).toBe("2 in this vault");
});

// UI-R2-02 and LOOK-R3-02: one source for the count. The polled count only starts a new read.
const fact = (name: string) => screen.getByText(name, { selector: ".fact-key" }).nextElementSibling?.textContent ?? "";

test("while the replacement read is pending, Settings says that it is counting and shows no number", async () => {
  let finish: (keys: object[]) => void = () => {};
  mocks.keys.mockResolvedValueOnce([{}]).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const view = render(<Settings status={{ ...status, keyCount: 1 }} onLocked={() => {}} />);
  await act(async () => {});
  expect(fact("Keys")).toBe("1 in this vault");
  await act(async () => { view.rerender(<Settings status={{ ...status, keyCount: 2 }} onLocked={() => {}} />); });
  expect(fact("Keys")).toBe("Counting…");
  await act(async () => { finish([{}, {}, {}]); });
  expect(fact("Keys")).toBe("3 in this vault");
});

test("a failed list read shows no count, says so, and a retry that succeeds shows the count", async () => {
  const user = userEvent.setup({ delay: null });
  mocks.keys.mockResolvedValueOnce([{}]).mockRejectedValueOnce(new Error("fake read failure")).mockResolvedValueOnce([{}, {}, {}, {}, {}]);
  const view = render(<Settings status={{ ...status, keyCount: 1 }} onLocked={() => {}} />);
  await act(async () => {});
  await act(async () => { view.rerender(<Settings status={{ ...status, keyCount: 5 }} onLocked={() => {}} />); });
  await act(async () => {});
  expect(fact("Keys")).toContain("AKM could not count the keys. fake read failure");
  expect(fact("Keys")).not.toMatch(/\d in this vault/);
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await act(async () => {});
  expect(fact("Keys")).toBe("5 in this vault");
});

test("a retry of the count is disabled and busy while it runs", async () => {
  const user = userEvent.setup({ delay: null });
  let finish: (keys: object[]) => void = () => {};
  mocks.keys.mockRejectedValueOnce(new Error("fake read failure")).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await open();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  const retry = screen.getByRole("button", { name: "Trying again…" }) as HTMLButtonElement;
  expect(retry.disabled).toBe(true);
  expect(retry.getAttribute("aria-busy")).toBe("true");
  await act(async () => { finish([{}]); });
  expect(fact("Keys")).toBe("1 in this vault");
});

test("a count reply that arrives after Settings closed raises nothing", async () => {
  let finish: (keys: object[]) => void = () => {};
  mocks.keys.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const errors = vi.spyOn(console, "error");
  const view = render(<Settings status={status} onLocked={() => {}} />);
  view.unmount();
  await act(async () => { finish([{}]); });
  expect(errors).not.toHaveBeenCalled();
});

test("unreadable backup details say so and never say that no backup exists", async () => {
  mocks.backup.mockRejectedValueOnce({ kind: "io", message: "fake backup read failure" });
  await open();
  expect(fact("Backup")).toContain("AKM could not read the backup details. fake backup read failure");
  expect(fact("Backup")).not.toMatch(/no backup/i);
});

test("a refresh of the backup details is disabled and busy while it runs", async () => {
  const user = userEvent.setup({ delay: null });
  await open();
  let finish: (info: object) => void = () => {};
  mocks.backup.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await user.click(screen.getByRole("button", { name: "Refresh backup info" }));
  const refresh = screen.getByRole("button", { name: "Reading backup info…" }) as HTMLButtonElement;
  expect(refresh.disabled).toBe(true);
  expect(refresh.getAttribute("aria-busy")).toBe("true");
  await act(async () => { finish({ exists: false }); });
  expect((screen.getByRole("button", { name: "Refresh backup info" }) as HTMLButtonElement).disabled).toBe(false);
});
