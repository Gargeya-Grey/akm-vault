import { keyFactList, type DrawnFact, type KeyRowFacts } from "../rowFacts";

const MONO = new Set(["Variable", "ID"]);

/** A list of drawn facts, as a confirmation or a drawer shows them. `id` lets a dialog be described by it. */
export function FactList({ facts, id }: { facts: DrawnFact[]; id?: string }) {
  return (
    <div className="fact-list" id={id}>
      {facts.map((item) => (
        <div className="fact-row" key={item.label}>
          <span className="fact-key">{item.label}</span>
          <span className={`fact-value${MONO.has(item.label) ? " mono" : ""}`}>{item.text}</span>
        </div>
      ))}
    </div>
  );
}

/**
 * The key that a dialog acts on: every fact of its row, expiry, renewal and ID included, from the
 * row facts (U2-R1-02). The dialog names `id` in its aria-describedby, so a screen reader hears
 * the same facts.
 */
export function KeyTarget({ facts, id }: { facts: KeyRowFacts; id?: string }) {
  return <FactList facts={keyFactList(facts)} id={id} />;
}
