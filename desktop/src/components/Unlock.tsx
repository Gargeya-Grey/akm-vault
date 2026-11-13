import { useEffect, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "../api";
import type { Status } from "../types";
import { BrandMark, LockCylinder } from "./glyphs";
import { Icon } from "./Icon";
import { Notice } from "./Notice";

interface Props {
  status: Status;
  onUnlocked: (status: Status) => void;
}

type Mode = "unlock" | "create";

const SHORTEST_PASSWORD = 12;

export function Unlock({ status, onUnlocked }: Props) {
  const [mode, setMode] = useState<Mode>(status.exists ? "unlock" : "create");
  const [path, setPath] = useState(status.path);
  const [password, setPassword] = useState("");
  // Unicode characters, as the native rule counts them. String length counts UTF-16 units.
  const passwordLength = [...password].length;
  const [confirm, setConfirm] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);

  useEffect(() => () => { request.current += 1; }, []);

  useEffect(() => {
    setPath(status.path);
    setMode(status.exists ? "unlock" : "create");
  }, [status.path, status.exists]);

  async function browse() {
    const currentRequest = ++request.current;
    try {
      const chosen =
        mode === "unlock"
          ? await openDialog({
              multiple: false,
              directory: false,
              title: "Choose a KDBX vault",
              filters: [{ name: "KeePass vault", extensions: ["kdbx"] }],
            })
          : await saveDialog({
              title: "Create a KDBX vault",
              defaultPath: path,
              filters: [{ name: "KeePass vault", extensions: ["kdbx"] }],
            });

      if (currentRequest === request.current && typeof chosen === "string") setPath(chosen);
    } catch (cause) {
      if (currentRequest === request.current) setError(errorText(cause));
    }
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);

    // Native code takes the path exactly as it is sent, and a file can exist at a path and at the
    // same path without its outer spaces (D2B-05). So the path goes as the field holds it.
    const target = path;
    if (!target.trim()) {
      setError("Choose where the vault file should live.");
      return;
    }

    if (mode === "create") {
      if (passwordLength < SHORTEST_PASSWORD) {
        setError("Use at least 12 characters for the master password.");
        return;
      }
      if (password !== confirm) {
        setError("The two passwords do not match.");
        return;
      }
    } else if (!password) {
      setError("Enter the master password.");
      return;
    }

    setBusy(true);
    const currentRequest = ++request.current;
    try {
      const next =
        mode === "create"
          ? await api.create(target, password, name.trim() || null)
          : await api.unlock(target, password);
      if (currentRequest !== request.current) return;
      setPassword("");
      setConfirm("");
      onUnlocked(next);
    } catch (cause) {
      if (currentRequest === request.current) setError(errorText(cause));
    } finally {
      if (currentRequest === request.current) setBusy(false);
    }
  }

  function switchMode(next: Mode) {
    request.current += 1;
    setMode(next);
    setError(null);
    setPassword("");
    setConfirm("");
  }

  const pinsSet = Math.min(passwordLength, SHORTEST_PASSWORD);

  return (
    <main className="unlock">
      <aside className="lock-stage" aria-label="About the vault">
        <div className="lock-brand">
          <BrandMark />
          <p className="bar-name">AKM</p>
        </div>
        <div className="lock-art">
          <LockCylinder filled={passwordLength} busy={busy} rule={mode === "create"} />
        </div>
        <ul className="lock-promises">
          <li>
            <strong>A real KeePass file</strong>
            {/* FR-11: the format and the ciphers are named only for a vault that AKM creates. */}
            <span>
              {mode === "create"
                ? "This new vault uses KDBX 4.1 with AES-256 and Argon2id."
                : "A vault you open keeps its own encryption settings. AKM changes an older format only when you upgrade it to KDBX 4.1."}
              {" "}Open it in KeePassXC at any time, with no dependency on this app.
            </span>
          </li>
          <li>
            <strong>No recovery path</strong>
            <span>There is no password reset. Back up the file and keep the password somewhere you trust.</span>
          </li>
          <li>
            <strong>Your folder, your sync</strong>
            <span>OneDrive, Dropbox or Syncthing. Keep the file downloaded locally. Detected changes block saving; this is not a distributed lock or an automatic merge.</span>
          </li>
        </ul>
      </aside>

      <div className="unlock-pane">
        <div className="unlock-copy">
          {mode === "unlock" ? (
            <h1>Your keys, <em>one place.</em></h1>
          ) : (
            <h1>Start a vault. <em>Keep the only key.</em></h1>
          )}
          <p className="lead">
            {mode === "unlock"
              ? "Unlock a vault on this machine or in your cloud folder. AKM also writes encrypted backups and a separate activity journal."
              : "Choose where the KDBX file lives. Your master password is not saved in settings and cannot be recovered."}
          </p>

          <form className="unlock-form" onSubmit={submit}>
            <div className="field">
              <label htmlFor="vault-path">Vault file</label>
              <div className="path-row">
                <input
                  id="vault-path"
                  className="mono"
                  value={path}
                  disabled={busy}
                  onChange={(event) => setPath(event.target.value)}
                  spellCheck={false}
                  autoComplete="off"
                />
                <button type="button" className="btn-secondary" onClick={browse} disabled={busy}>
                  <Icon name="folder" />
                  Browse…
                </button>
              </div>
            </div>

            {mode === "create" && (
              <div className="field">
                <label htmlFor="vault-name">Vault name</label>
                <input
                  id="vault-name"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  placeholder="API Keys"
                  autoComplete="off"
                  aria-describedby="vault-name-note"
                />
                <p className="field-note" id="vault-name-note">Written into the file’s metadata. Optional.</p>
              </div>
            )}

            <div className="field">
              <label htmlFor="master-password">Master password</label>
              <input
                id="master-password"
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoComplete={mode === "create" ? "new-password" : "current-password"}
                autoFocus
                aria-describedby={mode === "create" ? "master-password-note" : undefined}
              />
              {mode === "create" && (
                <p className="field-note" id="master-password-note">At least 12 characters. There is no reset and no recovery.</p>
              )}
              {mode === "create" && password.length > 0 && (
                <p className={`pins-state${pinsSet === SHORTEST_PASSWORD ? " done" : ""}`} aria-hidden="true">
                  <span className="num">{pinsSet}</span> of 12 pins set
                </p>
              )}
            </div>

            {mode === "create" && (
              <div className="field">
                <label htmlFor="master-confirm">Confirm password</label>
                <input
                  id="master-confirm"
                  type="password"
                  value={confirm}
                  onChange={(event) => setConfirm(event.target.value)}
                  autoComplete="new-password"
                />
              </div>
            )}

            {error && <div className="form-error"><Notice tone="error">{error}</Notice></div>}

            <div className="form-actions">
              <button type="submit" className="btn" disabled={busy} aria-busy={busy || undefined}>
                {busy ? "Working…" : mode === "create" ? "Create vault" : "Unlock vault"}
              </button>
              <button
                type="button"
                className="text-button"
                onClick={() => switchMode(mode === "unlock" ? "create" : "unlock")}
                disabled={busy}
              >
                {mode === "unlock" ? "Create a new vault instead" : "I already have a vault"}
              </button>
            </div>
          </form>
        </div>
      </div>
    </main>
  );
}
