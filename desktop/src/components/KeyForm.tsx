import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { dateOnlyExpiry } from "../schedule";
import type { KeyDraft, KeyView } from "../types";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";

interface Props {
  /** Present when editing; absent when adding. */
  item?: KeyView;
  onSaved: (item: KeyView) => void;
  onClose: () => void;
}

function splitList(value: string): string[] {
  const seen = new Set<string>();
  for (const part of value.split(",")) {
    const trimmed = part.trim();
    if (trimmed) seen.add(trimmed);
  }
  return [...seen];
}

export function KeyForm({ item, onSaved, onClose }: Props) {
  const editing = item !== undefined;
  const dialogRef = useDialogFocus<HTMLDivElement>();

  const [title, setTitle] = useState(item?.title ?? "");
  const [secret, setSecret] = useState("");
  const [issuer, setIssuer] = useState(item?.issuer ?? "");
  const [project, setProject] = useState(item?.project ?? "");
  const [environment, setEnvironment] = useState(item?.environment ?? "");
  const [envVar, setEnvVar] = useState(item?.envVar ?? "");
  const [url, setUrl] = useState(item?.url ?? "");
  const [scopes, setScopes] = useState(item?.scopes.join(", ") ?? "");
  const [tags, setTags] = useState(item?.tags.join(", ") ?? "");
  const [notes, setNotes] = useState(item?.notes ?? "");
  const [expires, setExpires] = useState(
    item?.expiresAt ? item.expiresAt.slice(0, 10) : "",
  );
  const [rotation, setRotation] = useState(
    item?.rotationDays !== null && item?.rotationDays !== undefined
      ? String(item.rotationDays)
      : "",
  );
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

    let rotationDays: number | null = null;
    if (rotation.trim()) {
      const parsed = Number(rotation);
      if (!Number.isInteger(parsed) || parsed <= 0 || parsed > 3650) {
        setError("Rotation must be a whole number of days between 1 and 3650.");
        return;
      }
      rotationDays = parsed;
    }

    const draft: KeyDraft = {
      title: title.trim(),
      secret: secret ? secret : null,
      username: item?.username ?? null,
      issuer: issuer.trim() || null,
      project: project.trim() || null,
      environment: environment.trim() || null,
      scopes: splitList(scopes),
      envVar: envVar.trim() || null,
      url: url.trim() || null,
      notes: notes.trim() || null,
      tags: splitList(tags),
      expiresAt: expires ? dateOnlyExpiry(expires) : null,
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
                    max={3650}
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
