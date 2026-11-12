import type { DrawnFact } from "../rowFacts";
import { keyIdentity } from "../rowFacts";
import type { KeyView } from "../types";

const MONO = new Set(["Variable", "ID"]);

/** A list of drawn facts, as a confirmation or a drawer shows them. */
export function FactList({ facts }: { facts: DrawnFact[] }) {
  return (
    <div className="fact-list">
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
 * The facts that tell one key from another, as its row draws them, and its ID when the row
 * shows one. A dialog that names a key shows these, so that two look-alike rows stay apart.
 */
export function keyIdentityFacts(item: KeyView, id: string | undefined): DrawnFact[] {
  const { project, environment, variable, issuer, scopes } = keyIdentity(item);
  return [project, environment, variable, issuer, ...(scopes.set ? [scopes] : []), ...(id === undefined ? [] : [{ label: "ID", text: id, set: true }])];
}
