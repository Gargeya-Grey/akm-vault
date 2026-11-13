import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Unlock } from "./Unlock";

// D2B-05: native code takes the vault path exactly as the page sends it, and two different files
// can exist at a path and at the same path without its outer spaces. So the page sends the path as
// the field holds it: from the saved settings, typed, or picked.

const mock = vi.hoisted(() => ({ unlock: vi.fn(), create: vi.fn(), open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mock.open, save: mock.save }));
vi.mock("../api", () => ({ api: { unlock: mock.unlock, create: mock.create }, errorText: String }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

const PADDED = "C:\\fake\\vault.kdbx\u00A0";
const status = (path: string, exists: boolean) => ({ sessionId: null, path, exists, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
const PASSWORD = "fake-password-0123";

const SOURCES: [how: string, exists: boolean, choose: () => Promise<void>, saved: string][] = [
  ["from the saved settings, to unlock", true, async () => {}, PADDED],
  ["typed, to unlock", true, async () => { fireEvent.change(screen.getByLabelText("Vault file"), { target: { value: PADDED } }); }, "C:\\fake\\other.kdbx"],
  ["picked, to unlock", true, async () => { mock.open.mockResolvedValueOnce(PADDED); await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Browse…" })); }); }, "C:\\fake\\other.kdbx"],
  ["picked, to create", false, async () => { mock.save.mockResolvedValueOnce(PADDED); await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Browse…" })); }); }, "C:\\fake\\other.kdbx"],
];

for (const [how, exists, choose, saved] of SOURCES) {
  test(`a vault path ${how} reaches native code exactly, with its outer spaces`, async () => {
    render(<Unlock status={status(saved, exists)} onUnlocked={() => {}} />);
    await choose();
    fireEvent.change(screen.getByLabelText("Master password"), { target: { value: PASSWORD } });
    if (!exists) fireEvent.change(screen.getByLabelText("Confirm password"), { target: { value: PASSWORD } });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: exists ? "Unlock vault" : "Create vault" })); });
    const sent = exists ? mock.unlock : mock.create;
    expect(sent).toHaveBeenCalledOnce();
    expect(sent.mock.calls[0][0]).toBe(PADDED);
  });
}

test("a vault path of spaces is refused before native code is asked", async () => {
  render(<Unlock status={status(" \u00A0", true)} onUnlocked={() => {}} />);
  fireEvent.change(screen.getByLabelText("Master password"), { target: { value: PASSWORD } });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Unlock vault" })); });
  expect(screen.getByText("Choose where the vault file should live.")).toBeTruthy();
  expect(mock.unlock).not.toHaveBeenCalled();
});
