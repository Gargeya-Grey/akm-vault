import { Notice } from "./Notice";

interface Props {
  /** What could not be read, as a heading. */
  heading: string;
  /** What the failure does not mean. */
  detail: string;
  error: string;
  /** A read is in flight. The retry control is then disabled and says that it is busy. */
  busy: boolean;
  onRetry?: () => void;
}

/** The failed state of a read (fault class L). It never says that a list is empty. */
export function ReadFailure({ heading, detail, error, busy, onRetry }: Props) {
  return (
    <div className="empty-state read-failure">
      <h2>{heading}</h2>
      <p>{detail}</p>
      <Notice tone="error">{error}</Notice>
      {onRetry && (
        <div className="empty-actions">
          <button type="button" className="text-button" onClick={onRetry} disabled={busy} aria-busy={busy || undefined}>
            {busy ? "Trying again…" : "Try again"}
          </button>
        </div>
      )}
    </div>
  );
}
