# Phase 2 unit 1a fresh implementation acceptance review

Date: 2026-10-31. This is a scoped, read-only implementation review, not a nine-area rescore or professional audit.

## Source-correction verdict

**Do not accept the current candidate yet.** The warning ownership and single-snapshot changes close the central ordinary-session cases, but the findings below leave supported correctness and privacy gaps in the frozen contracts. Some affected append behavior predates this unit; it is still a direct caller of the new snapshot and requested-event reporting and has not been corrected by this candidate.

## Ranked findings

### 1. P2. Backup failures are presented as journal failures

`crates/akm-core/src/vault.rs:533-541` puts failed `.bak` refresh and backup-pruning warnings in the same vector as journal warnings. `desktop/src-tauri/src/session.rs:391-406` converts every nonempty vector into a `journal` operation notice. `desktop/src/App.tsx:187-195` then states that the activity entry was not recorded.

Scenario: the `.bak` destination is an existing directory. A vault add commits, the convenience backup refresh fails, and journal append succeeds. The frontend will incorrectly say the activity entry was not recorded and will not tell the user the backup failed. In a fake-data release-CLI probe, the add exited 0 with the `.bak` warning, while `audit --verify` exited 0 and showed `add-completed`. The native conversion and frontend text are inspection-supported, not a GUI reproduction. Simultaneous backup and journal warnings are also collapsed into one notice.

Correction: preserve typed, origin-owned backup and journal classifications separately, with fixed secret-free text. Extending the frozen notice-kind contract requires an explicit integration decision. Add actual Session and App/API tests for backup-only, journal-only, and simultaneous failures, including a lock/new-open interleaving.

### 2. P2. An unterminated valid record allows disclosure while the append corrupts the journal

`crates/akm-core/src/audit.rs:177-192` accepts a valid final JSON record without a newline. `audit.rs:78-79,111-119,252-267` then reads it as the tail and appends the next object without first checking for a line separator. `crates/akm-cli/src/main.rs:307-316` treats that append as successful and discloses the secret.

The fake-data probe removed only the final newline from a valid one-record journal. `audit --verify` reported a consistent chain and exited 0. A requested reveal then exited 0 and printed the fake secret. The next audit exited 2, reported damage on line 1, and could display neither record because the two objects were concatenated. The same append helper is used by native pre-disclosure logging. This is a pre-existing append seam exposed by the current snapshot and fail-closed contract, not a claim that unit 1a introduced `tail()`.

Correction: refuse an unsafe append before disclosure, without modifying the existing bytes. Do not implement tail repair under this unit. Add CLI and native fake-data regressions for a valid unterminated last record and assert no disclosure, no append, and unchanged bytes.

### 3. P2. A drained warning can disappear during a non-crash root remount

`desktop/src-tauri/src/session.rs:524-529` destructively drains notices when `vault_warnings` is called. `desktop/src/api.ts:41-45` dispatches the reply as a window event. `desktop/src/App.tsx:55-64` retains it only in the mounted root listener and removes that listener on unmount.

A targeted React test using the actual App and API wrapper started a warning request, unmounted the root, delivered the delayed A reply, and remounted with B. The second native poll was empty and no operation notice appeared. This is a synthetic non-crash remount, not a claim that the normal lock/open path unmounts the root or that crash-durable notices were promised. The source has no acknowledgment or replay for this lifecycle gap.

Correction: retain ID-keyed native notices until explicit dismissal or an equivalent remount-safe handoff, rather than draining before a receiver owns them. Add the tested unmount/delayed-reply/remount case and an actual webview reload or other production lifecycle test if that transition is supported. Preserve the sanitized payload and B's active state.

### 4. P2. A failed disclosure can echo malformed journal contents

`crates/akm-core/src/audit.rs:263-264` embeds the serde parse error from the original line in the returned error. `desktop/src-tauri/src/error.rs:49` forwards invalid-input text, and `crates/akm-cli/src/main.rs:192-194,307-312` prints it for a failed reveal.

The fake-data probe replaced the journal with `{"seq":"FAKE_JOURNAL_MARKER"}`. Audit reporting correctly hid the marker, and reveal failed closed, but the CLI error echoed `FAKE_JOURNAL_MARKER` as the invalid string value. The bytes remained unchanged. A malformed line may contain sensitive data, so sanitizing only `snapshot()` is insufficient for the direct append caller.

Correction: return structural damage information without serde values or raw line content from append/tail errors. Add CLI and native requested-disclosure regressions that check neither errors nor notices contain a fake marker and that no secret is returned.

### 5. P2. The warning asserts an append was not recorded when its result can be uncertain

`crates/akm-core/src/audit.rs:115-118` can return an error after `write_all` has written the complete line, for example when `sync_data` fails. `desktop/src-tauri/src/commands.rs:473-486` and `session.rs:391-406` reduce any append error to a notice. `desktop/src/App.tsx:190-193` categorically says the entry was not recorded. A partial write can also leave uncertain or damaged bytes. This is an inspection-supported failure mode, not an injected filesystem reproduction.

Correction: say AKM could not confirm the journal entry and advise checking it, without suggesting the saved operation be repeated. Keep journal and backup warnings distinct. Add fault injection after a complete write but before successful sync for both vault mutation and native export publication.

### 6. P3. New CLI launch and export requests still use legacy action names

`crates/akm-cli/src/main.rs:358-362,377-384` continues to write `run-start` and `export-start` for new events, despite the frozen requirement that new pre-disclosure events use explicit `*-requested` names. The fake CLI export probe produced `export-start`; disclosure logging still occurred before export. The existing test at `crates/akm-cli/tests/security.rs:285` also asserts `run-start`.

Correction: emit `run-requested` and `export-requested`, retain honest unknown-completion presentation for old records, and test both new CLI actions and legacy records. Preserve the existing fail-closed ordering.

### 7. P3. Available offending sequence is dropped from both audit presentations

`crates/akm-core/src/audit.rs:193-211` records the offending sequence in `broken_at`, and `desktop/src-tauri/src/commands.rs:298-304` sends it to the frontend. `crates/akm-cli/src/main.rs:434-437` and `desktop/src/components/Activity.tsx:91-96` display only the physical line. The prior CLI verify output included the broken sequence. A fake corrupted-hash probe returned exit 2 and line 1 but omitted the known offending sequence 1.

Correction: display the sequence when available, alongside the physical line, without echoing damaged content. Test parsed broken-link/hash records and malformed JSON, for which the sequence may be unavailable.

## Runtime proof and limits

Independent checks in this review were the fake-data CLI probes in `probe.py` and `sequence-probe.py`, plus one targeted Vitest run of `root-lifecycle.test.tsx`. The Vitest command was `node 'W:\API Keys Manager\desktop\node_modules\vitest\vitest.mjs' run --config 'C:\Users\Gargeya\AppData\Local\Temp\opencode\akm-unit1a-acceptance-review\vitest.config.ts'`. It reported one passing test, which asserts the demonstrated warning loss. Probe commands and outputs are retained in this directory.

The caller reports 144 workspace tests, 34 frontend tests, eight browser scenarios, strict Clippy, TypeScript/Vite, and completed release builds. I did not rerun or independently certify those suites or builds. The focused native GUI journal proof was main-owned and in progress when assigned; its outcome is not asserted here. I did not run a GUI, real clipboard, settings access, OS lock/suspend, installation, network, or a full build. Exact-candidate native publication with late lock/journal failure, actual OS delivery, and independent-reader evidence remain separate proof questions. No project `verify-*` skill was found; the project verification guide and the caller's narrower policy were used instead.

## Source identity and model

The candidate source-manifest SHA-256 is `075367639931c07212c3ffa5660f5a7bd1b91c0478c161aaab2cfcef52c1a6a5`. The diff SHA-256 is `b37f21440ea37f26175a5f2b841656beac8d62543941988d64460b43b3081b37`. The 139-entry before archive SHA-256 is `f11c41ebb482e1b43c266980125809d0b888e0c07a499fecfaf2ddab8587b8e4`; all archive entries matched `before-sha256.json`.

At the final integrity check, all 143 pinned candidate paths existed. Only `docs/STATE.md` and `tools/verify_windows_app.ps1` differed from the pinned source manifest, consistent with the caller's expected state/proof-harness edits. No pinned application file differed, and no unlisted application files were found under the scanned source roots. Details are in `integrity-final.json`. The probed release CLI SHA-256 is `ebe1f8d98a758780cb767d4f7650ac4f93f05f3b6733f37ba528f469beb9b0e0`; its build-to-source provenance relies on the main session's evidence, not this review.

The requested model was `openai/gpt-6.1-terra#max`. The harness identifies this reviewer as GPT-6.1 Terra, and the model catalog lists `max` as available. Active effort or variant telemetry was not independently observable, so availability is not proof that `max` was active. No global settings were changed.
