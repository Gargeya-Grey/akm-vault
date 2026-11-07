import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { api, errorText } from "../api";
import {
  expiryLabel,
  keyState,
  plural,
  rotationLabel,
  statusDescription,
} from "../format";
import { attentionOf } from "../reminders";
import type { KeyView, RotationReceipt } from "../types";
import { useNow } from "../useNow";
import { FilterSelect, uniqueOptions } from "./FilterSelect";
import { AttentionSummary } from "./AttentionSummary";
import { KeyDetail } from "./KeyDetail";
import { Icon } from "./Icon";
import { KeyForm } from "./KeyForm";
import { RotateDialog } from "./RotateDialog";
import { ConfirmDialog, Notice } from "./Notice";

interface SummaryControl {
  dismissed: boolean;
  onDismiss: () => void;
}

/**
 * `summary` is owned by the app, so that a dismissal outlives this view until the next unlock.
 * Without it the view keeps its own flag.
 */
export function KeysView({ readOnly = false, summary }: { readOnly?: boolean; summary?: SummaryControl }) {
  const [summaryDismissed, setSummaryDismissed] = useState(false);
  const [keys, setKeys] = useState<KeyView[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [project, setProject] = useState("");
  const [environment, setEnvironment] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [formItem, setFormItem] = useState<KeyView | null | undefined>(
    undefined,
  );
  const [deleting, setDeleting] = useState<KeyView | null>(null);
  const [deletingBusy, setDeletingBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [rotating, setRotating] = useState<KeyView | null>(null);
  const now = useNow(keys);
  const statusId = useId();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setKeys(await api.keys());
      setError(null);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!flash) return;
    const id = window.setTimeout(() => setFlash(null), 6000);
    return () => window.clearTimeout(id);
  }, [flash]);

  const projects = useMemo(
    () => uniqueOptions(keys.map((key) => key.project)),
    [keys],
  );
  const environments = useMemo(
    () => uniqueOptions(keys.map((key) => key.environment)),
    [keys],
  );

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return keys
      .filter((key) => {
        if (project && key.project !== project) return false;
        if (environment && key.environment !== environment) return false;
        if (!needle) return true;
        return [
          key.title,
          key.issuer,
          key.project,
          key.environment,
          key.resolvedEnvVar,
          key.url,
          key.notes,
          ...key.scopes,
          ...key.tags,
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase()
          .includes(needle);
      })
      .sort((a, b) => a.title.localeCompare(b.title));
  }, [keys, query, project, environment]);

  const selected = keys.find((key) => key.id === selectedId) ?? null;
  const attention = attentionOf(keys, now);
  const showSummary = !loading && attention.total > 0 && !(summary?.dismissed ?? summaryDismissed);

  function clearFilters() {
    setQuery("");
    setProject("");
    setEnvironment("");
  }

  function saved(item: KeyView) {
    setKeys((current) => {
      const next = current.filter((key) => key.id !== item.id);
      return [...next, item].sort((a, b) => a.title.localeCompare(b.title));
    });
    setSelectedId(item.id);
    setFormItem(undefined);
    setFlash(`Saved “${item.title}”. The vault file has been rewritten.`);
  }

  function rotated(receipt: RotationReceipt) {
    setKeys((current) =>
      current.map((key) => (key.id === receipt.key.id ? receipt.key : key)),
    );
    setSelectedId(receipt.key.id);
    setRotating(null);
    setFlash(
      `Rotated “${receipt.key.title}”. The old secret stays in the vault history. Rotating here does not change the secret at the provider.`,
    );
  }

  async function remove() {
    if (!deleting) return;
    setDeletingBusy(true);
    setDeleteError(null);
    try {
      await api.remove(deleting.id);
      setKeys((current) => current.filter((key) => key.id !== deleting.id));
      if (selectedId === deleting.id) setSelectedId(null);
      setFlash(`Moved “${deleting.title}” to the Recycle Bin. Restore it there, or delete it for good. Check backup notices before you rely on encrypted backups.`);
      setDeleting(null);
    } catch (cause) {
      setDeleteError(errorText(cause));
    } finally {
      setDeletingBusy(false);
    }
  }

  return (
    <main className="wrap view">
      <header className="view-head">
        <div>
          <h1>
            Keys, <em>accounted for.</em>
          </h1>
          <p className="lead">
            Search what this vault holds, reveal only when you mean to, and see
            what needs rotating. The file stays a normal KeePass vault.
          </p>
        </div>
        <button
          type="button"
          className="btn"
          onClick={() => setFormItem(null)}
          disabled={readOnly}
        >
          <Icon name="plus" />
          Add key
        </button>
      </header>

      {showSummary && (
        <AttentionSummary
          attention={attention}
          onDismiss={summary?.onDismiss ?? (() => setSummaryDismissed(true))}
        />
      )}

      <div className="stat-row" aria-label="Vault summary">
        <div className="stat">
          <span className="stat-num mono">{keys.length}</span>
          <span className="stat-label">
            {keys.length === 1 ? "key" : "keys"} in this vault
          </span>
        </div>
        <div className="stat">
          <span className="stat-num mono">{projects.length}</span>
          <span className="stat-label">
            {projects.length === 1 ? "project" : "projects"}
          </span>
        </div>
        <div className="stat">
          <span className={`stat-num mono${attention.total > 0 ? " danger-text" : ""}`}>
            {attention.total}
          </span>
          <span className="stat-label">need attention</span>
        </div>
        <div className="stat">
          <span className="stat-num mono">{visible.length}</span>
          <span className="stat-label">shown now</span>
        </div>
      </div>

      <section className="raised ledger" aria-label="API keys">
        <div className="ledger-tools">
          <div className="search">
            <Icon name="search" />
            <label className="sr-only" htmlFor="key-search">
              Search keys
            </label>
            <input
              id="key-search"
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search title, project, env var…"
              autoComplete="off"
              data-focus-fallback=""
            />
          </div>
          <FilterSelect
            id="filter-project"
            label="Project"
            value={project}
            placeholder="All projects"
            options={projects}
            onChange={setProject}
          />
          <FilterSelect
            id="filter-environment"
            label="Environment"
            value={environment}
            placeholder="All environments"
            options={environments}
            onChange={setEnvironment}
          />
        </div>

        {error && (
          <div className="ledger-message">
            <Notice tone="error">{error}</Notice>
          </div>
        )}
        {flash && (
          <div className="ledger-message">
            <Notice tone="ok">{flash}</Notice>
          </div>
        )}

        <div className="key-head" aria-hidden="true">
          <span>Key</span>
          <span>Project</span>
          <span>Env var</span>
          <span>Expiry</span>
          <span>Status</span>
          <span />
        </div>

        {loading ? (
          <div className="empty-state">
            <h2>Opening the index…</h2>
            <p>The vault is already unlocked. Keys appear here in a moment.</p>
          </div>
        ) : keys.length === 0 ? (
          <div className="empty-state">
            <h2>No keys yet.</h2>
            <p>
              Add the first one and it will appear here, with its project,
              environment variable and rotation date.
            </p>
            <button
              type="button"
              className="text-button"
              onClick={() => setFormItem(null)}
              disabled={readOnly}
            >
              <Icon name="plus" />
              Add the first key
            </button>
          </div>
        ) : visible.length === 0 ? (
          <div className="empty-state">
            <h2>Nothing matches those filters.</h2>
            <p>{plural(keys.length, "key")} in the vault, none shown here.</p>
            <button type="button" className="text-button" onClick={clearFilters}>
              Clear search and filters
            </button>
          </div>
        ) : (
          <ul className="key-rows">
            {visible.map((item) => {
              const state = keyState(item, now);
              return (
                <li key={item.id}>
                  <button
                    type="button"
                    className="key-row"
                    onClick={() => setSelectedId(item.id)}
                    aria-label={`Open ${item.title}`}
                    aria-describedby={`${statusId}-${item.id}`}
                  >
                    <span className="key-ident">
                      <span className="key-title">{item.title}</span>
                      <span className="key-sub">
                        {item.issuer ?? "No issuer"}
                        {item.scopes.length > 0 &&
                          ` · ${plural(item.scopes.length, "scope")}`}
                      </span>
                    </span>
                    <span className="key-meta">
                      <span>{item.project ?? "No project"}</span>
                      <span className="key-env">
                        {item.environment ?? "—"}
                      </span>
                    </span>
                    <span className="key-var mono">{item.resolvedEnvVar}</span>
                    <span className="key-expiry">
                      <span className="mono">{expiryLabel(item)}</span>
                      <span className="key-sub">
                        {item.rotationDays === null
                          ? "no rotation"
                          : `rotate ${rotationLabel(item, now)}`}
                      </span>
                    </span>
                    <span className={`state state-${state.tone}`}>
                      <span className="state-dot" aria-hidden="true" />
                      {state.label}
                    </span>
                    <span className="row-arrow" aria-hidden="true">
                      <Icon name="arrowRight" />
                    </span>
                  </button>
                  <span className="sr-only" id={`${statusId}-${item.id}`}>
                    {statusDescription(item, now)}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {selected && (
        <KeyDetail
          item={selected}
          readOnly={readOnly}
          onClose={() => setSelectedId(null)}
          onEdit={() => {
            setSelectedId(null);
            setFormItem(selected);
          }}
          onRotate={() => {
            setSelectedId(null);
            setRotating(selected);
          }}
          onDelete={() => {
            setDeleteError(null);
            setSelectedId(null);
            setDeleting(selected);
          }}
        />
      )}

      {formItem !== undefined && (
        <KeyForm
          item={formItem ?? undefined}
          onSaved={saved}
          onClose={() => setFormItem(undefined)}
        />
      )}

      {rotating && (
        <RotateDialog
          item={rotating}
          onRotated={rotated}
          onClose={() => { setSelectedId(rotating.id); setRotating(null); }}
        />
      )}

      {deleting && (
        <ConfirmDialog
          title="Move this key to the Recycle Bin?"
          confirmLabel="Move to Recycle Bin"
          busy={deletingBusy}
          destructive={false}
          onConfirm={remove}
          onCancel={() => { setSelectedId(deleting.id); setDeleting(null); }}
        >
          <p>
            <strong>{deleting.title}</strong> leaves the key list and moves to the Recycle Bin
            inside the vault. Its secret, history and group are kept. Open the Recycle Bin to
            restore it, or to delete it for good. Nothing is deleted for good until you ask.
          </p>
          {deleteError && <Notice tone="error">{deleteError}</Notice>}
        </ConfirmDialog>
      )}
    </main>
  );
}
