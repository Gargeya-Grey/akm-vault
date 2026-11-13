import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
// @ts-expect-error The library that Testing Library uses to compute a description publishes no types through its package exports.
import { computeAccessibleDescription } from "dom-accessibility-api";
import { afterEach, expect, test, vi } from "vitest";
import type { KeyView } from "../types";
import { KeyForm } from "./KeyForm";
import { Unlock } from "./Unlock";
import { UseKeys } from "./UseKeys";

// D2B-06: a hint that belongs to one control is that control's accessible description, so a screen
// reader reads it with the control. The description is computed as a browser computes it.

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async (command: string) => (command === "keys_list" ? [] : undefined)) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
afterEach(cleanup);

const stored: KeyView = {
  id: "fake-key-0001", title: "Fake key", username: null, issuer: null, project: "demo", environment: "dev",
  scopes: [], envVar: "FAKE_KEY", resolvedEnvVar: "FAKE_KEY", url: null, notes: null, tags: [],
  expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null,
  secretChangedAt: null, secretChange: "unknown", historyCount: 0,
};

const addForm = () => render(<KeyForm onSaved={() => {}} onClose={() => {}} />);
const editForm = () => render(<KeyForm item={stored} onSaved={() => {}} onClose={() => {}} />);
const createVault = () => render(<Unlock status={{ sessionId: null, path: "C:/fake/vault.kdbx", exists: false, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false }} onUnlocked={() => {}} />);
const useKeys = (readOnly = false) => async () => {
  await act(async () => { render(<UseKeys readOnly={readOnly} />); });
  await act(async () => {});
};
const importDrawer = async () => {
  await useKeys()();
  fireEvent.click(screen.getByRole("button", { name: "Import .env…" }));
};
const field = (label: string) => () => screen.getByLabelText(label);
const button = (name: string) => () => screen.getByRole("button", { name });

const HINTS: [form: string, open: () => unknown, control: () => HTMLElement, hint: RegExp][] = [
  ["the Add key form", addForm, field("Secret"), /^Encrypted inside the KDBX entry\.$/],
  ["the Add key form", addForm, field("Environment variable"), /^The name this key gets in a \.env file\.$/],
  ["the Add key form", addForm, field("Expires"), /^Optional\.$/],
  ["the Add key form", addForm, field("Rotate every"), /^Days\. Leave blank for no reminder\.$/],
  ["the Add key form", addForm, field("Scopes"), /^One scope on each line\./],
  ["the Add key form", addForm, field("Tags"), /^Comma separated\.$/],
  ["the edit form", editForm, field("Secret (optional)"), /^Leave it blank to keep the stored value\./],
  ["the create-vault form", createVault, field("Vault name"), /Optional\.$/],
  ["the create-vault form", createVault, field("Master password"), /^At least 12 characters\./],
  ["Use keys", useKeys(), button("Open terminal with keys…"), /can read these keys until you close it\.$/],
  ["Use keys in a vault that needs the format upgrade", useKeys(true), button("Import .env…"), /^Upgrade the vault format before importing\.$/],
  ["the import drawer", importDrawer, field("Paste the .env contents"), /^Comments and blank lines are ignored\./],
];

for (const [form, open, control, hint] of HINTS) {
  test(`${form}: ${hint.source} describes its control`, async () => {
    await open();
    expect(computeAccessibleDescription(control())).toMatch(hint);
  });
}
