import { Fragment, useEffect, useId, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { nativeReads, useNativeRead } from "../nativeRead";
import { needsAttention, plural, rotationUsed, statusDescription } from "../format";
import { attentionOf } from "../reminders";
import { keyRowDescription, keyRowFacts, namedKey, NO_PROJECT, searchText, useShortIds, type KeyRowFacts } from "../rowFacts";
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
import { KeyTarget } from "./FactList";
import { ReadFailure } from "./ReadFailure";

interface SummaryControl {
  dismissed: boolean;
  onDismiss: () => void;
}

type Shelf =
  | { kind: "all" }
  | { kind: "attention" }
  // The project as the row draws it: an unset project and a literal "No project" share a shelf.
  | { kind: "project"; name: string };

const ALL_KEYS: Shelf = { kind: "all" };
const NO_KEYS: KeyView[] = [];

/**
 * The page title of a shelf. A project shelf says "Project" before the name, so a project named
 * "All keys" or "Needs attention" never draws or reads like that shelf.
 */
function shelfTitle(shelf: Shelf) {
  if (shelf.kind === "all") return "All keys";
  if (shelf.kind === "attention") return "Needs attention";
  if (shelf.name === NO_PROJECT) return NO_PROJECT;
  return <><span className="title-kind">Project</span> <bdi>{shelf.name}</bdi></>;
}

const NO_FACTS = new Map<string, KeyRowFacts>();

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

  // What each row draws. Every shelf, group, filter and search reads these.
  const shortId = useShortIds(all.map((key) => key.id));
  const factsById = useMemo(() => (all.length === 0 ? NO_FACTS : new Map(all.map((key) => [key.id, keyRowFacts(key, now, shortId)]))), [all, now, shortId]);
  const factsOf = (key: KeyView) => factsById.get(key.id) ?? keyRowFacts(key, now, shortId);

  const attention = attentionOf(all, now);
  // A shelf empties when its last key leaves. The view then falls back, so nobody is stranded on it.
  const shelf: Shelf =
    (chosenShelf.kind === "attention" && attention.total === 0) ||
    (chosenShelf.kind === "project" && !all.some((key) => factsOf(key).project.text === chosenShelf.name))
      ? ALL_KEYS
      : chosenShelf;

  const projects = useMemo(
    () => uniqueOptions([...factsById.values()].map((facts) => facts.project.text), (text) => text).filter((option) => option.value !== NO_PROJECT),
    [factsById],
  );
  const environments = useMemo(
    () => uniqueOptions([...factsById.values()].map((facts) => facts.environment.text), (text) => text),
    [factsById],
  );
  const unfiled = [...factsById.values()].filter((facts) => facts.project.text === NO_PROJECT).length;

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return all
      .filter((key) => {
        const facts = factsById.get(key.id);
        if (!facts) return false;
        if (shelf.kind === "attention" && !needsAttention(key, now)) return false;
        if (shelf.kind === "project" && facts.project.text !== shelf.name) return false;
        if (environment && facts.environment.text !== environment) return false;
        if (!needle) return true;
        return searchText(key, facts).includes(needle);
      })
      .sort((a, b) => a.title.localeCompare(b.title));
  }, [all, factsById, query, shelf, environment, now]);

  const groups = useMemo(() => {
    const byProject = new Map<string, KeyView[]>();
    for (const key of visible) {
      const name = factsById.get(key.id)?.project.text ?? NO_PROJECT;
      byProject.set(name, [...(byProject.get(name) ?? []), key]);
    }
    return [...byProject].sort(([a], [b]) => (a === NO_PROJECT ? 1 : b === NO_PROJECT ? -1 : a.localeCompare(b)));
  }, [visible, factsById]);
  // Rows carry no project column, so the heading is the only place that names the project.
  // On a project shelf the page title names it.
  const showGroupNames = shelf.kind !== "project";

  const find = (id: string | null) => (id === null ? null : all.find((key) => key.id === id) ?? null);
  const selected = find(selectedId);
  const deleting = find(deletingId);
  const deletingFacts = deleting && factsOf(deleting);
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
    setFlash(`Saved ${namedKey(factsOf(item))}. The vault file has been rewritten.`);
  }

  function rotated(receipt: RotationReceipt) {
    read.apply((current) => current.map((key) => (key.id === receipt.key.id ? receipt.key : key)));
    setSelectedId(receipt.key.id);
    setRotatingId(null);
    setFlash(
      `Rotated ${namedKey(factsOf(receipt.key))}. The old secret stays in the vault history. Rotating here does not change the secret at the provider.`,
    );
  }

  async function remove() {
    if (!deleting) return;
    const target = deleting;
    const name = namedKey(factsOf(target));
    setDeletingBusy(true);
    setDeleteError(null);
    try {
      await api.remove(target.id);
      read.apply((current) => current.filter((key) => key.id !== target.id));
      setSelectedId((current) => (current === target.id ? null : current));
      setFlash(`Moved ${name} to the Recycle Bin. Restore it there, or delete it for good. Check backup notices before you rely on encrypted backups.`);
      setDeletingId(null);
    } catch (cause) {
      setDeleteError(errorText(cause));
    } finally {
      setDeletingBusy(false);
    }
  }

  function renderRow(item: KeyView) {
    const facts = factsOf(item);
    const used = rotationUsed(item, now);
    return (
      <li className="key-line" key={item.id}>
        <button
          type="button"
          className="key-row"
          onClick={() => setSelectedId(item.id)}
          aria-label={`Open ${facts.title}`}
          aria-describedby={`${statusId}-${item.id}`}
        >
          <KeyCut id={item.id} />
          <span className="key-ident">
            <span className="key-title">{facts.title}</span>
            <span className="key-sub">
              <bdi>{facts.issuer.text}</bdi>
              {facts.scopes.set && ` · ${facts.scopes.text}`}
            </span>
            <span className="key-sub key-id mono">ID {facts.id.text}</span>
          </span>
          <span className="key-ident">
            <span className="key-var mono">{facts.variable.text}</span>
            <span className="key-env">{facts.environment.text}</span>
          </span>
          <span className="key-expiry">
            <span>{facts.renewal}</span>
            <span className="key-when-line">
              {used !== null && <LifeRule used={used} tone={facts.tone} />}
              <span className={`key-sub${item.expiresAt ? " mono" : ""}`}>{facts.expiry}</span>
            </span>
          </span>
          <span className={`state state-${facts.tone}`}>
            <span className="state-dot" aria-hidden="true" />
            {facts.state}
          </span>
          <span className="row-arrow" aria-hidden="true">
            <Icon name="arrowRight" />
          </span>
        </button>
        <span className="sr-only" id={`${statusId}-${item.id}`}>
          {keyRowDescription(facts)} {statusDescription(item, now)}
        </span>
      </li>
    );
  }

  const isShelf = (candidate: Shelf) =>
    candidate.kind === shelf.kind &&
    (candidate.kind !== "project" || (shelf.kind === "project" && candidate.name === shelf.name));

  // A project button reads "Project" before its name, as the page title of its shelf does.
  const shelfButton = (target: Shelf, label: string, count: number) => (
    <button
      type="button"
      className="shelf-item"
      aria-pressed={isShelf(target)}
      aria-label={target.kind === "project" && target.name !== NO_PROJECT ? `Project ${label}` : undefined}
      onClick={() => setChosenShelf(target)}
    >
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
              {shelfButton({ kind: "project", name: option.value }, option.label, keys.filter((key) => factsOf(key).project.text === option.value).length)}
            </Fragment>
          ))}
          {unfiled > 0 && shelfButton({ kind: "project", name: NO_PROJECT }, NO_PROJECT, unfiled)}
        </nav>}

        {showSummary && (
          <AttentionSummary
            attention={attention}
            nameOf={(key) => namedKey(factsOf(key))}
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
              <section className="key-group" key={`project:${name}`}>
                {showGroupNames && (
                  <h2 className="group-head">
                    {name !== NO_PROJECT && <><span className="sr-only">Project</span> </>}
                    <bdi>{name}</bdi> <span className="num" aria-hidden="true">{items.length}</span>
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
          current={read.state.status === "ready" && !read.busy}
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

      {deleting && deletingFacts && (
        <ConfirmDialog
          title="Move this key to the Recycle Bin?"
          confirmLabel="Move to Recycle Bin"
          busy={deletingBusy}
          destructive={false}
          onConfirm={remove}
          onCancel={() => { setSelectedId(deleting.id); setDeletingId(null); }}
        >
          <p>
            <strong><bdi>{deletingFacts.title}</bdi></strong> leaves the key list and moves to the Recycle Bin
            inside the vault. Its secret, history and group are kept. Open the Recycle Bin to
            restore it, or to delete it for good. Nothing is deleted for good until you ask.
          </p>
          <KeyTarget facts={deletingFacts} />
          {deleteError && <Notice tone="error">{deleteError}</Notice>}
        </ConfirmDialog>
      )}
    </main>
  );
}
