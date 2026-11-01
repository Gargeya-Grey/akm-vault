# Unit 1a review, round 5 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: application source `main` at `cfeb1b0e7624f13118fcc16030407829bfdbbfd3`, through `git diff baseline/pre-unit-1a main`; docs from `test/r4-04-proof-checks` at `30fb44845a4800bb839d671689236fea2c94bc8a`
- Build hashes checked by the reviewer (all matched STATE.md): `akm-desktop.exe` bc393ed9…6ea8, `akm.exe` 495d7297…2011b, MSI e29139ea…b6f3ac, NSIS b0455b89…c2f7cb
- R4-04 input: deferred by the owner on 2026-11-01
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 6 new findings (4 P2, 2 P3), recorded as 1A-R5-01 to 1A-R5-06 in FINDINGS.md. This is the fifth rejection of unit 1a. Under AGENTS.md, patching stops, and the design question goes to the owner (STATE.md).

The text below is the reviewer's report, copied by hand. Typographic quotation marks, apostrophes and dashes became plain ASCII, and the reviewer's links to local files became plain `file:line` text or file names. Other differences cannot be excluded.

---

**The source review rejects unit 1a.** The strongest findings concern lock-event delivery, clipboard cleanup, and import-review invalidation. The required real-window proof is also deferred.

**Ranked findings**

1. **P2 - A lock can be missed before the listener registers.** `desktop/src/App.tsx:159` starts the status request before the asynchronous lock subscription is established. If native code computes an unlocked status, locks the session, and emits its notification before registration finishes, `desktop/src/nativeSession.ts:102` can accept the delayed status without a tombstone. A mocked-native probe using the actual App and API, with a stub Keys view, rendered the unlocked region while the simulated native session was locked. It did not reproduce a real Windows event ordering or demonstrate a secret disclosure. A regression should delay listener registration, lock after status computation, and verify that the stale status is rejected; it should also cover subscription failure and root replacement.

2. **P2 - An unreadable clipboard is treated as a newer user copy.** `desktop/src-tauri/src/clipboard.rs:384` treats an unchanged sequence with unreadable text as a mismatch. `GetClipboardData` or `GlobalLock` failure makes `matches_text` return false, after which the lease is discarded as though ownership had ended. The episode handler (`desktop/src-tauri/src/session.rs:782`) sees no error and records no warning. Fake-backend probes against the actual lease implementation reproduced both an expired normal copy and a partial-publication failure leaving fake contents present, with no retained lease; the latter reported no cleanup failure. The Win32 failure path is supported by inspection, not a real clipboard test. A regression should distinguish an unreadable same-sequence clipboard from a confirmed newer copy, retain a safe retry and warning for uncertainty, and preserve a newer identical user copy.

3. **P2 - Cancelling or editing an import does not revoke its native review.** `desktop/src/components/Settings.tsx:54` clears its local preview and token but sends no native cancellation. `PendingImports` (`desktop/src-tauri/src/pending.rs:25`) can therefore retain the input and accept the old token. A fake probe of the actual pending-review component accepted a token after the simulated frontend cancellation; source inspection establishes the missing cancellation call. A delayed preview can also install its token after the dialog closes. An actual App/API/native regression should cancel or edit before confirmation, including while preview is delayed, and show that the old token cannot be confirmed or installed later. Already-dispatched confirmations must retain their outcome.

4. **P2 - Successful CLI file exports have no completion record.** `crates/akm-cli/src/main.rs:377` records `export-requested`, then publishes the file without recording the known result. Against the named release CLI, a fake scoped export exited successfully and created its output, while the journal contained `export-requested` and no `export-completed`. The journal wording remains cautious, but this misses the unit's request-and-known-outcome contract. A CLI regression should require a separate completion record after publication and verify that a post-publication logging failure produces a non-retryable warning.

5. **P3 - A stale import token consumes a newer review.** `desktop/src-tauri/src/pending.rs:46` removes the current pending review before checking the supplied token. If a delayed confirmation carrying old token A reaches native code after review B is installed, A is rejected but B is removed too. A fake probe of the actual component reproduced both rejections. A regression should deliver A after B is installed and show that A fails while B remains confirmable once.

6. **P3 - A declared sequence is dropped when another field is invalid.** `crates/akm-core/src/audit.rs:200` discards the whole record when typed deserialization fails. The release CLI reported only line 1 for valid JSON containing integer `seq: 7` and an invalid timestamp, although the declared sequence was available. The test should display an available, clearly untrusted declared sequence without echoing record contents, and avoid inventing one when it is absent or ambiguous.

**Source acceptance: NO.** The source defects above leave the ownership, cleanup, review-invalidation, and journal-outcome contracts incomplete. The per-finding acceptance below concerns the reviewed source seam; no finding is closed while the work unit has not passed review.

**Work unit complete: NO.** The owner deferred R4-04. No real-window proof has established delayed import and file-save outcomes across actual minimize, restore, navigation and dismissal, including protected-detail expiry, cancellation, error, exactly-once acknowledgement and no resurrection. The known observation gaps in the R4-04 proof mechanism remain unresolved.

**Disposition of the requested findings**

| Finding | Disposition | Evidence and limit |
|---|---|---|
| R4-01 | **Fixed (accepted)** | Two successive fake CLI writes with later-sorting backups and simultaneous `.bak` and journal failures kept the immediate original and five dated backups. The post-commit retention-loss path has a typed warning by inspection. |
| R4-02 | **Not fixed** | Native IDs address the reported old-A event, but the listener-registration gap permits the current lock to be missed. See finding 1. |
| R4-03 | **Not fixed** | Explicit cleanup errors reach the episode handler, but unreadable same-sequence contents take the false-success path. See finding 2. |
| P2-WARN-OWNERSHIP | **Fixed with limits** | Typed operation notices retain their originating ticket and are separate from application warnings. Actual Tauri webview reload was not independently tested. |
| P2-JOURNAL-CLAIMS | **Fixed with limits** | Snapshot and UI wording no longer claim authentication or completeness. CLI file publication lacks its known completion record, and a declared sequence can be lost. |
| 1A-R1-01 | **Fixed (accepted)** | Core warning kinds map separately to native notice kinds; the fake CLI run also produced distinct backup and journal warnings together. |
| 1A-R1-02 | **Fixed (accepted)** | An unterminated JSON-whitespace suffix caused the release CLI to refuse disclosure and preserve journal bytes. |
| 1A-R1-03 | **Fixed with limits** | Native ID snapshots and explicit dismissal support remount handoff by inspection. A real webview reload was not independently exercised. |
| 1A-R1-04 | **Fixed (accepted)** | An invalid UTF-8 journal suffix was refused without disclosing the fake secret or changing bytes; malformed parsing is structurally reported. |
| 1A-R1-05 | **Fixed with limits** | Source and regression assertions retain success and use "could not be confirmed" for injected post-write sync failure. A real disk-sync failure was not induced. |
| 1A-R1-06 | **Fixed (accepted)** | CLI run and export use `*-requested`; the fake export confirmed the latter. Finding 4 concerns its missing file-publication outcome. |
| 1A-R1-07 | **Not fixed** | A parseable declared sequence disappears when the timestamp is invalid. See finding 6. |
| 1A-R2-01 | **Fixed (accepted)** | The release CLI refused a terminated vertical-tab suffix and an unterminated whitespace suffix, preserving bytes and withholding the fake secret. |
| 1A-R2-02 | **Fixed with limits** | Secret-free completion records use IDs and survive React-root handoff by source and reported tests. Full webview reload and R4-04 remain outside that evidence. |
| 1A-R2-03 | **Fixed with limits** | Application warnings have native IDs, separate retention and ID-only dismissal. Real Tauri reload was not independently tested. |
| 1A-R2-04 | **Fixed (accepted)** | Activity uses `Object.hasOwn`, including for inherited names such as `hasOwnProperty`. |
| 1A-R2-05 | **Fixed (accepted)** | Delete confirmation and success no longer promise a refreshed `.bak` or unconditional recovery; retention uncertainty has a separate notice. |
| 1A-R2-06 | **Fixed (accepted)** | Native audit sequence fields serialize as decimal strings, including values beyond JavaScript's exact integer range. Finding 6 concerns a sequence discarded before serialization. |
| 1A-R3-01 | **Fixed with limits** | Dispatched import and save outcomes are owned outside Settings through React-root handoff. R4-04 remains deferred; those outcome records live in document memory. |
| 1A-R3-02 | **Not fixed** | Retry and recurrence IDs are handled, but an indeterminate clipboard read is classified as recovery. See finding 2. |
| 1A-R3-03 | **Fixed with limits** | Ordinary obsolete successes and errors pass through the frontend generation fence. The missed native event in finding 1 can leave that generation unchanged. |

**What this review did not cover**

The full check run in `docs/STATE.md` is reported evidence, not a rerun by this reviewer. I did not launch the GUI, access the real clipboard or app settings, change the Windows session, install a build, run a full build or suite, or use real secrets. Real Windows lock and suspend delivery, the real idle wait, real clipboard fault behavior, real Tauri reload, and real delivery timing for the new event-ordering case remain unverified here. The small Rust probes used fake backends; they do not establish that the corresponding Windows faults occurred.

The checked-out review branch was `test/r4-04-proof-checks` at `30fb44845a4800bb839d671689236fea2c94bc8a`. The reviewed application source was `main` at `cfeb1b0e7624f13118fcc16030407829bfdbbfd3`; the scoped application files on the review branch match it, excluding the separately reviewed proof-tool changes. All 139 baseline files matched the archived pre-unit manifest. There are no deletions in that snapshot comparison. The archive establishes the earlier hash of `desktop/icon.svg`; preimages for the other 17 added-looking paths, and deletions outside the inspected inventories, cannot be proved.

The independently checked SHA-256 hashes all matched:

- `akm-desktop.exe`: `bc393ed9a5081aac2d30e4999e1537bb806c047e9a22bd025e06d2b78d3e6ea8`
- `akm.exe`: `495d72976b552746ebe170f1ab1f25280272b54bff5365b251d3cf546342011b`
- MSI: `e29139eadb113da5d998a173ea05d7ddf10ae29f8d1bcb72798f3a7e61b6f3ac`
- NSIS: `b0455b8912350d6d1c98f271364257f47e7f6340b19e2ed6a80cbfb3c2c2f7cb`

Hash matches identify the artifacts; they do not independently prove their build provenance. This scoped review is not a professional audit.

The shared artifact observer could not be used within this review's write restriction: it requires a project-local artifact root and shared-registry writes, while the brief permits probe files only in a new `%TEMP%` folder. The fake-data probes and task-local handoff receipt (`handoff.json`, review temporary folder) are retained there.
