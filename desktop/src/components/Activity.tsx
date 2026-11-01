import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "../api";
import { formatDateTime, plural } from "../format";
import type { AuditReport } from "../types";
import { Notice } from "./Notice";

const ACTIONS: Record<string, string> = {
  add: "Added",
  update: "Updated",
  remove: "Removed",
  reveal: "Revealed",
  copy: "Copied",
  export: "Exported",
  import: "Imported",
  init: "Created",
  upgrade: "Format upgraded",
  "run-start": "Launch requested",
  "export-start": "Export requested",
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
            Every reveal, copy, write and export is appended to a hash-chained
            journal beside the vault. It records what happened, never the secret
            values themselves.
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
          {report &&
            (report.brokenAt === null ? (
              <p className="state state-ok">
                <span className="state-dot" aria-hidden="true" />
                Chain verified
              </p>
            ) : (
              <p className="state state-warn">
                <span className="state-dot" aria-hidden="true" />
                Chain broken
              </p>
            ))}
        </div>

        {error && (
          <div className="ledger-message">
            <Notice tone="error">{error}</Notice>
          </div>
        )}

        {report && report.brokenAt !== null && (
          <div className="ledger-message">
            <Notice tone="error">
              The chain first fails at entry{" "}
              <span className="mono">{report.brokenAt}</span>. Treat this journal
              as untrusted and compare it with a backup.
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
            <h2>Nothing has happened yet.</h2>
            <p>
              Add a key, reveal one, or export a .env slice. Each action lands
              here with a sequence number and a hash.
            </p>
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
                  {ACTIONS[entry.action] ?? entry.action}
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
              Showing {plural(entries.length, "entry")}
              {entries.length === 500 ? " (most recent)" : ""}
            </span>
            <span className="label">
              Append-only. Not tamper-proof against whoever owns the file.
            </span>
          </div>
        )}
      </section>
    </main>
  );
}
