import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Unlock } from "./Unlock";

// FR-11: the unlock screen names KDBX 4.1, AES-256 and Argon2id only for a vault that AKM creates.
// A vault that AKM opens keeps its own settings (crates/akm-core/tests/vault.rs ::
// older_formats_require_explicit_upgrade_and_preserve_the_database_and_kdf).

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../api", () => ({ api: {}, errorText: String }));
afterEach(cleanup);

const status = (exists: boolean) => ({ sessionId: null, path: "C:/fake/vault.kdbx", exists, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
const promise = () => screen.getByRole("complementary", { name: "About the vault" }).querySelector(".lock-promises li")!.textContent!;
const CIPHERS = /KDBX 4\.1 with AES-256|Argon2id/;

for (const [how, exists, flip] of [
  ["an existing vault", true, false],
  ["a new vault", false, false],
  ["an existing vault, after a switch from create", false, true],
  ["a new vault, after a switch from unlock", true, true],
] as const) {
  const opening = exists !== flip;
  test(`for ${how} the screen ${opening ? "keeps the vault's own settings and names no cipher" : "names the format and the ciphers of the new vault"}`, () => {
    render(<Unlock status={status(exists)} onUnlocked={() => {}} />);
    if (flip) fireEvent.click(screen.getByRole("button", { name: exists ? "Create a new vault instead" : "I already have a vault" }));
    expect(screen.getByRole("button", { name: opening ? "Unlock vault" : "Create vault" })).toBeTruthy();
    const text = promise();
    if (opening) {
      expect(text).not.toMatch(CIPHERS);
      expect(text).toContain("A vault you open keeps its own encryption settings.");
      expect(text).toContain("AKM changes an older format only when you upgrade it to KDBX 4.1.");
    } else {
      expect(text).toContain("This new vault uses KDBX 4.1 with AES-256 and Argon2id.");
      expect(text).not.toContain("keeps its own");
    }
    expect(text).toContain("Open it in KeePassXC at any time, with no dependency on this app.");
  });
}
