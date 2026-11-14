import { useEffect, useId, useState } from "react";
import { api, errorText } from "../api";
import { useKeyRowFacts } from "../rowFacts";
import { dateOnlyExpiry } from "../schedule";
import type { KeyDraft, KeyView } from "../types";
import { KeyTarget } from "./FactList";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";

interface Props {
  /** Present when editing; absent when adding. */
  item?: KeyView;
  /** Null when an edit changed nothing, so native code saved nothing. */
  onSaved: (item: KeyView | null) => void;
  /** The request failed. Its reply may have been lost after native code saved. */
  onFailed?: () => void;
  onClose: () => void;
}

/** The key that the edit form changes, as its row draws it (U2-R1-02). */
function EditTarget({ item, id }: { item: KeyView; id: string }) {
  const facts = useKeyRowFacts(item);
  return (
    <section className="form-group" aria-labelledby="form-group-target">
      <h3 id="form-group-target">This key</h3>
      <div id={id}>
        <p className="key-title"><bdi>{facts.title}</bdi></p>
        <KeyTarget facts={facts} />
      </div>
    </section>
  );
}

/** One scope on each line, so a comma is part of a scope, as each `--scope` of `akm edit` is one (FW-11). */
const SCOPE_SEPARATOR = /\r?\n/;
/** Where the vault splits its tags when it reads them, so the form shows what a reopen shows. */
const TAG_SEPARATOR = /[;,\t]/;

function splitList(value: string, separator: RegExp): string[] {
  const seen = new Set<string>();
  for (const part of value.split(separator)) {
    const trimmed = part.trim();
    if (trimmed) seen.add(trimmed);
  }
  return [...seen];
}

/** The longest rotation interval that the core accepts (`MAX_ROTATION_DAYS`). */
const MAX_ROTATION_DAYS = 36500;

/** The text that the form shows for each stored field. Some of it is a lossy view of the value, so
 * an edit sends only the fields whose text the user changed. */
function shownText(item?: KeyView) {
  return {
    title: item?.title ?? "",
    username: item?.username ?? "",
    issuer: item?.issuer ?? "",
    project: item?.project ?? "",
    environment: item?.environment ?? "",
    envVar: item?.envVar ?? "",
    url: item?.url ?? "",
    scopes: item?.scopes.join("\n") ?? "",
    tags: item?.tags.join(", ") ?? "",
    notes: item?.notes ?? "",
    expires: item?.expiresAt ? item.expiresAt.slice(0, 10) : "",
    rotation: item?.rotationDays !== null && item?.rotationDays !== undefined ? String(item.rotationDays) : "",
  };
}

export function KeyForm({ item, onSaved, onFailed, onClose }: Props) {
  const editing = item !== undefined;
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const targetId = useId();

  const [shown] = useState(() => shownText(item));
  const [title, setTitle] = useState(shown.title);
  const [secret, setSecret] = useState("");
  const [username, setUsername] = useState(shown.username);
  const [issuer, setIssuer] = useState(shown.issuer);
  const [project, setProject] = useState(shown.project);
  const [environment, setEnvironment] = useState(shown.environment);
  const [envVar, setEnvVar] = useState(shown.envVar);
  const [url, setUrl] = useState(shown.url);
  const [scopes, setScopes] = useState(shown.scopes);
  const [tags, setTags] = useState(shown.tags);
  const [notes, setNotes] = useState(shown.notes);
  const [expires, setExpires] = useState(shown.expires);
  const [rotation, setRotation] = useState(shown.rotation);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // An edit names and judges only the fields whose text the user changed (FW-03, D2B-02). Native
  // code keeps every other field exactly as it was stored, so a stored value that the form's own
  // rules would refuse never blocks an edit of another field.
  const changed = (now: string, was: string) => !editing || now !== was;
  const rotationJudged = changed(rotation, shown.rotation);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);

    if (changed(title, shown.title) && !title.trim()) {
      setError("Give the key a title.");
      return;
    }

    if (!editing && !secret) {
      setError("Paste the secret value.");
      return;
    }

    let rotationDays: number | null = null;
    if (rotationJudged && rotation.trim()) {
      const parsed = Number(rotation);
      if (!Number.isInteger(parsed) || parsed <= 0 || parsed > MAX_ROTATION_DAYS) {
        setError(`Rotation must be a whole number of days between 1 and ${MAX_ROTATION_DAYS}.`);
        return;
      }
      rotationDays = parsed;
    }

    const draft: KeyDraft = {};
    if (changed(title, shown.title)) draft.title = title;
    if (secret) draft.secret = secret;
    if (changed(username, shown.username)) draft.username = username;
    if (changed(issuer, shown.issuer)) draft.issuer = issuer;
    if (changed(project, shown.project)) draft.project = project;
    if (changed(environment, shown.environment)) draft.environment = environment;
    if (changed(envVar, shown.envVar)) draft.envVar = envVar;
    if (changed(url, shown.url)) draft.url = url;
    if (changed(scopes, shown.scopes)) draft.scopes = splitList(scopes, SCOPE_SEPARATOR);
    if (changed(tags, shown.tags)) draft.tags = splitList(tags, TAG_SEPARATOR);
    if (changed(notes, shown.notes)) draft.notes = notes;
    if (changed(expires, shown.expires)) draft.expiresAt = expires ? dateOnlyExpiry(expires) : null;
    if (rotationJudged) draft.rotationDays = rotationDays;

    setBusy(true);
    try {
      const saved = editing
        ? await api.update(item.id, draft)
        : await api.add(draft);
      onSaved(saved);
    } catch (cause) {
      setError(errorText(cause));
      onFailed?.();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="overlay overlay-drawer"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="drawer raised"
        role="dialog"
        aria-modal="true"
        aria-labelledby="form-title"
        aria-describedby={editing ? targetId : undefined}
      >
        <header className="modal-head">
          <div>
            <p className="label">{editing ? "Edit metadata" : "New entry"}</p>
            <h2 id="form-title">{editing ? "Edit key" : "Add a key"}</h2>
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label="Close"
            onClick={onClose}
            disabled={busy}
          >
            <Icon name="close" size={20} />
          </button>
        </header>

        <form onSubmit={submit}>
          <div className="drawer-body">
            {editing && <EditTarget item={item} id={targetId} />}
            <section className="form-group" aria-labelledby="form-group-key">
              <h3 id="form-group-key">The key</h3>
              <div className="form-grid">
                <div className="field">
                  <label htmlFor="key-title">Title</label>
                  <input
                    id="key-title"
                    value={title}
                    onChange={(event) => setTitle(event.target.value)}
                    placeholder="OpenAI — staging"
                    autoComplete="off"
                    autoFocus
                    required={changed(title, shown.title)}
                  />
                </div>

                <div className="field">
                  <label htmlFor="key-issuer">Issuer</label>
                  <input
                    id="key-issuer"
                    value={issuer}
                    onChange={(event) => setIssuer(event.target.value)}
                    placeholder="OpenAI"
                    autoComplete="off"
                  />
                </div>

                <div className="field full">
                  <label htmlFor="key-username">Username</label>
                  <input
                    id="key-username"
                    className="mono"
                    value={username}
                    onChange={(event) => setUsername(event.target.value)}
                    placeholder="Optional, for example an access key ID"
                    autoComplete="off"
                    spellCheck={false}
                  />
                </div>

                <div className="field full">
                  <label htmlFor="key-secret">
                    Secret {editing && <span className="label">(optional)</span>}
                  </label>
                  <input
                    id="key-secret"
                    type="password"
                    value={secret}
                    onChange={(event) => setSecret(event.target.value)}
                    placeholder={editing ? "Leave blank to keep it" : "sk-…"}
                    autoComplete="off"
                    spellCheck={false}
                    aria-describedby="key-secret-note"
                  />
                  <p className="field-note" id="key-secret-note">
                    {editing
                      ? "Leave it blank to keep the stored value. A new value counts as a rotation, and the old value stays in the vault history. To rotate a key, use Rotate secret in its detail view."
                      : "Encrypted inside the KDBX entry."}
                  </p>
                </div>
              </div>
            </section>

            <section className="form-group" aria-labelledby="form-group-where">
              <h3 id="form-group-where">Where it is used</h3>
              <div className="form-grid">
                <div className="field">
                  <label htmlFor="key-project">Project</label>
                  <input
                    id="key-project"
                    value={project}
                    onChange={(event) => setProject(event.target.value)}
                    placeholder="my-app"
                    autoComplete="off"
                  />
                </div>

                <div className="field">
                  <label htmlFor="key-environment">Environment</label>
                  <input
                    id="key-environment"
                    value={environment}
                    onChange={(event) => setEnvironment(event.target.value)}
                    placeholder="dev"
                    autoComplete="off"
                  />
                </div>

                <div className="field full">
                  <label htmlFor="key-env-var">Environment variable</label>
                  <input
                    id="key-env-var"
                    className="mono"
                    value={envVar}
                    onChange={(event) => setEnvVar(event.target.value)}
                    placeholder="Derived from the title"
                    autoComplete="off"
                    spellCheck={false}
                    aria-describedby="key-env-var-note"
                  />
                  <p className="field-note" id="key-env-var-note">The name this key gets in a .env file.</p>
                </div>
              </div>
            </section>

            <section className="form-group" aria-labelledby="form-group-life">
              <h3 id="form-group-life">When it wears out</h3>
              <div className="form-grid">
                <div className="field">
                  <label htmlFor="key-expires">Expires</label>
                  <input
                    id="key-expires"
                    type="date"
                    value={expires}
                    onChange={(event) => setExpires(event.target.value)}
                    aria-describedby="key-expires-note"
                  />
                  <p className="field-note" id="key-expires-note">Optional.</p>
                </div>

                <div className="field">
                  <label htmlFor="key-rotation">Rotate every</label>
                  <input
                    id="key-rotation"
                    type="number"
                    inputMode="numeric"
                    min={rotationJudged ? 1 : undefined}
                    max={rotationJudged ? MAX_ROTATION_DAYS : undefined}
                    step={1}
                    value={rotation}
                    onChange={(event) => setRotation(event.target.value)}
                    placeholder="90"
                    aria-describedby="key-rotation-note"
                  />
                  <p className="field-note" id="key-rotation-note">Days. Leave blank for no reminder.</p>
                </div>
              </div>
            </section>

            <section className="form-group" aria-labelledby="form-group-details">
              <h3 id="form-group-details">Details</h3>
              <div className="form-grid">
                <div className="field full">
                  <label htmlFor="key-url">URL</label>
                  <input
                    id="key-url"
                    value={url}
                    onChange={(event) => setUrl(event.target.value)}
                    placeholder="https://platform.openai.com/api-keys"
                    autoComplete="off"
                  />
                </div>

                <div className="field">
                  <label htmlFor="key-scopes">Scopes</label>
                  <textarea
                    id="key-scopes"
                    className="mono"
                    value={scopes}
                    onChange={(event) => setScopes(event.target.value)}
                    rows={3}
                    placeholder={"chat\nmodels"}
                    autoComplete="off"
                    spellCheck={false}
                    aria-describedby="key-scopes-note"
                  />
                  <p className="field-note" id="key-scopes-note">One scope on each line. A comma is part of the scope.</p>
                </div>

                <div className="field">
                  <label htmlFor="key-tags">Tags</label>
                  <input
                    id="key-tags"
                    value={tags}
                    onChange={(event) => setTags(event.target.value)}
                    placeholder="production, billing"
                    autoComplete="off"
                    aria-describedby="key-tags-note"
                  />
                  <p className="field-note" id="key-tags-note">Comma separated.</p>
                </div>

                <div className="field full">
                  <label htmlFor="key-notes">Notes</label>
                  <textarea
                    id="key-notes"
                    value={notes}
                    onChange={(event) => setNotes(event.target.value)}
                    rows={3}
                    placeholder="What this key can do, and what to do before rotating it."
                  />
                </div>
              </div>
            </section>

            {error && <Notice tone="error">{error}</Notice>}
          </div>

          <div className="drawer-foot end">
            <button
              type="button"
              className="text-button"
              onClick={onClose}
              disabled={busy}
            >
              Cancel
            </button>
            <button type="submit" className="btn" disabled={busy} aria-busy={busy || undefined}>
              {busy ? "Saving…" : editing ? "Save changes" : "Add key"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
