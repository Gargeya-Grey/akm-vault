import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { formatDateTime } from "../format";
import type { AuditReport } from "../types";
import { Notice } from "./Notice";

const ACTIONS: Record<string, string> = {
  "add-completed": "Added",
  "update-completed": "Updated",
  "remove-completed": "Removed",
  "export-completed": "Exported",
  "import-completed": "Imported",
  "upgrade-completed": "Format upgraded",
  "reveal-requested": "Reveal requested",
  "copy-requested": "Copy requested",
  "export-requested": "Export requested",
  "run-requested": "Launch requested",
};

const STATES: Record<AuditReport["state"], string> = {
  absent: "Journal unavailable",
  empty: "No records available",
  consistent: "Recorded chain is consistent",
  corrupt: "Journal data is damaged",
};

export function Activity() {
  const [report, setReport] = useState<AuditReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.audit(500));
      setError(null);
    } catch (cause) {
      setReport(null);
      setError(errorText(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const entries = report ? [...report.entries].reverse() : [];

  return (
    <main className="wrap view">
      <header className="view-head">
        <div>
          <h1>
            Activity, <em>after the fact.</em>
          </h1>
          <p className="lead">
            Available activity records live beside the vault and may expose metadata to sync providers.
            Requests do not prove completion. Records are not authenticated and cannot prove a complete history.
          </p>
        </div>
        <button
          type="button"
          className="btn-secondary"
          onClick={() => void load()}
          disabled={loading}
        >
          {loading ? "Reading…" : "Refresh"}
        </button>
      </header>

      <section className="raised ledger" aria-label="Activity journal">
        <div className="ledger-tools audit-tools">
          <div>
            <p className="label">Journal file</p>
            <p className="mono path-text">{report?.path ?? "Reading…"}</p>
          </div>
          {report && <p className={`state ${report.state === "corrupt" ? "state-warn" : ""}`}>{STATES[report.state]}</p>}
        </div>

        {error && (
          <div className="ledger-message">
            <Notice tone="error">{error}</Notice>
          </div>
        )}

        {report && <div className="ledger-message"><p>Completeness unknown</p></div>}

        {report?.state === "corrupt" && (
          <div className="ledger-message">
            <Notice tone="error">
              Damage was found at physical line <span className="mono">{report.problemLine}</span>{report.brokenAt === null ? "." : <>, sequence <span className="mono">{report.brokenAt}</span>.</>}
              Original bytes were preserved. The displayed prefix is not authenticated; missing records and the cause of damage are unknown.
            </Notice>
          </div>
        )}

        {report && (
          <div className="audit-head" aria-hidden="true">
            <span>#</span>
            <span>Time</span>
            <span>Action</span>
            <span>Key</span>
            <span>Detail</span>
          </div>
        )}

        {loading && !report ? (
          <div className="empty-state">
            <h2>Reading the journal…</h2>
            <p>Entries appear newest first.</p>
          </div>
        ) : entries.length === 0 ? (
          <div className="empty-state">
            <h2>No records displayed.</h2>
            <p>This is not proof of inactivity. Records may be absent, unavailable or incomplete.</p>
          </div>
        ) : (
          <ol className="audit-rows">
            {entries.map((entry) => (
              <li className="audit-row" key={entry.seq}>
                <span className="audit-seq mono">
                  {String(entry.seq).padStart(3, "0")}
                </span>
                <span className="audit-time mono">{formatDateTime(entry.ts)}</span>
                <span className="audit-action">
                  {Object.hasOwn(ACTIONS, entry.action) ? ACTIONS[entry.action] : `Legacy ${entry.action} (outcome unknown)`}
                </span>
                <span className="audit-key">
                  <span>{entry.keyTitle ?? "Vault"}</span>
                  <span className="key-sub">
                    {[entry.project, entry.environment]
                      .filter(Boolean)
                      .join(" · ") || "—"}
                  </span>
                </span>
                <span className="audit-detail">{entry.detail ?? "—"}</span>
              </li>
            ))}
          </ol>
        )}

        {report && entries.length > 0 && (
          <div className="ledger-foot">
            <span className="label">
              Showing <span className="mono">{entries.length}</span> {entries.length === 1 ? "record" : "records"} from this snapshot.
            </span>
            <span className="label">
              Not authenticated. Truncation, missing records and rewritten history may be undetectable.
            </span>
          </div>
        )}
      </section>
    </main>
  );
}
