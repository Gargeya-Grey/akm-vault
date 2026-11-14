import { Fragment, useEffect, useId, useRef, useState } from "react";
import { api, errorText } from "../api";
import { clipboardCopyText } from "../clipboardText";
import { formatDate, formatDateTime, isExpired, plural, rotationLabel, secretChangeLabel } from "../format";
import { nativeReads, useNativeRead } from "../nativeRead";
import { Plaintext, plaintextScope, useSealed } from "../plaintext";
import { keyDetails, useKeyRowFacts } from "../rowFacts";
import type { HistoryVersion, KeyView } from "../types";
import { KeyTarget } from "./FactList";
import { KeyCut } from "./glyphs";
import { Icon } from "./Icon";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";
import { PROTECTED_VIEW_EXPIRY_EVENT } from "../nativeSession";
import { useNow } from "../useNow";

interface Props {
  item: KeyView;
  /**
   * True while the list read that gave `item` holds. A read that fails or starts again makes it
   * false, and a revealed secret then goes (U2-R1-04).
   */
  current: boolean;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRotate: () => void;
  readOnly?: boolean;
}

const HIDE_SECONDS = 30;

// The names a vault entry gives its fields, in words. The marker of the secret's change time says
// nothing a reader needs: the secret already counts as changed.
const FIELD_WORDS: Record<string, string> = {
  Title: "title", UserName: "username", URL: "URL", Notes: "notes", Tags: "tags", Expiry: "expiry",
  AKM_ISSUER: "issuer", AKM_PROJECT: "project", AKM_ENV: "environment", AKM_SCOPES: "scopes",
  AKM_ENV_VAR: "variable name", AKM_ROTATION_DAYS: "rotation interval", Other: "another field",
};
const SECRET_FIELDS = new Set(["Password", "AKM_SECRET_CHANGED"]);

function changedText(version: HistoryVersion): string {
  const others = version.changed.filter((name) => !SECRET_FIELDS.has(name) && name !== "AKM_ID").map((name) => FIELD_WORDS[name] ?? "another field");
  const all = version.secretChanged ? ["secret", ...others] : others;
  return all.length > 0 ? `Changed: ${all.join(", ")}` : "No field changed";
}

/** The earlier versions of one key. It reads when it mounts, so closing the list drops the read. */
function EarlierVersions({ id }: { id: string }) {
  const read = useNativeRead(nativeReads.history(id), [id]);
  if (read.state.status === "loading") return <p className="history-note" role="status">Reading the earlier versions…</p>;
  if (read.state.status === "failed") {
    return (
      <div className="history-note">
        <Notice tone="error">{read.state.error}</Notice>
        <button type="button" className="text-button" onClick={read.reload} disabled={read.busy} aria-busy={read.busy || undefined}>
          {read.busy ? "Trying again…" : "Try again"}
        </button>
      </div>
    );
  }
  if (read.state.data.length === 0) return <p className="history-note">AKM read the vault history and found no versions of this key.</p>;
  return (
    <ol className="history-list" aria-label="Earlier versions">
      {read.state.data.map((version) => (
        <li key={version.index}>
          {version.modifiedAt ? <span className="mono">{formatDateTime(version.modifiedAt)}</span> : <span>Time unknown</span>}
          <span className="history-change">{changedText(version)}</span>
        </li>
      ))}
    </ol>
  );
}

export function KeyDetail({ item, current, onClose, onEdit, onDelete, onRotate, readOnly = false }: Props) {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const targetId = useId();
  // The secret is plaintext: it shows only for this key, while the read and the vault data hold.
  const secretScope = current ? plaintextScope.reveal(item.id) : null;
  const secret = useSealed(secretScope);
  const holdSecret = secret.hold;
  const [seconds, setSeconds] = useState(HIDE_SECONDS);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  // The key whose earlier versions are open. Bound to the key, so another key never shows them.
  const [historyFor, setHistoryFor] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const presentation = useRef(0);

  useEffect(() => {
    const expire = () => {
      presentation.current += 1;
      holdSecret(null); setBusy(false); setError(null); setCopied(null);
      setHistoryFor(null);
    };
    expire();
    const hidden = () => { if (document.hidden) expire(); };
    window.addEventListener(PROTECTED_VIEW_EXPIRY_EVENT, expire);
    document.addEventListener("visibilitychange", hidden);
    return () => {
      presentation.current += 1;
      window.removeEventListener(PROTECTED_VIEW_EXPIRY_EVENT, expire);
      document.removeEventListener("visibilitychange", hidden);
    };
  }, [item.id, holdSecret]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    if (secret.value === null) return;
    setSeconds(HIDE_SECONDS);
    // A monotonic clock: a wall clock that moves back must not keep the secret on screen.
    const started = performance.now();
    timer.current = window.setInterval(() => {
      const left = HIDE_SECONDS - Math.floor((performance.now() - started) / 1000);
      if (left <= 0) {
        holdSecret(null);
        if (timer.current !== null) window.clearInterval(timer.current);
        timer.current = null;
      } else {
        setSeconds(left);
      }
    }, 1000);
    return () => {
      if (timer.current !== null) window.clearInterval(timer.current);
      timer.current = null;
    };
  }, [secret.value, holdSecret]);

  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(null), 6000);
    return () => window.clearTimeout(id);
  }, [copied]);

  async function reveal() {
    const view = presentation.current;
    setBusy(true);
    setError(null);
    try {
      const value = await api.reveal(item.id);
      if (view === presentation.current) {
        // Reset the countdown in the same render as the secret, so the
        // "Hides in N seconds" text never starts from an earlier reveal's value.
        setSeconds(HIDE_SECONDS);
        holdSecret(value);
      }
    } catch (cause) {
      if (view === presentation.current) setError(errorText(cause));
    } finally {
      if (view === presentation.current) setBusy(false);
    }
  }

  function hide() {
    holdSecret(null);
  }

  async function copy() {
    const view = presentation.current;
    setError(null);
    try {
      const clear = await api.copy(item.id);
      if (view === presentation.current) setCopied(clipboardCopyText(clear));
    } catch (cause) {
      if (view === presentation.current) setError(errorText(cause));
    }
  }

  const now = useNow([item]);
  // The row's facts, ID included, come first and describe the drawer (U2-R1-02). The details
  // below them are values that only the drawer shows.
  const rowFacts = useKeyRowFacts(item);
  const details = keyDetails(item);
  const facts: { key: string; value: React.ReactNode; below?: React.ReactNode }[] = [];

  if (details.username) facts.push({ key: "Username", value: details.username });
  if (details.scopes.length > 0) {
    facts.push({
      key: "Scope names",
      value: (
        <span className="chips">
          {details.scopes.map((scope) => (
            <span className="chip mono" key={scope}>
              {scope}
            </span>
          ))}
        </span>
      ),
    });
  }
  if (details.tags.length > 0) {
    facts.push({
      key: "Tags",
      value: (
        <span className="chips">
          {details.tags.map((tag) => (
            <span className="chip" key={tag}>
              {tag}
            </span>
          ))}
        </span>
      ),
    });
  }
  if (details.url) facts.push({ key: "URL", value: details.url });
  facts.push({
    key: "Expires",
    value: item.expiresAt ? (
      <span>
        <span className="mono">{formatDate(item.expiresAt)}</span>
        {isExpired(item, now) && <span className="danger-text"> · expired</span>}
      </span>
    ) : (
      "Not set"
    ),
  });
  facts.push({
    key: "Rotation",
    value:
      item.rotationDays === null
        ? "Not set"
        : `every ${item.rotationDays} days · ${rotationLabel(item, now)}`,
  });
  facts.push({
    key: "Secret changed",
    value:
      item.secretChange === "known" ? (
        <span className="mono">{secretChangeLabel(item)}</span>
      ) : item.secretChange === "changed_outside" ? (
        <span>unknown · changed outside AKM</span>
      ) : (
        <span>unknown</span>
      ),
  });
  facts.push({
    key: "History",
    value:
      item.historyCount === 0
        ? "No earlier versions"
        : `${plural(item.historyCount, "earlier version")} in the vault history`,
    below: item.historyCount > 0 && (
      <div className="history-fact">
        {historyFor !== item.id ? (
          <button type="button" className="text-button history-toggle" onClick={() => setHistoryFor(item.id)}>
            Show earlier versions
          </button>
        ) : (
          <>
            <button type="button" className="text-button history-toggle" onClick={() => setHistoryFor(null)}>
              Hide earlier versions
            </button>
            <EarlierVersions id={item.id} />
          </>
        )}
      </div>
    ),
  });
  if (item.createdAt)
    facts.push({
      key: "Added",
      value: <span className="mono">{formatDateTime(item.createdAt)}</span>,
    });
  if (item.updatedAt)
    facts.push({
      key: "Updated",
      value: <span className="mono">{formatDateTime(item.updatedAt)}</span>,
    });

  return (
    <div
      className="overlay overlay-drawer"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="drawer raised"
        role="dialog"
        aria-modal="true"
        aria-labelledby="detail-title"
        aria-describedby={targetId}
      >
        <header className="modal-head">
          <div className="drawer-ident">
            <span className="key-tile"><KeyCut id={item.id} /></span>
            <div>
              <p className="label"><bdi>{rowFacts.issuer.set ? rowFacts.issuer.text : "API key"}</bdi></p>
              <h2 id="detail-title"><bdi>{rowFacts.title}</bdi></h2>
              <p className={`state state-${rowFacts.tone}`}>
                <span className="state-dot" aria-hidden="true" />
                {rowFacts.state}
              </p>
            </div>
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label="Close"
            onClick={onClose}
          >
            <Icon name="close" size={20} />
          </button>
        </header>

        <div className="drawer-body">
        <section className="secret-stage" aria-label="Secret value">
          <span className="label">Secret</span>
          <code className={`mono secret-value${secret.shown ? "" : " masked"}`}>
            <Plaintext value={secret.value} scope={secretScope} fallback="••••••••••••••••••••••••" />
          </code>
          <div className="secret-actions">
            {secret.shown ? (
              <button type="button" className="text-button" onClick={hide}>
                <Icon name="eyeOff" />
                Hide
              </button>
            ) : (
              <button
                type="button"
                className="text-button"
                onClick={reveal}
                disabled={busy}
                aria-busy={busy || undefined}
              >
                <Icon name="eye" />
                {busy ? "Revealing…" : "Reveal"}
              </button>
            )}
            <button type="button" className="btn-secondary" onClick={copy}>
              <Icon name={copied ? "check" : "copy"} />
              Copy
            </button>
          </div>
          {secret.shown && (
            <p className="secret-note">
              Hides in <span className="mono">{seconds}</span> seconds. Revealing
              is written to the activity log.
            </p>
          )}
        </section>
        {(copied || error) && (
          <div className="drawer-notes">
            {copied && <Notice tone="ok">{copied}</Notice>}
            {error && <Notice tone="error">{error}</Notice>}
          </div>
        )}

        <KeyTarget facts={rowFacts} id={targetId} />
        <div className="fact-list">
          {facts.map((fact) => (
            <div className="fact-row" key={fact.key}>
              <span className="fact-key">{fact.key}</span>
              <span className="fact-value">{fact.value}</span>
              {fact.below}
            </div>
          ))}
        </div>

        {details.notes && (
          <section className="detail-notes">
            <h3>Notes</h3>
            <p className="reading">
              {details.notes.map((line, index) => (
                <Fragment key={index}>
                  {index > 0 && <br />}
                  {line}
                </Fragment>
              ))}
            </p>
          </section>
        )}

        </div>

        <div className="drawer-foot">
          <button type="button" className="text-button danger-text" onClick={onDelete} disabled={readOnly}>
            <Icon name="trash" />
            Move to Recycle Bin
          </button>
          <div className="action-cluster">
            <button type="button" className="text-button" onClick={onRotate} disabled={readOnly}>
              <Icon name="refresh" />
              Rotate secret
            </button>
            <button type="button" className="text-button" onClick={onClose}>
              Close
            </button>
            <button type="button" className="btn-secondary" onClick={onEdit} disabled={readOnly}>
              <Icon name="pencil" />
              Edit
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
