import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Settings } from "./Settings";
import type { ReminderStatus } from "../types";

// Unit 4 (M13): the opt-in setting, and every state it can show. A denied or disabled
// permission is a visible state, not an error loop.

const mocks = vi.hoisted(() => ({
  backup: vi.fn().mockResolvedValue({ exists: false }),
  keys: vi.fn().mockResolvedValue([]),
  remindersStatus: vi.fn(),
  remindersSet: vi.fn(),
  remindersNotify: vi.fn(),
}));
vi.mock("../native", () => ({ reads: mocks }));
vi.mock("../api", () => ({
  api: mocks,
  errorText: (error: unknown) => (error && typeof error === "object" && "message" in error ? String((error as { message: unknown }).message) : String(error)),
}));

const status = { sessionId: crypto.randomUUID(), path: "C:/test/vault.kdbx", exists: true, unlocked: true, keyCount: 0, vaultName: "Test", needsUpgrade: false };
const base: ReminderStatus = { enabled: false, permission: "granted", delivery: "none", blocked: false };

beforeEach(() => {
  vi.clearAllMocks();
  mocks.backup.mockResolvedValue({ exists: false });
  mocks.keys.mockResolvedValue([]);
  mocks.remindersStatus.mockResolvedValue(base);
});
afterEach(cleanup);

async function open() {
  await act(async () => { render(<Settings status={status} onLocked={() => {}} />); });
}
const toggle = () => screen.getByRole("checkbox", { name: "Show a Windows notification when keys need attention" }) as HTMLInputElement;
const state = () => document.querySelector(".reminder-state")?.textContent;

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

test("the setting is off by default, says what it does, and sends nothing by itself", async () => {
  await open();
  expect(toggle().checked).toBe(false);
  expect(state()).toBe("Off. AKM shows a summary after you unlock the vault. It sends no Windows notification.");
  expect(mocks.remindersSet).not.toHaveBeenCalled();
  expect(mocks.remindersNotify).not.toHaveBeenCalled();
  expect(screen.getByRole("checkbox", { name: "Show a Windows notification when keys need attention", description: /only how many keys need attention.*names no key, project or environment.*only while AKM is open and unlocked/ })).toBeTruthy();
});

test("turning it on with the keyboard saves once and shows the on state", async () => {
  const user = userEvent.setup({ delay: null });
  mocks.remindersSet.mockResolvedValue({ ...base, enabled: true });
  await open();
  toggle().focus();
  await user.keyboard(" ");
  expect(mocks.remindersSet).toHaveBeenCalledExactlyOnceWith(true);
  await screen.findByText(/^On\. AKM sends one count-only notification/);
  expect(toggle().checked).toBe(true);
});

test("turning it off saves false", async () => {
  const user = userEvent.setup({ delay: null });
  mocks.remindersStatus.mockResolvedValue({ ...base, enabled: true });
  mocks.remindersSet.mockResolvedValue(base);
  await open();
  await screen.findByText(/^On\./);
  await user.click(toggle());
  expect(mocks.remindersSet).toHaveBeenCalledExactlyOnceWith(false);
  await screen.findByText(/^Off\./);
});

test("a denied permission is shown as a state, with the way out, and the page sends nothing", async () => {
  mocks.remindersStatus.mockResolvedValue({ ...base, enabled: true, permission: "denied", blocked: true });
  await open();
  await screen.findByText(/Windows has not allowed notifications for AKM/);
  expect(state()).toBe("On, but Windows has not allowed notifications for AKM. AKM does not try again. Allow them in the Windows notification settings for AKM, then turn this setting off and on.");
  expect(document.querySelector(".reminder-state")?.getAttribute("role")).toBe("status");
  expect(mocks.remindersNotify).not.toHaveBeenCalled();
  expect(mocks.remindersStatus).toHaveBeenCalledTimes(1);
});

test("a notification that Windows did not accept is shown as a state", async () => {
  mocks.remindersStatus.mockResolvedValue({ ...base, enabled: true, delivery: "failed", blocked: true });
  await open();
  await screen.findByText(/did not accept the last notification/);
  expect(state()).toBe("On, but Windows did not accept the last notification. AKM does not try again in this run. Turn this setting off and on to try again.");
});

test("a sent notification is not called seen, because AKM cannot see Focus assist or the per-app switch", async () => {
  mocks.remindersStatus.mockResolvedValue({ ...base, enabled: true, delivery: "sent" });
  await open();
  await screen.findByText(/handed to Windows/);
  expect(state()).toBe("On. The last notification was handed to Windows. AKM cannot see whether Windows showed it: Focus assist and the per-app notification switch decide that.");
});

test("a setting that cannot be saved is an error and the box returns to the saved choice", async () => {
  const user = userEvent.setup({ delay: null });
  mocks.remindersSet.mockRejectedValue({ kind: "io", message: "The settings could not be written." });
  await open();
  await user.click(toggle());
  await screen.findByText("The settings could not be written.");
  expect(toggle().checked).toBe(false);
  expect(state()).toMatch(/^Off\./);
});

test("an unreadable setting is shown, and the box cannot be used until it can be read", async () => {
  mocks.remindersStatus.mockRejectedValue({ kind: "internal", message: "no" });
  await open();
  await screen.findByText("AKM could not read the reminder setting.");
  expect(toggle().disabled).toBe(true);
});

test("a retry of an unreadable setting that succeeds clears the error and frees the box", async () => {
  const user = userEvent.setup({ delay: null });
  mocks.remindersStatus.mockRejectedValueOnce({ kind: "internal", message: "no" }).mockResolvedValueOnce(base);
  await open();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await act(async () => {});
  expect(screen.queryByText("AKM could not read the reminder setting.")).toBeNull();
  expect(toggle().disabled).toBe(false);
});

test("while a change is being saved the box is disabled, so a second click cannot send a second request", async () => {
  const user = userEvent.setup({ delay: null });
  let finish!: (value: ReminderStatus) => void;
  mocks.remindersSet.mockReturnValue(new Promise<ReminderStatus>((done) => { finish = done; }));
  await open();
  await user.click(toggle());
  expect(toggle().disabled).toBe(true);
  await user.click(toggle());
  expect(mocks.remindersSet).toHaveBeenCalledTimes(1);
  await act(async () => finish({ ...base, enabled: true }));
  expect(toggle().disabled).toBe(false);
});
