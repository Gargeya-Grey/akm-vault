import { summaryHeading, summaryLine } from "../reminders";
import type { Attention } from "../reminders";
import type { KeyView } from "../types";

interface Props {
  attention: Attention;
  /** The name of a key in a line: its drawn title and its short ID, as the Keys screen gives them. */
  nameOf: (key: KeyView) => string;
  onDismiss: () => void;
}

/**
 * The unlock summary. It holds counts and key names (title and short ID) only. It is not a dialog and takes no focus;
 * dismissing it moves focus to the view's fallback target.
 */
export function AttentionSummary({ attention, nameOf, onDismiss }: Props) {
  const lines: [string, typeof attention.expired][] = [
    ["Expired", attention.expired],
    ["Rotation overdue", attention.overdue],
    ["Due soon", attention.dueSoon],
    ["Rotation unknown", attention.unknown],
  ];
  return (
    <section className="raised attention-summary" aria-label="Keys needing attention">
      <div className="attention-head">
        <h2>{summaryHeading(attention.total)}</h2>
        <button
          type="button"
          className="text-button"
          onClick={() => {
            document.querySelector<HTMLElement>("[data-focus-fallback]")?.focus();
            onDismiss();
          }}
        >
          Dismiss summary
        </button>
      </div>
      <ul>
        {lines.filter(([, keys]) => keys.length > 0).map(([label, keys]) => (
          <li key={label}>{summaryLine(label, keys, nameOf)}</li>
        ))}
      </ul>
    </section>
  );
}
