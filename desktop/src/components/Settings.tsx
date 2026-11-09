import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { CLIPBOARD_LOCK_TEXT } from "../clipboardText";
import { formatBytes, formatDateTime } from "../format";
import { REMINDER_NOTE, reminderText } from "../reminders";
import type { BackupInfo, ReminderStatus, Status } from "../types";
import { Notice } from "./Notice";

interface Props {
  status: Status;
  onLocked: () => void;
}

export function Settings({ status, onLocked }: Props) {
  const [backup, setBackup] = useState<BackupInfo | null>(null);
  const [keyCount, setKeyCount] = useState<number | null>(null);
  const mounted = useRef(true);
  const listRead = useRef(0);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [reminder, setReminder] = useState<ReminderStatus | null>(null);
  const [reminderLoad, setReminderLoad] = useState<"loading" | "ready" | "failed">("loading");
  const [reminderSaving, setReminderSaving] = useState(false);
  const [reminderError, setReminderError] = useState<string | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const loadBackup = useCallback(async () => {
    try {
      const next = await api.backup();
      if (mounted.current) setBackup(next);
    } catch (cause) {
      if (mounted.current) setLoadError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    void loadBackup();
  }, [loadBackup]);

  // The status poll can lag a change that just finished, so the list gives the count. A new
  // polled count starts a new read, the latest read wins, and a failed read uses the polled count.
  useEffect(() => {
    const read = ++listRead.current;
    const current = () => mounted.current && read === listRead.current;
    void api.keys().then(
      (keys) => { if (current()) setKeyCount(keys.length); },
      () => { if (current()) setKeyCount(null); },
    );
  }, [status.keyCount]);

  useEffect(() => {
    void (async () => {
      try {
        const next = await api.remindersStatus();
        if (!mounted.current) return;
        setReminder(next);
        setReminderLoad("ready");
      } catch {
        if (mounted.current) setReminderLoad("failed");
      }
    })();
  }, []);

  async function changeReminder(enabled: boolean) {
    if (reminderSaving) return;
    setReminderSaving(true);
    setReminderError(null);
    try {
      const next = await api.remindersSet(enabled);
      if (mounted.current) setReminder(next);
    } catch (cause) {
      if (mounted.current) setReminderError(errorText(cause));
    } finally {
      if (mounted.current) setReminderSaving(false);
    }
  }

  return (
    <main className="view">
      <header className="view-head">
        <div>
          <h1>Settings, <em>for this machine.</em></h1>
          <p className="lead">
            The vault file is the source of truth. These controls only touch
            this machine.
          </p>
        </div>
      </header>

      {loadError && <Notice tone="error">{loadError}</Notice>}

      <section className="settings-section">
        <h2>Vault</h2>
        <div className="fact-list">
          <div className="fact-row">
            <span className="fact-key">Name</span>
            <span className="fact-value">{status.vaultName ?? "Untitled"}</span>
          </div>
          <div className="fact-row">
            <span className="fact-key">File</span>
            <span className="fact-value mono path-text">{status.path}</span>
          </div>
          <div className="fact-row">
            <span className="fact-key">Keys</span>
            <span className="fact-value">
              <span><span className="mono">{keyCount ?? status.keyCount}</span> in this vault</span>
            </span>
          </div>
          <div className="fact-row">
            <span className="fact-key">Backup</span>
            <span className="fact-value">
              {backup?.exists ? (
                <>
                  <span className="mono path-text">{backup.path}</span>
                  <span className="key-sub">
                    <span className="mono">{formatBytes(backup.bytes)}</span>
                    {backup.modified
                      ? ` · written ${formatDateTime(backup.modified)}`
                      : ""}
                  </span>
                </>
              ) : (
                "Written automatically on the next save."
              )}
            </span>
          </div>
        </div>
        <div className="settings-actions">
          <button type="button" className="btn-secondary" onClick={onLocked}>
            Lock vault
          </button>
          <button
            type="button"
            className="text-button"
            onClick={() => void loadBackup()}
          >
            Refresh backup info
          </button>
        </div>
        <p className="field-note">
          The vault locks after 10 minutes without activity, on Windows session
          lock, and on suspend. {CLIPBOARD_LOCK_TEXT} You will need the master
          password again.
        </p>
      </section>

      <section className="settings-section" aria-labelledby="reminders-title">
        <h2 id="reminders-title">Reminders</h2>
        <div className="reminder-toggle">
          <input
            id="reminders-toggle"
            type="checkbox"
            checked={reminder?.enabled ?? false}
            disabled={reminderLoad !== "ready" || reminderSaving}
            aria-describedby="reminders-note"
            onChange={(event) => void changeReminder(event.target.checked)}
          />
          <label htmlFor="reminders-toggle">Show a Windows notification when keys need attention</label>
        </div>
        <p className="field-note" id="reminders-note">{REMINDER_NOTE}</p>
        {reminderLoad === "failed" && <Notice tone="error">AKM could not read the reminder setting.</Notice>}
        {reminder && <p className="reminder-state prose" role="status">{reminderText(reminder)}</p>}
        {reminderError && <Notice tone="error">{reminderError}</Notice>}
      </section>

      <section className="settings-section">
        <h2>No recovery path</h2>
        <p className="prose">
          The master password is not saved to settings. There is no reset, no recovery
          code and no server-side copy. Back up the .kdbx file somewhere
          separate from this machine, and keep the password somewhere you trust.
          Retained backups beside the vault are a convenience, not a separate
          backup strategy.
        </p>
      </section>

      <section className="settings-section">
        <h2>About</h2>
        <p className="prose">Development build. Not independently audited.</p>
      </section>
    </main>
  );
}
