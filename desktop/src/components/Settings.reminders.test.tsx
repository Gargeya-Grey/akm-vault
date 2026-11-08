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
