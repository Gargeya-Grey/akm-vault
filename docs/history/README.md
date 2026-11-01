# Historical evidence archive

These are byte-identical copies of selected surviving engineering records, not a new test run, current defect list or audit. [manifest.json](manifest.json) records the source path, byte length and SHA-256 of every copied file.

## Read order

Use [current state](../STATE.md) and the [review ledger](../REVIEW_LEDGER.md) first. Consult raw reports to inspect a claim or reproduce its scenario. Later approvals supersede the stated scope of earlier failures, not their historical evidence.

- [Original scored baseline](reviews/00-baseline.md) is a snapshot of root `REVIEW.md`. It contains the consolidated original three-perspective assessment. Separate original perspective reports were not available in the surviving records.
- `reviews/first-correction/` contains surviving first-correction probes/results. Its standalone verdict was not found. The six findings remain recorded in root `PHASE1.md`; no missing verbatim report was invented.
- [Second correction review](reviews/correction/review-result.md), its [runtime addendum](reviews/correction/review-addendum.md) and [follow-up](reviews/correction/followup-review-result.md) preserve the failed candidates and changing evidence.
- [Acceptance review](reviews/acceptance/review-result.md) and [merged bounded approval](reviews/acceptance/followup-review-result.md) preserve the final source-correction decision. Their GUI gap was later closed by main-session evidence, not by silently editing their text.
- `phase1-final/` contains the final collected evidence, pinned sources/diff, original source archive, final phase-one narrative and task graph data. The old graph's readiness gate remains blocked; phase-one source approval is not overall trust approval.
- [The later GUI supplement](phase1-supplement/manifest.json) preserves recovered raw partial-run/build output with its own manifest. It does not alter the original 44-record archive. The earlier GUI fingerprint survives only as secondary conversation evidence, not an embedded raw-run hash.
- The fresh [2026-10-31 calibration](../reviews/2026-10-31-calibration/disposition.md) has a separate raw verdict, input snapshots and provenance. Its new findings are open; recording them is not a code fix.

## Limits and handling

The archive does not contain live AppData settings, settings preimages, vaults, plaintext exports, compiled binaries, dependencies or chat transcripts. The historical source ZIP is retained as an inert source snapshot after a filename privacy/path check; do not extract or execute it over the working tree.

Archived probes may reference old absolute paths, removed APIs and their original scratch configurations. They are evidence of the old scenario, not turnkey substitutes for the permanent current regressions. Never launch a historical GUI/clipboard probe just because a report contains its command.

Do not overwrite an archived record to make it agree with a later verdict. Add new dated evidence with provenance and update the ledger. Hashes establish identity only. This project-local archive survives chat compaction and temporary-file cleanup, but it is not an off-device backup or a saved git repository.
