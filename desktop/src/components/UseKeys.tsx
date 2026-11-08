import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { clipboardCopyText } from "../clipboardText";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { plural } from "../format";
import type { KeyView } from "../types";
import { FilterSelect, uniqueOptions } from "./FilterSelect";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";

interface Message {
  tone: "ok" | "error";
  text: string;
}

type Scope = { project: string; environment: string };
type ExportPreview = Scope & { text: string };

export function UseKeys({ readOnly = false }: { readOnly?: boolean }) {
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

  const refreshKeys = useCallback(async () => {
    try {
      const next = await api.keys();
      if (mounted.current) setKeys(next);
    } catch (cause) {
      if (mounted.current) setLoadError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    void refreshKeys();
  }, [refreshKeys]);

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

  async function openTerminal() {
    if (!scoped) return;
    const currentRequest = ++sideEffectRequest.current;
    setMessage(null);
    setBusy("launch");
    try {
      const count = await api.launchTerminal(project, environment);
      if (currentRequest !== sideEffectRequest.current) return;
      setMessage(
        count === null
          ? { tone: "ok", text: "Nothing was opened. You cancelled in the folder window or the confirmation window." }
          : { tone: "ok", text: `Opened a terminal with ${plural(count, "key")}. That window keeps them until you close it, even after AKM locks.` },
      );
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
    <main className="view">
      <header className="view-head">
        <div>
          <h1>Use keys, <em>in your projects.</em></h1>
          <p className="lead">
            Take one project’s keys out as a .env file, or bring keys in from one.
          </p>
        </div>
      </header>

      {loadError && <Notice tone="error">{loadError}</Notice>}

      <div className="use-grid">
        <section className="use-steps" aria-labelledby="use-out-title">
          <h2 id="use-out-title">Take keys out</h2>
          <p className="lead">
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
              disabled={busy === "save" || busy === "copy" || busy === "launch"}
            />
            <FilterSelect
              id="env-environment"
              label="Environment"
              value={environment}
              placeholder="Choose an environment"
              options={environments}
              onChange={chooseEnvironment}
              disabled={busy === "save" || busy === "copy" || busy === "launch"}
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
              className="btn-secondary"
              onClick={() => void openTerminal()}
              disabled={busy !== null || !scoped}
              aria-busy={busy === "launch" || undefined}
            >
              Open terminal with keys…
            </button>
          </div>

          <p className="field-note">
            {scoped ? `${plural(matchCount, "key")} selected in ${project} / ${environment}.` : "Choose a project and environment to enable export."}
          </p>
          <p className="field-note">
            Open terminal starts a command prompt in a folder that you choose. Every program you run there can read these keys until you close it.
          </p>

          {busy === "launch" && (
            <Notice tone="info">
              Choose the folder and answer the confirmation window. Nothing opens until you confirm there.
            </Notice>
          )}

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
                  Choose a project and environment, then select Preview to see the
                  exact lines that would be written. Nothing is saved until you ask.
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
                  {preview.text.trim() || "# No keys match this project and environment."}
                </pre>
              </>
            )}
          </div>
        </section>

        <aside className="use-side" aria-labelledby="use-in-title">
          <div className="use-side-block">
            <h2 id="use-in-title">Bring keys in</h2>
            <p>
              Paste a .env file. AKM opens a confirmation window that lists the
              entry names, and saves nothing until you confirm there.
            </p>
            <button
              type="button"
              className="btn-secondary"
              onClick={openImport}
              disabled={readOnly || importBusy}
            >
              <Icon name="upload" />
              Import .env…
            </button>
            {readOnly && <p className="field-note">Upgrade the vault format before importing.</p>}
          </div>
        </aside>
      </div>

      {importing && (
        <div
          className="overlay overlay-drawer"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !importBusy) closeImport();
          }}
        >
          <div
            ref={importDialogRef}
            tabIndex={-1}
            className="drawer raised"
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
              <div className="drawer-body">
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

                <div className="form-grid import-scope">
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
                    <label htmlFor="import-environment">Target environment</label>
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

                <div className="drawer-notes">
                  <Notice tone="info">
                    Only keys in this exact project and environment can be updated.
                    Empty values are rejected. AKM then opens a confirmation window
                    that lists the entry names. Nothing is saved until you confirm there.
                  </Notice>

                  {importBusy && <Notice tone="info">Waiting for your answer in the AKM confirmation window. Nothing is saved until you confirm there.</Notice>}
                  {importNote && <Notice tone="info">{importNote}</Notice>}
                  {importError && <Notice tone="error">{importError}</Notice>}
                </div>
              </div>

              <div className="drawer-foot end">
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
