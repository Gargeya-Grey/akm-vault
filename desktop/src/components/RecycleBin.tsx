import { useId, useState } from "react";
import { api, errorText } from "../api";
import { nativeReads, useNativeRead } from "../nativeRead";
import { formatDateTime, plural } from "../format";
import type { RecycledKey } from "../types";
import { EmptyBin, KeyCut } from "./glyphs";
import { Icon } from "./Icon";
import { ConfirmDialog, Notice } from "./Notice";
import { ReadFailure } from "./ReadFailure";

/**
 * Keys that were removed from the key list. Nothing here is deleted until the user asks:
 * restore returns a key with its id, secret, history and group; "Delete for good" is the
 * only action that removes a key from the vault.
 */
export function RecycleBin({ readOnly = false }: { readOnly?: boolean }) {
  const read = useNativeRead(nativeReads.recycled);
  const items = read.state.status === "ready" ? read.state.data : null;
  const [actionError, setActionError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  // An identity, never a copy: the confirmation draws the entry as the newest read gives it.
  const [purgingId, setPurgingId] = useState<string | null>(null);
  const purging = items?.find((entry) => entry.key.id === purgingId) ?? null;
  const [purgeError, setPurgeError] = useState<string | null>(null);
  const factsId = useId();

  async function restore(item: RecycledKey) {
    setBusyId(item.key.id);
    setActionError(null);
    try {
      const key = await api.restore(item.key.id);
      read.apply((current) => current.filter((entry) => entry.key.id !== item.key.id));
      setFlash(`Restored “${key.title}” to the key list.`);
    } catch (cause) {
      setActionError(errorText(cause));
    } finally {
      setBusyId(null);
    }
  }

  async function purge() {
    if (!purging) return;
    const target = purging;
    setBusyId(target.key.id);
    setPurgeError(null);
    try {
      await api.purge(target.key.id);
      read.apply((current) => current.filter((entry) => entry.key.id !== target.key.id));
      setFlash(`Deleted “${target.key.title}” for good.`);
      setPurgingId(null);
    } catch (cause) {
      setPurgeError(errorText(cause));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <main className="view">
      <header className="view-head">
        <div>
          <h1 tabIndex={-1} data-focus-fallback="">
            Recycle Bin, <em>nothing lost yet.</em>
          </h1>
          <p className="lead">
            Removed keys wait here with their secret, history and group. Nothing is deleted for
            good unless you choose it, and AKM never empties this list by itself.
          </p>
        </div>
      </header>

      <section aria-label="Recycled keys">
        {actionError && (
          <div className="ledger-message">
            <Notice tone="error">{actionError}</Notice>
          </div>
        )}
        {flash && (
          <div className="ledger-message">
            <Notice tone="ok">{flash}</Notice>
          </div>
        )}

        {read.state.status === "loading" ? (
          <div className="empty-state">
            <h2>Opening the Recycle Bin…</h2>
            <p>The vault is already unlocked. Removed keys appear here in a moment.</p>
          </div>
        ) : read.state.status === "failed" ? (
          <ReadFailure
            heading="AKM could not read the Recycle Bin."
            detail="This does not mean that the Recycle Bin is empty."
            error={read.state.error}
            busy={read.busy}
            onRetry={read.reload}
          />
        ) : read.state.data.length === 0 ? (
          <div className="empty-state">
            <EmptyBin />
            <h2>The Recycle Bin is empty.</h2>
            <p>A key you remove from the key list waits here until you restore it or delete it for good.</p>
          </div>
        ) : (
          <ul className="key-rows">
            {read.state.data.map((item) => (
              <li className="bin-row" key={item.key.id}>
                <KeyCut id={item.key.id} />
                <span className="key-ident" id={`${factsId}-${item.key.id}`}>
                  <span className="key-title">{item.key.title}</span>
                  <span className="key-sub">{item.key.project ?? "No project"}</span>
                  <span className="key-sub">{item.key.environment ?? "No environment"}</span>
                  <span className="key-sub mono">{item.key.resolvedEnvVar}</span>
                  <span className="key-sub">{item.key.issuer ?? "No issuer"}</span>
                  <span className="key-sub mono">ID {item.key.id}</span>
                  <span className="key-sub">
                    Removed <span className="mono">{item.deletedAt ? formatDateTime(item.deletedAt) : "at an unknown time"}</span>
                    {item.key.historyCount > 0 && ` · ${plural(item.key.historyCount, "earlier version")}`}
                  </span>
                </span>
                <span className="action-cluster">
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => void restore(item)}
                    disabled={readOnly || busyId !== null}
                    aria-label={`Restore ${item.key.title}`}
                    aria-describedby={`${factsId}-${item.key.id}`}
                  >
                    <Icon name="refresh" />
                    Restore
                  </button>
                  <button
                    type="button"
                    className="text-button danger-text"
                    onClick={() => { setPurgeError(null); setPurgingId(item.key.id); }}
                    disabled={readOnly || busyId !== null}
                    aria-label={`Delete ${item.key.title} for good`}
                    aria-describedby={`${factsId}-${item.key.id}`}
                  >
                    <Icon name="trash" />
                    Delete for good
                  </button>
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {purging && (
        <ConfirmDialog
          title="Delete this key for good?"
          confirmLabel="Delete for good"
          busy={busyId !== null}
          onConfirm={() => void purge()}
          onCancel={() => setPurgingId(null)}
        >
          <p>
            <strong>{purging.key.title}</strong> will be removed from the vault, with its history.
            This cannot be undone from inside the app. Older copies of the vault and older encrypted
            backups can still hold the key. AKM records the deletion, but a KeePassXC merge with an
            older copy may bring the key back.
          </p>
          <div className="fact-list">
            <div className="fact-row"><span className="fact-key">Project</span><span className="fact-value">{purging.key.project ?? "No project"}</span></div>
            <div className="fact-row"><span className="fact-key">Environment</span><span className="fact-value">{purging.key.environment ?? "No environment"}</span></div>
            <div className="fact-row"><span className="fact-key">Variable</span><span className="fact-value mono">{purging.key.resolvedEnvVar}</span></div>
            <div className="fact-row"><span className="fact-key">Issuer</span><span className="fact-value">{purging.key.issuer ?? "No issuer"}</span></div>
            <div className="fact-row"><span className="fact-key">ID</span><span className="fact-value mono">{purging.key.id}</span></div>
            <div className="fact-row"><span className="fact-key">Removed</span><span className="fact-value">{purging.deletedAt ? formatDateTime(purging.deletedAt) : "At an unknown time"}</span></div>
          </div>
          {purgeError && <Notice tone="error">{purgeError}</Notice>}
        </ConfirmDialog>
      )}
    </main>
  );
}
