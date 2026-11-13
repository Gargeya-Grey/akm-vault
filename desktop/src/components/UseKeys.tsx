import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { clipboardCopyText } from "../clipboardText";
import { nativeReads, useNativeRead } from "../nativeRead";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { receiptState, useReceipts, waitingReceipt, type Outcome } from "../sessionBoundary";
import { plural } from "../format";
import { FilterSelect, uniqueOptions } from "./FilterSelect";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { ReadFailure } from "./ReadFailure";
import { useDialogFocus } from "./useDialogFocus";

interface Message {
  tone: "ok" | "error";
  text: string;
}

type Scope = { project: string; environment: string };
type ExportPreview = Scope & { text: string };
/** A request that this view dispatched, with the receipt that owns its state (U2-R1-03). */
type Tracked = { receipt: number; request: number };

// What the form shows when the native record, or a failure in transport, settled the launch. A
// reply that settled it shows its own text.
const LAUNCH_SETTLED: Record<Exclude<Outcome, "pending">, Message> = {
  completed: { tone: "ok", text: "A terminal opened with keys. That window keeps them until you close it, even after AKM locks." },
  completedWhileLocked: { tone: "ok", text: "A terminal opened with keys, but its session ended before the result arrived. That window keeps them until you close it, even after AKM locks." },
  cancelled: { tone: "ok", text: "Nothing was opened. You cancelled in the folder window or the confirmation window." },
  reportedError: { tone: "error", text: "The terminal launch failed. No terminal from that request runs." },
  unconfirmed: { tone: "error", text: "A terminal launch could not be confirmed after the session changed. A terminal may hold keys. Check for an open terminal before you try again." },
  lost: { tone: "error", text: "A terminal launch sent no answer to this window. A terminal may hold keys. A saved-operation notice gives the outcome when AKM recorded one. Check for an open terminal before you try again." },
};
const IMPORT_SAVED = "The import was saved. This window did not receive the counts, so check the keys before you import again.";
const IMPORT_SESSION_ENDED = "The import was saved, but its session ended before the result arrived. Do not repeat it. Open the vault and check the keys.";
const IMPORT_DECLINED = "Nothing was imported. You declined it in the confirmation window.";
const IMPORT_FAILED = "The import reported an error. This does not confirm that nothing was saved. Check the keys before you try again.";
const IMPORT_LOST = "The import sent no answer to this window. This does not confirm that nothing was saved. A saved-operation notice gives the outcome when AKM recorded one.";
const IMPORT_UNCONFIRMED = "The import could not be confirmed after the session changed. Check the keys before you try again.";

/** True when nothing but the reply settled the receipt, so the reply is the outcome to show. */
function replySettled(receipt: number | null): boolean {
  return receipt === null || receiptState(receipt)?.by === "reply";
}

export function UseKeys({ readOnly = false }: { readOnly?: boolean }) {
  const read = useNativeRead(nativeReads.keys);
  const keys = read.state.status === "ready" ? read.state.data : null;
  const [chosenProject, setProject] = useState("");
  const [chosenEnvironment, setEnvironment] = useState("");
  const [preview, setPreview] = useState<ExportPreview | null>(null);
  const exportRequest = useRef(0);
  const sideEffectRequest = useRef(0);
  const importRequest = useRef(0);
  const mounted = useRef(true);
  // A preview and a copy have no receipt, so their wait stays here.
  const [busy, setBusy] = useState<"preview" | "copy" | null>(null);
  const [message, setMessage] = useState<Message | null>(null);

  const [importing, setImporting] = useState(false);
  const importDialogRef = useDialogFocus<HTMLDivElement>(importing);
  const [importText, setImportText] = useState("");
  const [importProject, setImportProject] = useState("");
  const [importEnvironment, setImportEnvironment] = useState("");
  const [importError, setImportError] = useState<string | null>(null);
  const [importNote, setImportNote] = useState<string | null>(null);
  const [launch, setLaunch] = useState<Tracked | null>(null);
  const [importOperation, setImportOperation] = useState<Tracked | null>(null);

  // A form waits while a receipt of its operation waits in this session. The receipt is the one
  // owner: a hide, an expiry or a new mount of this view does not end the wait (U2-R1-03).
  const receiptsVersion = useReceipts();
  const launching = waitingReceipt("terminal") !== null;
  const saving = waitingReceipt("export") !== null;
  const importBusy = waitingReceipt("import") !== null;
  const working = busy ?? (launching ? "launch" : saving ? "save" : null);

  const hidePreview = useCallback(() => {
    exportRequest.current += 1;
    setPreview(null);
    setBusy((current) => current === "preview" ? null : current);
  }, []);

  const editImport = useCallback(() => {
    importRequest.current += 1;
    setImportError(null);
    setImportNote(null);
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

  // A launch that the record or a transport failure settled: the form shows that outcome.
  useEffect(() => {
    if (launch === null) return;
    const state = receiptState(launch.receipt);
    if (state?.outcome === "pending") return;
    setLaunch(null);
    if (state === null || state.by === "reply" || launch.request !== sideEffectRequest.current) return;
    setMessage(LAUNCH_SETTLED[state.outcome as Exclude<Outcome, "pending">]);
  }, [receiptsVersion, launch]);

  // An import changed the vault whoever settled it, so the list reads again. When the record or a
  // transport failure settled it, the form shows that outcome.
  const reloadKeys = read.reload;
  useEffect(() => {
    if (importOperation === null) return;
    const state = receiptState(importOperation.receipt);
    if (state?.outcome === "pending") return;
    setImportOperation(null);
    if (state === null) return;
    const saved = state.outcome === "completed" || state.outcome === "completedWhileLocked";
    if (saved) reloadKeys();
    if (state.by === "reply" || importOperation.request !== importRequest.current) return;
    if (saved) {
      setImporting(false);
      setImportText("");
      setMessage({ tone: "ok", text: state.outcome === "completed" ? IMPORT_SAVED : IMPORT_SESSION_ENDED });
    } else if (state.outcome === "cancelled") {
      setImportNote(IMPORT_DECLINED);
    } else {
      const text = state.outcome === "lost" ? IMPORT_LOST : state.outcome === "unconfirmed" ? IMPORT_UNCONFIRMED : IMPORT_FAILED;
      setImportError(text);
      setMessage({ tone: "error", text });
    }
  }, [receiptsVersion, importOperation, reloadKeys]);

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
    () => uniqueOptions((keys ?? []).map((key) => key.project)),
    [keys],
  );
  // A choice counts only while its option is on screen. A refresh that removes it, or a read that
  // failed, leaves no scope, so no control acts on a scope that the screen does not show.
  const project = projects.some((option) => option.value === chosenProject) ? chosenProject : "";
  const environments = useMemo(
    () => uniqueOptions((keys ?? []).filter((key) => key.project === project).map((key) => key.environment)),
    [keys, project],
  );
  const environment = environments.some((option) => option.value === chosenEnvironment) ? chosenEnvironment : "";
  const scoped = Boolean(project && environment);
  const matchCount = (keys ?? []).filter((key) => key.project === project && key.environment === environment).length;
  // A preview shows only under the scope that it belongs to (fault class B).
  const shownPreview = preview !== null && preview.project === project && preview.environment === environment ? preview : null;

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
      if (text === null) {
        setPreview(null);
        setMessage({ tone: "ok", text: "Cancelled. No values were shown." });
        return;
      }
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
        text: seconds === null ? "Cancelled. Nothing was copied." : clipboardCopyText(seconds),
      });
    } catch (cause) {
      if (currentRequest === sideEffectRequest.current) setMessage({ tone: "error", text: errorText(cause) });
    } finally {
      if (currentRequest === sideEffectRequest.current) setBusy(null);
    }
  }

  async function saveEnv() {
    if (!scoped || working !== null) return;
    const currentRequest = ++sideEffectRequest.current;
    setMessage(null);
    try {
      const savedPath = await api.saveEnv(project, environment);
      if (currentRequest !== sideEffectRequest.current) return;
      if (savedPath) setMessage({ tone: "ok", text: `Saved plaintext secrets to ${savedPath}. Keep this file out of source control and cloud sharing.` });
    } catch (cause) {
      if (currentRequest === sideEffectRequest.current) setMessage({ tone: "error", text: errorText(cause) });
    }
  }

  async function openTerminal() {
    if (!scoped || working !== null) return;
    const currentRequest = ++sideEffectRequest.current;
    setMessage(null);
    const reply = api.launchTerminal(project, environment);
    const receipt = reply.receipt;
    if (receipt !== null) setLaunch({ receipt, request: currentRequest });
    try {
      const count = await reply;
      if (currentRequest !== sideEffectRequest.current || !replySettled(receipt)) return;
      setMessage(
        count === null
          ? { tone: "ok", text: "Nothing was opened. You cancelled in the folder window or the confirmation window." }
          : { tone: "ok", text: `Opened a terminal with ${plural(count, "key")}. That window keeps them until you close it, even after AKM locks.` },
      );
    } catch (cause) {
      if (currentRequest === sideEffectRequest.current && replySettled(receipt)) setMessage({ tone: "error", text: errorText(cause) });
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
    const reply = api.importEnv(importText, importProject.trim(), importEnvironment.trim());
    const receipt = reply.receipt;
    if (receipt !== null) setImportOperation({ receipt, request: currentRequest });
    try {
      const report = await reply;
      if (currentRequest !== importRequest.current || !replySettled(receipt)) return;
      if (report === null) {
        setImportNote(IMPORT_DECLINED);
        return;
      }
      setImporting(false);
      setImportText("");
      setMessage({
        tone: "ok",
        text: `Imported: ${report.added} added, ${report.updated} updated.`,
      });
    } catch (cause) {
      if (currentRequest === importRequest.current && replySettled(receipt)) {
        setImportError(errorText(cause));
        setMessage({ tone: "error", text: errorText(cause) });
      }
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

      {read.state.status === "failed" && (
        <ReadFailure
          heading="AKM could not read the keys."
          detail="This does not mean that the vault is empty. The project and environment lists need this read. Try again."
          error={read.state.error}
          busy={read.busy}
          onRetry={read.reload}
        />
      )}

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
              disabled={keys === null || working === "save" || working === "copy" || working === "launch"}
            />
            <FilterSelect
              id="env-environment"
              label="Environment"
              value={environment}
              placeholder="Choose an environment"
              options={environments}
              onChange={chooseEnvironment}
              disabled={keys === null || working === "save" || working === "copy" || working === "launch"}
            />
          </div>

          <div className="settings-actions">
            <button
              type="button"
              className="btn"
              onClick={() => void saveEnv()}
              disabled={working !== null || !scoped}
              aria-busy={working === "save" || undefined}
            >
              <Icon name="download" />
              {working === "save" ? "Saving…" : "Save .env…"}
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={() => void runPreview()}
              disabled={working !== null || !scoped}
              aria-busy={working === "preview" || undefined}
            >
              {working === "preview" ? "Previewing…" : "Preview"}
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={() => void copyEnv()}
              disabled={working !== null || !scoped}
              aria-busy={working === "copy" || undefined}
            >
              {working === "copy" ? "Copying…" : "Copy to clipboard"}
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={() => void openTerminal()}
              disabled={working !== null || !scoped}
              aria-busy={working === "launch" || undefined}
            >
              Open terminal with keys…
            </button>
          </div>

          <p className="field-note">
            {read.state.status === "loading"
              ? "Reading the projects in this vault…"
              : read.state.status === "failed"
              ? "Export needs the project list. Read it again first."
              : scoped ? `${plural(matchCount, "key")} selected in ${project} / ${environment}.` : "Choose a project and environment to enable export."}
          </p>
          <p className="field-note">
            Open terminal starts a command prompt in a folder that you choose. Every program you run there can read these keys until you close it.
          </p>

          {working === "launch" && (
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
            {shownPreview === null ? (
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
                  <p className="label">Preview: {shownPreview.project} / {shownPreview.environment}</p>
                  <p className="label">
                    {plural(
                      shownPreview.text.split("\n").filter((line) => line.trim()).length,
                      "line",
                    )}
                  </p>
                </div>
                <pre className="mono env-lines">
                  {shownPreview.text.trim() || "# No keys match this project and environment."}
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
            {importBusy && !importing && (
              <Notice tone="info">An import is waiting for your answer in the AKM confirmation window. Nothing is saved until you confirm there.</Notice>
            )}
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
