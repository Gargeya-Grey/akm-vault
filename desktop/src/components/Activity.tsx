import { useState } from "react";
import { api, errorText } from "../api";
import { nativeReads, useNativeRead } from "../nativeRead";
import { formatDateTime } from "../format";
import type { AuditReport, JournalRecovery } from "../types";
import { EmptyPage } from "./glyphs";
import { Notice } from "./Notice";
import { ReadFailure } from "./ReadFailure";

const ACTIONS: Record<string, string> = {
  "add-completed": "Added",
  "update-completed": "Updated",
  "remove-completed": "Moved to Recycle Bin",
  "rotate-completed": "Rotated",
  "restore-completed": "Restored",
  "purge-completed": "Deleted for good",
  "export-completed": "Exported",
  "import-completed": "Imported",
  "upgrade-completed": "Format upgraded",
  "reveal-requested": "Reveal requested",
  "copy-requested": "Copy requested",
  "export-requested": "Export requested",
  "run-requested": "Launch requested",
  "journal-recovered": "New journal started after damage",
};

const STATES: Record<AuditReport["state"], string> = {
  absent: "Journal unavailable",
  empty: "No records available",
  consistent: "Recorded chain is consistent",
  corrupt: "Journal data is damaged",
};

const DAMAGE: Record<NonNullable<AuditReport["damage"]>, string> = {
  truncatedTail: "the last line has no line end",
  malformedJson: "a line is not a valid record",
  brokenLink: "a record does not follow the one before it",
  hashMismatch: "a record does not match its hash",
};

function keyLabel(keyTitle: string | null, keyId: string | null): string {
  if (keyTitle !== null) return keyTitle;
  return keyId === null ? "Vault" : `Key ${keyId.slice(0, 8)}`;
}

export function Activity() {
  const read = useNativeRead(nativeReads.journal);
  // A failed read drops the snapshot, so no state badge outlives the read that gave it.
  const report: AuditReport | null = read.state.status === "ready" ? read.state.data : null;
  const [recovering, setRecovering] = useState(false);
  const [recovery, setRecovery] = useState<{ kind: "done"; result: JournalRecovery } | { kind: "declined" } | { kind: "failed"; error: string } | null>(null);

  // Native asks first, in its own window. The page only asks; it cannot approve for the user.
  // A decline and a failure are separate outcomes: a decline changed nothing, and a failure
  // reports what native said.
  async function recover() {
    setRecovering(true);
    setRecovery(null);
    try {
      const result = await api.recoverJournal();
      setRecovery(result === null ? { kind: "declined" } : { kind: "done", result });
    } catch (cause) {
      setRecovery({ kind: "failed", error: errorText(cause) });
    } finally {
      setRecovering(false);
      read.reload();
    }
  }

  const entries = report ? [...report.entries].reverse() : [];

  return (
    <main className="view">
      <header className="view-head">
        <div>
          <h1>
            Activity, <em>after the fact.</em>
          </h1>
          <p className="lead">
            Activity records are kept on this device, in a private folder that is not beside the vault.
            They hold key ids and counts, not key names or values. Requests do not prove completion.
            Records are not authenticated and cannot prove a complete history.
          </p>
        </div>
        <button
          type="button"
          className="btn-secondary"
          onClick={read.reload}
          disabled={read.busy}
          aria-busy={read.busy || undefined}
        >
          {read.busy ? "Reading…" : "Refresh"}
        </button>
      </header>

      <section aria-label="Activity journal">
        <div className="audit-tools">
          <div>
            <p className="label">Journal file</p>
            <p className="mono path-text">{report?.path ?? (read.state.status === "failed" ? "Not read" : "Reading…")}</p>
          </div>
          {report && (
            <div className="audit-status">
              <p className={`state ${report.state === "corrupt" ? "state-warn" : ""}`}>{STATES[report.state]}</p>
              <p className="label">Completeness unknown</p>
            </div>
          )}
        </div>

        {read.state.status === "failed" && (
          <ReadFailure
            heading="AKM could not read the journal."
            detail="This does not mean that nothing was recorded. Select Refresh to try again."
            error={read.state.error}
            busy={read.busy}
          />
        )}

        {report?.notices.map((notice) => (
          <div className="ledger-message" key={notice.kind}>
            <Notice tone="info">{notice.message}</Notice>
          </div>
        ))}

        {report?.state === "corrupt" && (
          <div className="ledger-message">
            <Notice tone="error">
              Damage was found{report.damage === null ? "" : ` (${DAMAGE[report.damage]})`} at physical line <span className="mono">{report.problemLine}</span>{report.brokenAt === null ? "." : <>, declared sequence <span className="mono">{report.brokenAt}</span> (not verified).</>}
              Original bytes were preserved. The displayed prefix is not authenticated; missing records and the cause of damage are unknown.
            </Notice>
          </div>
        )}

        {report?.state === "corrupt" && (
          <div className="ledger-message">
            <Notice tone="info">
              Key reveal, copy and export stay blocked while the journal is damaged, so that no disclosure goes unrecorded.
              You can start a new journal. A window will ask you first. The damaged file is kept unchanged beside it, and the new journal records the gap.
              History before the damage cannot be proved complete.
            </Notice>
            <button type="button" className="btn-secondary" onClick={() => void recover()} disabled={recovering || read.busy} aria-busy={recovering || undefined}>
              {recovering ? "Waiting for your answer…" : "Start a new journal…"}
            </button>
          </div>
        )}

        {recovery?.kind === "done" && (
          <div className="ledger-message">
            <Notice tone="ok">
              A new journal started. The damaged file was kept as <span className="mono path-text">{recovery.result.keptName}</span> ({recovery.result.bytes} bytes, SHA-256 <span className="mono path-text">{recovery.result.sha256}</span>). History before the damage cannot be proved complete.
            </Notice>
          </div>
        )}

        {recovery?.kind === "declined" && (
          <div className="ledger-message">
            <Notice tone="info">Cancelled. The journal is unchanged and still blocks disclosure.</Notice>
          </div>
        )}

        {recovery?.kind === "failed" && (
          <div className="ledger-message">
            <Notice tone="error">{recovery.error}</Notice>
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

        {read.state.status === "loading" ? (
          <div className="empty-state">
            <h2>Reading the journal…</h2>
            <p>Entries appear newest first.</p>
          </div>
        ) : read.state.status === "failed" ? null : entries.length === 0 ? (
          <div className="empty-state">
            <EmptyPage />
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
                <span className="audit-action" data-kind={entry.action === "reveal-requested" || entry.action === "copy-requested" || entry.action.startsWith("export") ? "disclosure" : undefined}>
                  {Object.hasOwn(ACTIONS, entry.action) ? ACTIONS[entry.action] : `Legacy ${entry.action} (outcome unknown)`}
                </span>
                <span className="audit-key">
                  <span>{keyLabel(entry.keyTitle, entry.keyId)}</span>
                  <span className="key-sub">
                    {[entry.project, entry.environment]
                      .filter(Boolean)
                      .join(" · ") || "—"}
                  </span>
                </span>
                <span className="audit-detail" data-empty={entry.detail ? undefined : true}>{entry.detail ?? "—"}</span>
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
