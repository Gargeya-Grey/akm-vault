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
  onSaved: (item: KeyView) => void;
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

function splitList(value: string): string[] {
  const seen = new Set<string>();
  for (const part of value.split(",")) {
    const trimmed = part.trim();
    if (trimmed) seen.add(trimmed);
  }
  return [...seen];
}

/** The longest rotation interval that the core accepts (`MAX_ROTATION_DAYS`). */
const MAX_ROTATION_DAYS = 36500;

/** The text that the form shows for each stored field. Some of it is a lossy view of the value. */
function shownText(item?: KeyView) {
  return {
    title: item?.title ?? "",
    username: item?.username ?? "",
    issuer: item?.issuer ?? "",
    project: item?.project ?? "",
    environment: item?.environment ?? "",
    envVar: item?.envVar ?? "",
    url: item?.url ?? "",
    scopes: item?.scopes.join(", ") ?? "",
    tags: item?.tags.join(", ") ?? "",
    notes: item?.notes ?? "",
    expires: item?.expiresAt ? item.expiresAt.slice(0, 10) : "",
    rotation: item?.rotationDays !== null && item?.rotationDays !== undefined ? String(item.rotationDays) : "",
  };
}

const text = (value: string) => value.trim() || null;

export function KeyForm({ item, onSaved, onClose }: Props) {
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

    if (!title.trim()) {
      setError("Give the key a title.");
      return;
    }

    if (!editing && !secret) {
      setError("Paste the secret value.");
      return;
    }

    // A field that the user did not touch goes back exactly as it was stored: the form's text
    // cannot hold a comma inside a list item, a time of day, or a space at an end.
    function field<T>(now: string, was: string, stored: T | undefined, read: (now: string) => T): T {
      return stored !== undefined && now === was ? stored : read(now);
    }

    let rotationDays: number | null = null;
    if (item !== undefined && rotation === shown.rotation) {
      rotationDays = item.rotationDays;
    } else if (rotation.trim()) {
      const parsed = Number(rotation);
      if (!Number.isInteger(parsed) || parsed <= 0 || parsed > MAX_ROTATION_DAYS) {
        setError(`Rotation must be a whole number of days between 1 and ${MAX_ROTATION_DAYS}.`);
        return;
      }
      rotationDays = parsed;
    }

    const draft: KeyDraft = {
      title: field(title, shown.title, item?.title, (now) => now.trim()),
      secret: secret ? secret : null,
      // An empty text clears a stored username: native code keeps the stored one when it gets none.
      username: field(username, shown.username, item?.username, (now) => now.trim()),
      issuer: field(issuer, shown.issuer, item?.issuer, text),
      project: field(project, shown.project, item?.project, text),
      environment: field(environment, shown.environment, item?.environment, text),
      scopes: field(scopes, shown.scopes, item?.scopes, splitList),
      envVar: field(envVar, shown.envVar, item?.envVar, text),
      url: field(url, shown.url, item?.url, text),
      notes: field(notes, shown.notes, item?.notes, text),
      tags: field(tags, shown.tags, item?.tags, splitList),
      expiresAt: field(expires, shown.expires, item?.expiresAt, (now) => (now ? dateOnlyExpiry(now) : null)),
      rotationDays,
    };

    setBusy(true);
    try {
      const saved = editing
        ? await api.update(item.id, draft)
        : await api.add(draft);
      onSaved(saved);
    } catch (cause) {
      setError(errorText(cause));
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
                    required
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
                  />
                  <p className="field-note">
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
                  />
                  <p className="field-note">The name this key gets in a .env file.</p>
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
                  />
                  <p className="field-note">Optional.</p>
                </div>

                <div className="field">
                  <label htmlFor="key-rotation">Rotate every</label>
                  <input
                    id="key-rotation"
                    type="number"
                    inputMode="numeric"
                    min={1}
                    max={MAX_ROTATION_DAYS}
                    step={1}
                    value={rotation}
                    onChange={(event) => setRotation(event.target.value)}
                    placeholder="90"
                  />
                  <p className="field-note">Days. Leave blank for no reminder.</p>
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
                  <input
                    id="key-scopes"
                    className="mono"
                    value={scopes}
                    onChange={(event) => setScopes(event.target.value)}
                    placeholder="chat, models"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <p className="field-note">Comma separated.</p>
                </div>

                <div className="field">
                  <label htmlFor="key-tags">Tags</label>
                  <input
                    id="key-tags"
                    value={tags}
                    onChange={(event) => setTags(event.target.value)}
                    placeholder="production, billing"
                    autoComplete="off"
                  />
                  <p className="field-note">Comma separated.</p>
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
