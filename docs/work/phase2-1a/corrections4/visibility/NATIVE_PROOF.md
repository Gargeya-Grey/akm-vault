# Renewed exact-candidate proof and readiness boundary

Status: [visibility-1 candidate pinned](visibility-1-candidate.json), GUI `bc393ed9a5081aac2d30e4999e1537bb806c047e9a22bd025e06d2b78d3e6ea8`; source/lint/type/browser checks, unsigned bundle rebuild and exact-GUI non-visibility proofs complete. [Cleanup](runtime-cleanup.json) confirms settings/artifact/input restoration and no matching process observed. Both startup discoveries used retained owned-root refresh. The earlier owner-ready attempt and its failed import-dialog-expiry assertion remain preserved. That readiness is spent; this plan does not authorize a new minimize.

## Candidate before native work

- Finish source checks, then build the changed GUI and unsigned MSI/NSIS. Use immutable `visibility-1-candidate.json` with exact four artifact hashes, current compiled inputs and pinned build-record/log hashes. Preserve the original artifacts manifest and all failed logs. Retain CLI/KeePassXC evidence only with the [unchanged input/artifact record](retained-cli-evidence.json); no claim of a fresh reader run.
- Use `run_native.py --candidate visibility-1` so proof identities cannot overwrite historical `native-*` records. It verifies candidate inputs, build records/logs and all artifacts before settings access. Each mode creates its identity exclusively; no automatic proof retry.
- Renew `JournalOnly` (including immediate-original/deletion recovery and journal UI) and `JournalExportOnly` on that exact GUI. These modes skip clipboard entirely and do not minimize or change Windows lock/power/desktop state. A failure is retained and dispositioned, not silently rerun.

## Recovery for every launch

The existing harness refuses another AKM process, saves the original settings bytes and `recovery.json` before launch, records executable hash and process creation identity, and uses only new fixed-fake-data files in its owned `Temp\opencode\akm-app-proof-<uuid>` directory. Keep the known selected fake baseline and unknown pre-restart preference as recorded in STATE.md. Do not infer a new preference or delete old failed scratch directories.

Exit normally where possible. On failure retain logs/diagnostics and inspect identity before acting on a PID. Restore settings only while the proof still owns their current value; verify exact byte restoration, artifact identity and absence of owned AKM/WebView processes. Never close an unrelated instance or clear an existing clipboard.

## Renewed readiness, then one bounded outcome attempt

Ask the owner only after the candidate and non-visibility evidence are ready. Explain that they should be available to observe and briefly leave the fake AKM window alone. No actual minimize or `--owner-ready-for-visibility` flag until a new affirmative readiness reply.

The existing `OutcomeOnly` flow remains unchanged: delay actual operations using only the fake journal lock; verify import commit; minimize/restore only the verified owned native handle with actual `IsIconic`; require local import-dialog and busy-state expiry before releasing the lock; navigate away; retain exactly one path-free application outcome and verify exactly one scoped fake import. Then test a native-approved exact-path file save, native cancellation and a delayed pre-disclosure journal error, with exact output/refusal and explicit acknowledgment/no resurrection. It includes multiple owned-window minimize/restore steps within one script attempt, not OS lock/suspend.

Keep native journal/dialog ordering and all original timing predicates. Do not substitute synthetic DOM/native events, add production delay hooks or relax timeouts. [Publication authority](PUBLICATION_CONTRACT.md) remains ticket/revision plus unchanged scope/destination/ACL checks; local presentation expiry does not revoke a native-approved file save. Any failed predicate stops the attempt, preserves evidence and returns to the owner before retry or mechanism change.

After the runtime gate is dispositioned, freeze full-unit/cycle/source/artifact/evidence inputs and dispatch the one fresh owner-authorized Terra max acceptance after reconfirming availability. No numerical readiness rescore, real secrets, clipboard, installation, signing, git or publication is included.
