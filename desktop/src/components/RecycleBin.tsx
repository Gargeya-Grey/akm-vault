import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { formatDateTime, plural } from "../format";
import type { RecycledKey } from "../types";
import { Icon } from "./Icon";
import { ConfirmDialog, Notice } from "./Notice";

/**
 * Keys that were removed from the key list. Nothing here is deleted until the user asks:
 * restore returns a key with its id, secret, history and group; "Delete for good" is the
 * only action that removes a key from the vault.
 */
export function RecycleBin({ readOnly = false }: { readOnly?: boolean }) {
  const [items, setItems] = useState<RecycledKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [purging, setPurging] = useState<RecycledKey | null>(null);
  const [purgeError, setPurgeError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setItems(await api.recycled());
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

  async function restore(item: RecycledKey) {
    setBusyId(item.key.id);
    setError(null);
    try {
      const key = await api.restore(item.key.id);
      setItems((current) => current.filter((entry) => entry.key.id !== item.key.id));
      setFlash(`Restored “${key.title}” to the key list.`);
    } catch (cause) {
      setError(errorText(cause));
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
      setItems((current) => current.filter((entry) => entry.key.id !== target.key.id));
      setFlash(`Deleted “${target.key.title}” for good.`);
      setPurging(null);
    } catch (cause) {
      setPurgeError(errorText(cause));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <main className="wrap view">
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

      <section className="raised ledger" aria-label="Recycled keys">
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

        {loading ? (
          <div className="empty-state">
            <h2>Opening the Recycle Bin…</h2>
            <p>The vault is already unlocked. Removed keys appear here in a moment.</p>
          </div>
        ) : items.length === 0 ? (
          <div className="empty-state">
            <h2>The Recycle Bin is empty.</h2>
            <p>A key you remove from the key list waits here until you restore it or delete it for good.</p>
          </div>
        ) : (
          <ul className="key-rows">
            {items.map((item) => (
              <li className="bin-row" key={item.key.id}>
                <span className="key-ident">
                  <span className="key-title">{item.key.title}</span>
                  <span className="key-sub">
                    {item.key.project ?? "No project"}
                    {item.key.environment ? ` · ${item.key.environment}` : ""}
                    {item.key.historyCount > 0 && ` · ${plural(item.key.historyCount, "earlier version")}`}
                  </span>
                </span>
                <span className="key-sub">
                  Removed <span className="mono">{item.deletedAt ? formatDateTime(item.deletedAt) : "at an unknown time"}</span>
                </span>
                <span className="action-cluster">
                  <button
                    type="button"
                    className="btn-secondary"
                    onClick={() => void restore(item)}
                    disabled={readOnly || busyId !== null}
                    aria-label={`Restore ${item.key.title}`}
                  >
                    <Icon name="refresh" />
                    Restore
                  </button>
                  <button
                    type="button"
                    className="text-button danger-text"
                    onClick={() => { setPurgeError(null); setPurging(item); }}
                    disabled={readOnly || busyId !== null}
                    aria-label={`Delete ${item.key.title} for good`}
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
          onCancel={() => setPurging(null)}
        >
          <p>
            <strong>{purging.key.title}</strong> will be removed from the vault, with its history.
            This cannot be undone from inside the app. Older copies of the vault and older encrypted
            backups can still hold the key. AKM records the deletion, but a KeePassXC merge with an
            older copy may bring the key back.
          </p>
          {purgeError && <Notice tone="error">{purgeError}</Notice>}
        </ConfirmDialog>
      )}
    </main>
  );
}
