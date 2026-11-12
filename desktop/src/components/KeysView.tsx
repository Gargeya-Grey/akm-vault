import { Fragment, useEffect, useId, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { nativeReads, useNativeRead } from "../nativeRead";
import {
  expiryLabel,
  keyState,
  needsAttention,
  plural,
  rotationLabel,
  rotationUsed,
  statusDescription,
} from "../format";
import { attentionOf } from "../reminders";
import type { KeyView, RotationReceipt } from "../types";
import { useNow } from "../useNow";
import { FilterSelect, uniqueOptions } from "./FilterSelect";
import { AttentionSummary } from "./AttentionSummary";
import { EmptyHooks, KeyCut, LifeRule, NoMatch } from "./glyphs";
import { KeyDetail } from "./KeyDetail";
import { Icon } from "./Icon";
import { KeyForm } from "./KeyForm";
import { RotateDialog } from "./RotateDialog";
import { ConfirmDialog, Notice } from "./Notice";
import { ReadFailure } from "./ReadFailure";

interface SummaryControl {
  dismissed: boolean;
  onDismiss: () => void;
}

type Shelf =
  | { kind: "all" }
  | { kind: "attention" }
  | { kind: "project"; name: string | null };

const ALL_KEYS: Shelf = { kind: "all" };
const NO_KEYS: KeyView[] = [];

function identityDescription(key: KeyView): string {
  const project = key.project === null ? "No project" : `Project ${key.project}`;
  const environment = key.environment === null ? "no environment" : `environment ${key.environment}`;
  const issuer = key.issuer === null ? "no issuer" : `issuer ${key.issuer}`;
  return `${project}, ${environment}, variable ${key.resolvedEnvVar}, ${issuer}, ID ${key.id}.`;
}

function shelfTitle(shelf: Shelf): string {
  if (shelf.kind === "all") return "All keys";
  if (shelf.kind === "attention") return "Needs attention";
  return shelf.name ?? "No project";
}

/**
 * `summary` is owned by the app, so that a dismissal outlives this view until the next unlock.
 * Without it the view keeps its own flag.
 */
export function KeysView({ readOnly = false, summary }: { readOnly?: boolean; summary?: SummaryControl }) {
  const [summaryDismissed, setSummaryDismissed] = useState(false);
  const read = useNativeRead(nativeReads.keys);
  // Only a read that succeeded gives keys. Every count and every "empty" text comes from here.
  const keys = read.state.status === "ready" ? read.state.data : null;
  const all = keys ?? NO_KEYS;
  const [flash, setFlash] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [chosenShelf, setChosenShelf] = useState<Shelf>(ALL_KEYS);
  const [environment, setEnvironment] = useState("");
  // Identities, never copies: each dialog draws the key as the newest read gives it.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [formItem, setFormItem] = useState<KeyView | null | undefined>(
    undefined,
  );
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [deletingBusy, setDeletingBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [rotatingId, setRotatingId] = useState<string | null>(null);
  const now = useNow(all);
  const statusId = useId();

  useEffect(() => {
    if (!flash) return;
    const id = window.setTimeout(() => setFlash(null), 6000);
    return () => window.clearTimeout(id);
  }, [flash]);

  const attention = attentionOf(all, now);
  // A shelf empties when its last key leaves. The view then falls back, so nobody is stranded on it.
  const shelf: Shelf =
    (chosenShelf.kind === "attention" && attention.total === 0) ||
    (chosenShelf.kind === "project" && !all.some((key) => key.project === chosenShelf.name))
      ? ALL_KEYS
      : chosenShelf;

  const projects = useMemo(
    () => uniqueOptions(all.map((key) => key.project)),
    [all],
  );
  const environments = useMemo(
    () => uniqueOptions(all.map((key) => key.environment)),
    [all],
  );
  const unfiled = all.filter((key) => key.project === null).length;

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return all
      .filter((key) => {
        if (shelf.kind === "attention" && !needsAttention(key, now)) return false;
        if (shelf.kind === "project" && key.project !== shelf.name) return false;
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
  }, [all, query, shelf, environment, now]);

  const groups = useMemo(() => {
    const byProject = new Map<string | null, KeyView[]>();
    for (const key of visible) {
      byProject.set(key.project, [...(byProject.get(key.project) ?? []), key]);
    }
    return [...byProject].sort(([a], [b]) => (a === null ? 1 : b === null ? -1 : a.localeCompare(b)));
  }, [visible]);
  // Rows carry no project column, so the heading is the only place that names the project.
  // On a project shelf the page title names it.
  const showGroupNames = shelf.kind !== "project";

  // Two rows that match in every shown fact need the one fact that is unique.
  const lookalikes = useMemo(() => {
    const seen = new Map<string, string[]>();
    for (const key of all) {
      const shown = JSON.stringify([key.title, key.issuer, key.project, key.environment, key.resolvedEnvVar]);
      seen.set(shown, [...(seen.get(shown) ?? []), key.id]);
    }
    return new Set([...seen.values()].filter((ids) => ids.length > 1).flat());
  }, [all]);

  const find = (id: string | null) => (id === null ? null : all.find((key) => key.id === id) ?? null);
  const selected = find(selectedId);
  const deleting = find(deletingId);
  const rotating = find(rotatingId);
  const showSummary = keys !== null && attention.total > 0 && !(summary?.dismissed ?? summaryDismissed);

  function clearFilters() {
    setQuery("");
    setChosenShelf(ALL_KEYS);
    setEnvironment("");
  }

  function saved(item: KeyView) {
    read.apply((current) => [...current.filter((key) => key.id !== item.id), item]);
    setSelectedId(item.id);
    setFormItem(undefined);
    setFlash(`Saved “${item.title}”. The vault file has been rewritten.`);
  }

  function rotated(receipt: RotationReceipt) {
    read.apply((current) => current.map((key) => (key.id === receipt.key.id ? receipt.key : key)));
    setSelectedId(receipt.key.id);
    setRotatingId(null);
    setFlash(
      `Rotated “${receipt.key.title}”. The old secret stays in the vault history. Rotating here does not change the secret at the provider.`,
    );
  }

  async function remove() {
    if (!deleting) return;
    const target = deleting;
    setDeletingBusy(true);
    setDeleteError(null);
    try {
      await api.remove(target.id);
      read.apply((current) => current.filter((key) => key.id !== target.id));
      if (selectedId === target.id) setSelectedId(null);
      setFlash(`Moved “${target.title}” to the Recycle Bin. Restore it there, or delete it for good. Check backup notices before you rely on encrypted backups.`);
      setDeletingId(null);
    } catch (cause) {
      setDeleteError(errorText(cause));
    } finally {
      setDeletingBusy(false);
    }
  }

  function renderRow(item: KeyView) {
    const state = keyState(item, now);
    const used = rotationUsed(item, now);
    return (
      <li className="key-line" key={item.id}>
        <button
          type="button"
          className="key-row"
          onClick={() => setSelectedId(item.id)}
          aria-label={`Open ${item.title}`}
          aria-describedby={`${statusId}-${item.id}`}
        >
          <KeyCut id={item.id} />
          <span className="key-ident">
            <span className="key-title">{item.title}</span>
            <span className="key-sub">
              {item.issuer ?? "No issuer"}
              {item.scopes.length > 0 && ` · ${plural(item.scopes.length, "scope")}`}
            </span>
            {lookalikes.has(item.id) && <span className="key-sub mono">ID {item.id}</span>}
          </span>
          <span className="key-ident">
            <span className="key-var mono">{item.resolvedEnvVar}</span>
            <span className="key-env">{item.environment ?? "—"}</span>
          </span>
          <span className="key-expiry">
            <span>
              {item.rotationDays === null ? "no rotation" : `rotate ${rotationLabel(item, now)}`}
            </span>
            <span className="key-when-line">
              {used !== null && <LifeRule used={used} tone={state.tone} />}
              <span className={`key-sub${item.expiresAt ? " mono" : ""}`}>{item.expiresAt ? expiryLabel(item) : "no expiry"}</span>
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
          {identityDescription(item)} {statusDescription(item, now)}
        </span>
      </li>
    );
  }

  const isShelf = (candidate: Shelf) =>
    candidate.kind === shelf.kind &&
    (candidate.kind !== "project" || (shelf.kind === "project" && candidate.name === shelf.name));

  const shelfButton = (target: Shelf, label: string, count: number) => (
    <button type="button" className="shelf-item" aria-pressed={isShelf(target)} onClick={() => setChosenShelf(target)}>
      <span>{label}</span>
      <span className="num" aria-hidden="true">{count}</span>
    </button>
  );

  return (
    <main className="keys-view">
      <div className="ledger-pane">
        <header className="list-head">
          <h1>
            {shelfTitle(shelf)} {keys !== null && <em className="num">{visible.length}</em>}
          </h1>
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

        {keys !== null && <nav className="shelf" aria-label="Projects">
          {shelfButton(ALL_KEYS, "All keys", keys.length)}
          <button
            type="button"
            className={`shelf-item shelf-attention${attention.total > 0 ? " warn" : ""}`}
            aria-pressed={isShelf({ kind: "attention" })}
            disabled={attention.total === 0}
            onClick={() => setChosenShelf({ kind: "attention" })}
          >
            <span className="num">{attention.total}</span>
            <span>need attention</span>
          </button>
          <p className="shelf-label">Projects <span className="num">{projects.length}</span></p>
          {projects.map((option) => (
            <Fragment key={option.value}>
              {shelfButton({ kind: "project", name: option.value }, option.label, keys.filter((key) => key.project === option.value).length)}
            </Fragment>
          ))}
          {unfiled > 0 && shelfButton({ kind: "project", name: null }, "No project", unfiled)}
        </nav>}

        {showSummary && (
          <AttentionSummary
            attention={attention}
            onDismiss={summary?.onDismiss ?? (() => setSummaryDismissed(true))}
          />
        )}

        <div className="list-tools">
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
            id="filter-environment"
            label="Environment"
            value={environment}
            placeholder="All environments"
            options={environments}
            onChange={setEnvironment}
          />
        </div>

        {flash && (
          <div className="ledger-message">
            <Notice tone="ok">{flash}</Notice>
          </div>
        )}

        <section aria-label="API keys">
          {keys !== null && visible.length > 0 && (
            <div className="key-head" aria-hidden="true">
              <span />
              <span>Key</span>
              <span>Variable</span>
              <span>Renewal</span>
              <span>Status</span>
              <span />
            </div>
          )}

          {read.state.status === "loading" ? (
            <div className="empty-state">
              <h2>Opening the index…</h2>
              <p>The vault is already unlocked. Keys appear here in a moment.</p>
            </div>
          ) : read.state.status === "failed" ? (
            <ReadFailure
              heading="AKM could not read the keys."
              detail="This does not mean that the vault is empty. Try again, or lock the vault and open it again."
              error={read.state.error}
              busy={read.busy}
              onRetry={read.reload}
            />
          ) : read.state.data.length === 0 ? (
            <div className="empty-state">
              <EmptyHooks />
              <h2>No keys yet.</h2>
              <p>
                Add the first one and it will appear here, with its project,
                environment variable and rotation date.
              </p>
              <div className="empty-actions">
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
            </div>
          ) : visible.length === 0 ? (
            <div className="empty-state">
              <NoMatch />
              <h2>Nothing matches those filters.</h2>
              <p>{plural(read.state.data.length, "key")} in the vault, none shown here.</p>
              <div className="empty-actions">
                <button type="button" className="text-button" onClick={clearFilters}>
                  Clear search and filters
                </button>
              </div>
            </div>
          ) : (
            groups.map(([name, items]) => (
              <section className="key-group" key={name === null ? "no-project" : `project:${name}`}>
                {showGroupNames && (
                  <h2 className="group-head">
                    {name ?? "No project"} <span className="num" aria-hidden="true">{items.length}</span>
                  </h2>
                )}
                <ul className="key-rows">{items.map(renderRow)}</ul>
              </section>
            ))
          )}
        </section>
      </div>

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
            setRotatingId(selected.id);
          }}
          onDelete={() => {
            setDeleteError(null);
            setSelectedId(null);
            setDeletingId(selected.id);
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
          onClose={() => { setSelectedId(rotating.id); setRotatingId(null); }}
        />
      )}

      {deleting && (
        <ConfirmDialog
          title="Move this key to the Recycle Bin?"
          confirmLabel="Move to Recycle Bin"
          busy={deletingBusy}
          destructive={false}
          onConfirm={remove}
          onCancel={() => { setSelectedId(deleting.id); setDeletingId(null); }}
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
