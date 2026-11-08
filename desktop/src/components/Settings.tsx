import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { CLIPBOARD_LOCK_TEXT, clipboardCopyText } from "../clipboardText";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { formatBytes, formatDateTime, plural } from "../format";
import { REMINDER_NOTE, reminderText } from "../reminders";
import type { BackupInfo, KeyView, ReminderStatus, Status } from "../types";
import { FilterSelect, uniqueOptions } from "./FilterSelect";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";

interface Props {
  status: Status;
  onLocked: () => void;
}

interface Message {
  tone: "ok" | "error";
  text: string;
}

type Scope = { project: string; environment: string };
type ExportPreview = Scope & { text: string };

export function Settings({ status, onLocked }: Props) {
  const [backup, setBackup] = useState<BackupInfo | null>(null);
  const [keys, setKeys] = useState<KeyView[]>([]);
  const [project, setProject] = useState("");
  const [environment, setEnvironment] = useState("");
  const scoped = Boolean(project && environment);
  const [preview, setPreview] = useState<ExportPreview | null>(null);
  const exportRequest = useRef(0);
  const sideEffectRequest = useRef(0);
  const importRequest = useRef(0);
  const mounted = useRef(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<Message | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [reminder, setReminder] = useState<ReminderStatus | null>(null);
  const [reminderLoad, setReminderLoad] = useState<"loading" | "ready" | "failed">("loading");
  const [reminderSaving, setReminderSaving] = useState(false);
  const [reminderError, setReminderError] = useState<string | null>(null);

  const [importing, setImporting] = useState(false);
  const importDialogRef = useDialogFocus<HTMLDivElement>(importing);
  const [importText, setImportText] = useState("");
  const [importProject, setImportProject] = useState("");
  const [importEnvironment, setImportEnvironment] = useState("");
  const [importBusy, setImportBusy] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [importNote, setImportNote] = useState<string | null>(null);

  const hidePreview = useCallback(() => {
    exportRequest.current += 1;
    setPreview(null);
    setBusy((current) => current === "preview" ? null : current);
  }, []);

  const editImport = useCallback(() => {
    importRequest.current += 1;
    setImportError(null);
    setImportNote(null);
    setImportBusy(false);
  }, []);

  const closeImport = useCallback(() => {
    editImport();
    setImportText("");
    setImporting(false);
  }, [editImport]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      exportRequest.current += 1;
      sideEffectRequest.current += 1;
      importRequest.current += 1;
    };
  }, []);

  useEffect(() => {
    const expire = () => {
      hidePreview();
      sideEffectRequest.current += 1;
      setBusy(null);
      setMessage(null);
      // Local protected results expire here. Already-dispatched outcomes belong to the API.
      closeImport();
      setImportProject("");
      setImportEnvironment("");
    };
    const onVisibility = () => { if (document.hidden) expire(); };
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener(PROTECTED_VIEW_EXPIRY_EVENT, expire);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener(PROTECTED_VIEW_EXPIRY_EVENT, expire);
    };
  }, [hidePreview, closeImport]);

  const loadBackup = useCallback(async () => {
    try {
      const next = await api.backup();
      if (mounted.current) setBackup(next);
    } catch (cause) {
      if (mounted.current) setLoadError(errorText(cause));
    }
  }, []);

  const refreshKeys = useCallback(async () => {
    try {
      const next = await api.keys();
      if (mounted.current) setKeys(next);
    } catch (cause) {
      if (mounted.current) setLoadError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    void loadBackup();
    void refreshKeys();
  }, [loadBackup, refreshKeys]);

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

  useEffect(() => {
    if (preview === null) return;
    const timer = window.setTimeout(hidePreview, 30_000);
    return () => window.clearTimeout(timer);
  }, [preview, hidePreview]);

  useEffect(() => {
    if (!importing) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !importBusy) closeImport();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [importing, importBusy, closeImport]);

  const projects = useMemo(
    () => uniqueOptions(keys.map((key) => key.project)),
    [keys],
  );
  const environments = useMemo(
    () => uniqueOptions(keys.filter((key) => key.project === project).map((key) => key.environment)),
    [keys, project],
  );
  const matchCount = keys.filter((key) => key.project === project && key.environment === environment).length;

  function chooseProject(value: string) {
    hidePreview();
    sideEffectRequest.current += 1;
    setBusy(null);
    setProject(value);
    setEnvironment("");
    setPreview(null);
    setMessage(null);
  }

  function chooseEnvironment(value: string) {
    hidePreview();
    sideEffectRequest.current += 1;
    setBusy(null);
    setEnvironment(value);
    setPreview(null);
    setMessage(null);
  }

  async function runPreview() {
    if (!scoped || document.hidden) return;
    const currentRequest = ++exportRequest.current;
    const scope = { project, environment };
    setBusy("preview");
    setMessage(null);
    try {
      const text = await api.exportEnv(scope.project, scope.environment);
      if (currentRequest !== exportRequest.current || document.hidden) return;
      setPreview({ ...scope, text });
      const lines = text.split("\n").filter((line) => line.trim()).length;
      setMessage({
        tone: "ok",
        text:
          lines > 0
            ? `Previewed ${plural(lines, "line")}. Values hide after 30 seconds.`
            : "No keys match that project and environment.",
      });
    } catch (cause) {
      if (currentRequest === exportRequest.current) setMessage({ tone: "error", text: errorText(cause) });
    } finally {
      if (currentRequest === exportRequest.current) setBusy(null);
    }
  }

  async function copyEnv() {
    if (!scoped) return;
    const currentRequest = ++sideEffectRequest.current;
    setBusy("copy");
    setMessage(null);
    try {
      const seconds = await api.copyEnv(project || null, environment || null);
      if (currentRequest !== sideEffectRequest.current) return;
      setMessage({
        tone: "ok",
        text: clipboardCopyText(seconds),
      });
    } catch (cause) {
      if (currentRequest === sideEffectRequest.current) setMessage({ tone: "error", text: errorText(cause) });
    } finally {
      if (currentRequest === sideEffectRequest.current) setBusy(null);
    }
  }

  async function saveEnv() {
    if (!scoped) return;
    const currentRequest = ++sideEffectRequest.current;
    setMessage(null);
    setBusy("save");
    try {
      const savedPath = await api.saveEnv(project, environment);
      if (currentRequest !== sideEffectRequest.current) return;
      if (savedPath) setMessage({ tone: "ok", text: `Saved plaintext secrets to ${savedPath}. Keep this file out of source control and cloud sharing.` });
    } catch (cause) {
      if (currentRequest === sideEffectRequest.current) setMessage({ tone: "error", text: errorText(cause) });
    } finally {
      if (currentRequest === sideEffectRequest.current) setBusy(null);
    }
  }

  function openImport() {
    editImport();
    setImporting(true);
    setImportError(null);
    setImportText("");
    setImportProject(project);
    setImportEnvironment(environment);
  }

  async function submitImport(event: React.FormEvent) {
    event.preventDefault();
    if (importBusy) return;
    setImportError(null);
    setImportNote(null);

    if (!importProject.trim() || !importEnvironment.trim()) {
      setImportError("Choose the target project and environment. Imports never search other scopes.");
      return;
    }
    if (!importText.trim()) {
      setImportError("Paste at least one NAME=value line.");
      return;
    }

    const currentRequest = ++importRequest.current;
    setImportBusy(true);
    try {
      const report = await api.importEnv(importText, importProject.trim(), importEnvironment.trim());
      if (currentRequest !== importRequest.current) return;
      if (report === null) {
        setImportNote("Nothing was imported. You declined it in the confirmation window.");
        return;
      }
      setImporting(false);
      setImportText("");
      setMessage({
        tone: "ok",
        text: `Imported: ${report.added} added, ${report.updated} updated.`,
      });
      await refreshKeys();
      await loadBackup();
    } catch (cause) {
      if (currentRequest === importRequest.current) {
        setImportError(errorText(cause));
        setMessage({ tone: "error", text: errorText(cause) });
      }
    } finally {
      if (currentRequest === importRequest.current) setImportBusy(false);
    }
  }

  return (
    <main className="wrap view">
      <header className="view-head">
        <div>
          <h1>
            Settings, <em>and the exit.</em>
          </h1>
          <p className="lead">
            The vault file is the source of truth. These controls only touch
            this machine: where the file lives, what the backup holds, and how a
            slice becomes a .env file.
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
              <span><span className="mono">{keys.length}</span> in this vault</span>
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
        <h2>Environment file</h2>
        <p className="prose">
          Select one project and environment before exposing any values.
          Exports are plaintext, even though the vault is encrypted. Preview
          only on a private screen. AKM hides it after 30 seconds.
        </p>

        <div className="settings-filters">
          <FilterSelect
            id="env-project"
            label="Project"
            value={project}
            placeholder="Choose a project"
            options={projects}
            onChange={chooseProject}
            disabled={busy === "save" || busy === "copy"}
          />
          <FilterSelect
            id="env-environment"
            label="Environment"
            value={environment}
            placeholder="Choose an environment"
            options={environments}
            onChange={chooseEnvironment}
            disabled={busy === "save" || busy === "copy"}
          />
        </div>

        <div className="settings-actions">
          <button
            type="button"
            className="btn"
            onClick={() => void saveEnv()}
            disabled={busy !== null || !scoped}
            aria-busy={busy === "save" || undefined}
          >
            <Icon name="download" />
            {busy === "save" ? "Saving…" : "Save .env…"}
          </button>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => void runPreview()}
            disabled={busy !== null || !scoped}
            aria-busy={busy === "preview" || undefined}
          >
            {busy === "preview" ? "Previewing…" : "Preview"}
          </button>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => void copyEnv()}
            disabled={busy !== null || !scoped}
            aria-busy={busy === "copy" || undefined}
          >
            {busy === "copy" ? "Copying…" : "Copy to clipboard"}
          </button>
          <button
            type="button"
            className="text-button"
            onClick={openImport}
            disabled={status.needsUpgrade || importBusy}
          >
            <Icon name="upload" />
            Import .env…
          </button>
        </div>

        <p className="field-note">
          {scoped ? `${plural(matchCount, "key")} selected in ${project} / ${environment}.` : "Choose a project and environment to enable export."}
          {status.needsUpgrade && " Upgrade the vault format before importing."}
        </p>

        {message && (
          <Notice tone={message.tone === "ok" ? "ok" : "error"}>
            {message.text}
          </Notice>
        )}

        <div className="raised env-preview">
          {preview === null ? (
            <div className="empty-state">
              <h3>No preview yet.</h3>
              <p>
                Choose a slice, then select Preview to see the exact lines that
                would be written. Nothing is saved until you ask.
              </p>
            </div>
          ) : (
            <>
              <div className="preview-head">
                <p className="label">Preview: {preview.project} / {preview.environment}</p>
                <p className="label">
                  {plural(
                    preview.text.split("\n").filter((line) => line.trim()).length,
                    "line",
                  )}
                </p>
              </div>
              <pre className="mono env-lines">
                {preview.text.trim() || "# No keys match this slice."}
              </pre>
            </>
          )}
        </div>
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

      {importing && (
        <div
          className="overlay"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !importBusy) closeImport();
          }}
        >
          <div
            ref={importDialogRef}
            tabIndex={-1}
            className="modal form-modal raised"
            role="dialog"
            aria-modal="true"
            aria-labelledby="import-title"
          >
            <header className="modal-head">
              <div>
                <p className="label">From a .env file</p>
                <h2 id="import-title">Import keys</h2>
              </div>
              <button
                type="button"
                className="icon-button"
                aria-label="Close"
                onClick={closeImport}
                disabled={importBusy}
              >
                <Icon name="close" size={20} />
              </button>
            </header>

            <form onSubmit={submitImport}>
              <div className="field">
                <label htmlFor="import-text">Paste the .env contents</label>
                <textarea
                  id="import-text"
                  className="mono"
                  rows={9}
                  value={importText}
                  onChange={(event) => { editImport(); setImportText(event.target.value); }}
                  disabled={importBusy}
                  placeholder={"OPENAI_API_KEY=sk-…\nGITHUB_TOKEN=ghp_…"}
                  spellCheck={false}
                  autoFocus
                />
                <p className="field-note">
                  Comments and blank lines are ignored. Duplicate names are an
                  error rather than a silent override.
                </p>
              </div>

              <div className="form-grid">
                <div className="field">
                  <label htmlFor="import-project">Target project</label>
                  <input
                    id="import-project"
                    value={importProject}
                    onChange={(event) => { editImport(); setImportProject(event.target.value); }}
                    disabled={importBusy}
                    placeholder="my-app"
                    autoComplete="off"
                  />
                </div>
                <div className="field">
                  <label htmlFor="import-environment">
                    Target environment
                  </label>
                  <input
                    id="import-environment"
                    value={importEnvironment}
                    onChange={(event) => { editImport(); setImportEnvironment(event.target.value); }}
                    disabled={importBusy}
                    placeholder="dev"
                    autoComplete="off"
                  />
                </div>
              </div>

              <Notice tone="info">
                Only keys in this exact project and environment can be updated.
                Empty values are rejected. AKM then opens a confirmation window
                that lists the entry names. Nothing is saved until you confirm there.
              </Notice>

              {importBusy && <Notice tone="info">Waiting for your answer in the AKM confirmation window. Nothing is saved until you confirm there.</Notice>}
              {importNote && <Notice tone="info">{importNote}</Notice>}

              {importError && <Notice tone="error">{importError}</Notice>}

              <div className="dialog-actions">
                <button
                  type="button"
                  className="text-button"
                  onClick={closeImport}
                  disabled={importBusy}
                >
                  Cancel
                </button>
                <button type="submit" className="btn" disabled={importBusy} aria-busy={importBusy || undefined}>
                  {importBusy ? "Waiting for confirmation…" : "Review import"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </main>
  );
}
