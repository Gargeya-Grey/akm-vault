import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { CLIPBOARD_LOCK_TEXT } from "../clipboardText";
import { formatBytes, formatDateTime } from "../format";
import { nativeReads, useNativeRead } from "../nativeRead";
import { REMINDER_NOTE, reminderText } from "../reminders";
import type { Status } from "../types";
import { Notice } from "./Notice";

interface Props {
  status: Status;
  onLocked: () => void;
}

export function Settings({ status, onLocked }: Props) {
  const backupRead = useNativeRead(nativeReads.backup);
  const backup = backupRead.state.status === "ready" ? backupRead.state.data : null;
  // One source for the count: the key list. A newer polled count only starts a new read; the
  // view shows a number only from a read that ended, and says that it is counting until then.
  const countRead = useNativeRead(nativeReads.keys, [status.keyCount]);
  const mounted = useRef(true);

  const reminderRead = useNativeRead(nativeReads.reminders);
  const reminder = reminderRead.state.status === "ready" ? reminderRead.state.data : null;
  const [reminderSaving, setReminderSaving] = useState(false);
  const [reminderError, setReminderError] = useState<string | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  async function changeReminder(enabled: boolean) {
    if (reminderSaving) return;
    setReminderSaving(true);
    setReminderError(null);
    try {
      const next = await api.remindersSet(enabled);
      if (mounted.current) reminderRead.apply(() => next);
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
              {countRead.state.status === "failed" ? (
                <span className="read-inline">
                  <span>AKM could not count the keys. {countRead.state.error}</span>
                  <button type="button" className="text-button" onClick={countRead.reload} disabled={countRead.busy} aria-busy={countRead.busy || undefined}>
                    {countRead.busy ? "Trying again…" : "Try again"}
                  </button>
                </span>
              ) : countRead.state.status === "ready" && !countRead.busy ? (
                <span><span className="mono">{countRead.state.data.length}</span> in this vault</span>
              ) : (
                <span>Counting…</span>
              )}
            </span>
          </div>
          <div className="fact-row">
            <span className="fact-key">Backup</span>
            <span className="fact-value">
              {backupRead.state.status === "failed" ? (
                <span>AKM could not read the backup details. {backupRead.state.error}</span>
              ) : backup === null ? (
                <span>Reading…</span>
              ) : backup.exists ? (
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
            onClick={backupRead.reload}
            disabled={backupRead.busy}
            aria-busy={backupRead.busy || undefined}
          >
            {backupRead.busy ? "Reading backup info…" : "Refresh backup info"}
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
            disabled={reminder === null || reminderRead.busy || reminderSaving}
            aria-describedby="reminders-note"
            onChange={(event) => void changeReminder(event.target.checked)}
          />
          <label htmlFor="reminders-toggle">Show a Windows notification when keys need attention</label>
        </div>
        <p className="field-note" id="reminders-note">{REMINDER_NOTE}</p>
        {reminderRead.state.status === "failed" && (
          <div className="read-inline">
            <Notice tone="error">AKM could not read the reminder setting.</Notice>
            <button type="button" className="text-button" onClick={reminderRead.reload} disabled={reminderRead.busy} aria-busy={reminderRead.busy || undefined}>
              {reminderRead.busy ? "Trying again…" : "Try again"}
            </button>
          </div>
        )}
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
