import { useEffect, useId, useState } from "react";
import { api, errorText } from "../api";
import { useKeyRowFacts } from "../rowFacts";
import type { KeyView, RotationReceipt } from "../types";
import { KeyTarget } from "./FactList";
import { Notice } from "./Notice";
import { useDialogFocus } from "./useDialogFocus";

interface Props {
  item: KeyView;
  onRotated: (receipt: RotationReceipt) => void;
  onClose: () => void;
}

/**
 * Replaces the secret of one key. The dialog cannot be closed while the request runs, so a
 * cancel never hides a request that may still commit. The typed value is cleared as soon as
 * the request ends, whatever the outcome.
 */
export function RotateDialog({ item, onRotated, onClose }: Props) {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const titleId = useId();
  const fieldId = useId();
  const targetId = useId();
  const facts = useKeyRowFacts(item);
  const [secret, setSecret] = useState("");
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
    if (busy) return;
    if (!secret) {
      setError("Paste the new secret value.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const receipt = await api.rotate(item.id, secret);
      setSecret("");
      onRotated(receipt);
    } catch (cause) {
      setSecret("");
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="confirm raised"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={targetId}
      >
        <h2 id={titleId}>Rotate secret</h2>
        <form onSubmit={submit}>
          <div className="confirm-body">
            <p>
              Store the new secret for <strong><bdi>{facts.title}</bdi></strong>. The old secret stays in the
              vault history. This does not change the secret at the provider: create the new
              secret there first.
            </p>
            <KeyTarget facts={facts} id={targetId} />
          </div>
          <div className="field">
            <label htmlFor={fieldId}>New secret</label>
            <input
              id={fieldId}
              type="password"
              value={secret}
              onChange={(event) => setSecret(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              disabled={busy}
            />
          </div>
          {error && <Notice tone="error">{error}</Notice>}
          <div className="dialog-actions">
            <button type="button" className="text-button" onClick={onClose} disabled={busy}>
              Cancel
            </button>
            <button type="submit" className="btn" disabled={busy} aria-busy={busy || undefined}>
              {busy ? "Rotating…" : "Rotate secret"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
