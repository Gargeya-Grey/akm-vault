# Unit 1a corrected-candidate acceptance review

## Verdicts

**SOURCE correction verdict: reject this candidate.** Two P2 findings violate the fail-closed disclosure and late-completion acknowledgment contracts. Four P3 findings affect warning delivery or truthful presentation. This is a scoped source and targeted-runtime review, not a numerical rescore or professional audit.

**RUNTIME proof verdict: partial and not renewed by this reviewer.** The main session reports Rust 153 passes with two gated ignores, strict Clippy and rustfmt, frontend 43, production build and browser 10, release CLI/GUI/MSI/NSIS builds, exact-GUI journal/dismissal and export proofs, and real KeePassXC interchange. The first export automation attempt failed startup-control discovery; the later retry passed after a root-refresh harness correction. The browser document-reload proof used mocked retained IPC, not a real Tauri reload. Native late-lock and sync-failure cases were injected, not observed Windows disk-sync failures. These results do not override the findings below. Layout checks reported pre-existing Activity/Keys debt, not a layout pass; that debt is not counted as a new finding here.

## Ranked findings

### 1. P2: A form-feed at the journal boundary permits disclosure and corrupts the appended record

`crates/akm-core/src/audit.rs:185-202` skips an ASCII-whitespace-only line before checking for a terminating newline. A final form-feed byte `0x0c` is accepted as whitespace by Rust but is not JSON whitespace. Append therefore writes the new JSON object immediately after that byte and returns success. The request record is malformed, yet `crates/akm-cli/src/main.rs:305-316` proceeds to disclose the secret. Native reveal, copy and export use the same `Vault::log`/append path in `desktop/src-tauri/src/commands.rs:221-230,378-390,452-465`.

**Reproduced against the pinned release CLI with fake data.** After an otherwise valid journal and trailing `0x0c`, `audit --verify` exited 0, `get --field secret` exited 0 and returned the fake secret, and the next `audit --verify` exited 2 for damage. The operation changed the original journal bytes. The space and vertical-tab controls behaved differently. See `probe_whitespace_boundary.py` and `whitespace-boundary-result.json` in this directory.

**Fix and acceptance test.** Validate every skipped byte and the actual append boundary under the exclusive lock. Reject non-JSON whitespace or any other unsafe final fragment before append, without repairing bytes. Test empty and nonempty journals, terminated and unterminated blank lines, form-feed, valid JSON whitespace and malformed data through core plus CLI/native disclosure, export and launch callers. Assert unchanged original bytes and no disclosure or launch on refusal.

### 2. P2: A lock-overtaken completion can lose its nonretryable receipt between root mounts

`desktop/src/api.ts:17-29` emits a completion event when native returns `completedWhileLocked`. `desktop/src/sessionBoundary.ts:14-15` only dispatches that event. `desktop/src/App.tsx:56-64` has no retained completion state outside the currently mounted root. If A's committed export or mutation completes after A's root unmounts but before the next root mounts, the event has no listener. With no journal or backup warning there is also no native operation notice to recover it. The old Settings handler suppresses results after unmount at `desktop/src/components/Settings.tsx:66-74,199-212`. B can remain active without ever seeing A's secret-free "do not repeat" acknowledgment.

**Reproduced with the actual App/API code and mocked native IPC in jsdom.** A delayed fake `completedWhileLocked` response delivered between roots retained the error for its abandoned promise, but the remounted App showed no completion receipt. This was not a native Tauri test. See `acceptance-handoff.test.tsx` and `probe-results.md`.

**Fix and acceptance test.** Retain and replay secret-free completion receipts across root handoff with origin-generation fencing and explicit acknowledgment. Test A commit, lock and B opening, delivery between roots, then remount. Require a visible nonretryable receipt and no false lock of B. Do not conflate this with native warning IDs or promise crash durability.

### 3. P3: A drained application safety warning can disappear during the same handoff

`desktop/src-tauri/src/session.rs:525-530` destructively drains application warnings, while `desktop/src/operationNotices.ts:11-16,23-25` caches only operation notices and broadcasts application strings to current listeners. `desktop/src/App.tsx:66-77` has no application-warning replay. If a warning reply arrives between roots, the native warning is gone and the next root cannot show it. This can hide a one-time native session-monitor failure or clipboard-cleanup failure. It is distinct from operation notice retention.

**Reproduced in the same jsdom probe.** A drained fake application warning disappeared while the operation notice in the same response was replayed. Native queue behavior is supported by source inspection, not a real Windows fault injection.

**Fix and acceptance test.** Preserve app-global warning delivery separately from fixed operation notices, including responses arriving between roots. Test an actual App/API root handoff with a one-shot mocked native warning and verify that it remains visible until explicitly dismissed, without attributing it to B or exposing vault-specific data.

### 4. P3: Some valid unknown action names crash Activity instead of displaying an unknown outcome

`desktop/src/components/Activity.tsx:7-18,127-129` indexes an ordinary object with untrusted `entry.action`. For `__proto__`, this returns the inherited prototype object rather than `undefined`, so React receives an object as a child and Activity fails to render. `AuditLog::append` accepts arbitrary action strings at `crates/akm-core/src/audit.rs:69-77`; the journal is unauthenticated and can contain unrecognized legacy actions. A valid but unknown action must not turn into an application render failure.

**Reproduced in two bounded parts.** A fake, correctly hashed `__proto__` journal was accepted as consistent by the release CLI. Separately, the actual Activity component crashed when mocked native IPC returned that action; a test-only React boundary caught the error. This was not a real Tauri journal flow. See `probe_legacy_action.py`, `legacy-action-result.json`, and the Activity case in `acceptance-handoff.test.tsx`.

**Fix and acceptance test.** Use an own-key lookup, null-prototype dictionary or `Map`. Exercise valid journal actions `__proto__`, `constructor` and `toString` through the report and Activity. Require their literal unknown-outcome labels and no render failure.

### 5. P3: Delete success still claims that a `.bak` copy exists when refresh failed

`desktop/src/components/KeysView.tsx:108-116` unconditionally reports that a backup remains in the `.bak` file after deletion. The confirmation at lines 337-341 also describes that copy as guaranteed. But `crates/akm-core/src/vault.rs:550-565` allows a committed deletion to succeed when the convenience `.bak` refresh fails, retaining a separately dated encrypted backup and emitting `BackupRefresh`. For example, an existing directory at the `.bak` path causes this condition, as exercised by `desktop/src-tauri/src/session.rs:786-803`. The success flash can mislead recovery even though the new backup warning is correctly classified. This is a pre-existing caller claim exposed by the new typed-warning contract, not a claimed new regression in the delete code.

**Fix and acceptance test.** Avoid guaranteeing the convenience `.bak` copy. Describe the retained dated backup accurately and surface the refresh warning. Test the actual delete UI with a successful deletion and failed `.bak` refresh, including the immediate success message and later warning.

### 6. P3: Large offending sequence numbers are rounded in Activity

`crates/akm-core/src/audit.rs:18,202-220` accepts a parsed `u64` offending sequence. The native `broken_at` DTO at `desktop/src-tauri/src/views.rs:161-168` serializes it as a JSON number, while `desktop/src/types.ts:57-63` and `desktop/src/components/Activity.tsx:90-95` use a JavaScript number. A damaged record with sequence `9007199254740993` is displayed as `9007199254740992`, contrary to the known-sequence reporting contract. This is a rare malformed-chain presentation case, not a claim that normally generated journals reach that size.

**Evidence and fix.** `probe_sequence_precision.cjs` reproduces the JavaScript JSON-number rounding; the native path is inspection-supported, not independently run. Transfer the offending sequence as an exact decimal string, or otherwise preserve its exact value. Test values on both sides of `2^53`, the physical line, and a malformed record for which no sequence is known.

## Positive seams and limits

Inspection supports the intended typed core warning adapter and fixed native kinds, origin-only warning payloads, ID-based native operation-notice retention, delayed-poll tombstones, one-lock journal snapshots, requested/completed action naming, and post-write sync-uncertainty wording. The export publication code retains the authorized-publication boundary and uses the originating ticket for its journal warning. Those seams do not address the counterexamples above. The current plaintext journal remains beside the vault and does not authenticate or prove completeness.

The reviewer ran no full build or suite, GUI/native automation, real clipboard or OS lock/suspend test. Targeted jsdom tests mock native IPC. The CLI probes used only fake secrets and self-deleting scratch data. `probe-results.md` records the commands, outcomes and the initial scratch-test setup failure.

## Source and model identity

At the final integrity check, all three frozen input hashes matched. The 144-path source manifest SHA-256 was `72202c318c7a9c1b3d7bf057a89bd31277b921c9bdcd8c3cdc10e1267442d066`. The only pinned mismatch was the main-owned `docs/STATE.md`, an allowed documentation change. No pinned application/build input differed, and no unpinned code file was found in the inspected application source directories. `source-integrity.json` contains exact hashes and the directory scope. The independently hashed release CLI was `382422d789ded2216aa8f3d344b8be72c27bd76a88f77fff88b76c9d2a5983e7`; the GUI was `c5ed1a150d97eca63687a9a2d923f522d7245bb9580124b87e82e2663c128aca`. Hash equality alone does not establish runtime behavior or build provenance.

The caller requested `openai/gpt-6.1-terra#max`. The harness identified this reviewer as `openai/gpt-6.1-terra`, and the model catalog listed `max` as available. Actual active effort and actual token usage were not exposed to this reviewer.

**Next action.** Main should preserve this raw review, disposition the findings, correct the two P2 failures and applicable P3 defects with permanent regressions, then repin and request fresh bounded acceptance. No project file or failed round-one evidence was modified by this reviewer.
