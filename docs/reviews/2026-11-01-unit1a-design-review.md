# Unit 1a protocol design review (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: `docs/design/unit1a-protocol.md` at `dc64e43` (revision 1), with the application source at the same commit
- Verdict: **Design ready to implement: NO.** 13 findings (10 P2, 3 P3), no P0 or P1. Revision 2 of the design answers each finding (its section 9).

The text below is the reviewer's report. Typographic quotation marks, apostrophes and dashes became plain ASCII, and the reviewer's links to local files became plain text with the repository path. Other differences cannot be excluded.

---

## Ranked findings

The findings are **by inspection** of the design, the requested source at `dc64e43`, its callers, and the pinned Tauri and serde implementations. The proposed changes do not exist yet, so these are not reproductions of implemented fixes. I found no P0 or P1 issue.

1. **P2: Listener registration does not guarantee lock-event delivery.**  
   In design §2 (`docs/design/unit1a-protocol.md:20`), both listeners can register successfully, an unlocked status can be computed, and the native session can then lock before that status reaches the frontend. If `vault-locked` emission fails, lib.rs (`desktop/src-tauri/src/lib.rs:29`) currently discards the error. No tombstone reaches `nativeSession.ts`, so the old status can still be accepted until polling catches up. Tauri's event bridge schedules JavaScript evaluation; successful registration is not an acknowledgment that each later event was processed.

   Define delivery-failure handling and a session-owned reconciliation mechanism, such as acknowledged/replayable state with a health timeout. Preserve the rule that a failure for A cannot lock B. State the achievable bound during a broken channel rather than promising instantaneous UI synchronization. Test an emit failure after registration and an event delayed past the status reply.

2. **P2: A resolved gate can outlive its React root.**  
   In design §2 (`docs/design/unit1a-protocol.md:26`), root A's gate can resolve, then A can unmount before its awaiting `loadStatus()` continuation runs. A JavaScript promise that has already resolved cannot be revoked. The continuation can call `api.status()` after disposal, capturing the new generation. api.ts (`desktop/src/api.ts:28`) calls `acceptNativeStatus` before the App's request counter checks the reply, so the disposed root can still mutate shared native-owner state. Root B's listeners may not yet be ready.

   Bind the gate and request to a root identity. Check it after the wait and before dispatch, and again before `api.status()` changes shared state. Reject or settle failed and disposed gates, and clean up listeners that resolve late. Test disposal immediately after gate resolution, including a new root still registering and a late old reply. Also test subscription failure when the frontend has no known owner; a failed `lock(None)` must remain visibly unconfirmed.

3. **P2: A delayed import preview can be rebound to a newer vault session.**  
   Design §3 (`docs/design/unit1a-protocol.md:45`) adds a request ID but does not bind the request to its originating native owner. The current preview handler (`desktop/src-tauri/src/commands.rs:655`) takes its session snapshot when native code receives the IPC. Preview A can be dispatched, delayed, overtaken by a lock and opening session B, and then handled using B's snapshot. It can install A's retained input in B and replace B's valid review. A frontend generation fence cannot undo that native side effect.

   Bind preview registration, cancellation, and installation to the originating native session and presentation, and validate that binding under the native gate. Test delayed A IPC arriving after B has installed a review; B's token must remain confirmable and A must not be installed.

4. **P2: A bounded cancellation set can resurrect an abandoned preview.**  
   Design §3 (`docs/design/unit1a-protocol.md:46`) permits retaining only the last 16 cancelled IDs. Cancel delayed preview A, cancel 16 later previews, and then let A reach `store_preview`. A's tombstone has been evicted, so it can be installed and can replace a newer review. The same problem applies to any fixed eviction limit without a bound on outstanding work.

   Register in-flight work natively and retain its cancellation state until that work has terminated, or bound admission so an outstanding cancellation cannot be evicted. Define cancel-before-registration ordering as well. Test more cancellations than the capacity while the oldest preview is stalled.

5. **P2: Cancellation is not guaranteed across failure and lifecycle changes.**  
   Design §3 (`docs/design/unit1a-protocol.md:48`) explicitly leaves a review alive when cancel IPC fails. The old token can therefore remain confirmable until a later lock or expiry, contrary to PROJECT.md's import contract (`docs/PROJECT.md:46`). The listed triggers also omit Settings unmount/navigation and webview reload. Current Settings cleanup (`desktop/src/components/Settings.tsx:67`) only increments frontend counters.

   Specify native invalidation for every abandonment path, including navigation, root replacement, reload, hide, and a failed or lost cancel. A frontend warning alone does not revoke native state. If communication failure prevents confirmation of revocation, retain an appropriately scoped warning and fail closed until native revocation is established. Test installed and in-flight previews across each path.

6. **P2: The confirm/cancel race has no defined native winner.**  
   Design §3's table (`docs/design/unit1a-protocol.md:60`) says a confirmation dispatched before dialog close completes. Separate IPC calls can reach the native gate in the opposite order. If cancel removes the review first, `take_preview` returns `stalePreview`; if confirm takes it first, the worker may proceed. Lock or a genuine write error can also prevent completion.

   Define the native linearization point. If cancellation wins, report a truthful rejection with no import. If confirmation has been accepted, preserve its eventual success or failure acknowledgment, including a committed result overtaken by locking. Test both orders and a lock during the accepted confirmation. If the stronger frontend-dispatch guarantee is intended, specify the required acceptance handshake.

7. **P2: Clipboard sequence zero and reuse are misclassified as ownership proof.**  
   Design §4 (`docs/design/unit1a-protocol.md:69`) treats any changed sequence as another program's write. A captured nonzero sequence followed by `0` would therefore discard the lease without warning, although Microsoft documents zero as a possible unavailable-access result. A partial publication can also capture zero. Conversely, equality after sequence-number reuse cannot alone prove that an identical newer user copy is still AKM's. [Microsoft's API documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber) describes the zero result.

   Treat an invalid or unavailable sequence as uncertain, retain the lease and warning, and never clear based on zero. Specify and prove an additional ownership/change check for sequence reuse, or explicitly identify the remaining limitation without claiming the newer-copy guarantee. Test zero at capture and cleanup, injected reuse, rollback failure, and retry. State that retries cannot continue after process termination.

8. **P2: A failed stdout export can already have disclosed secrets.**  
   Design §5 (`docs/design/unit1a-protocol.md:82`) says any failed write gets today's nonzero error and no completion record. export.rs (`crates/akm-cli/src/export.rs:343`) writes multiple chunks and then flushes. A pipe can receive one or more fake-secret bytes before a later write or flush fails. A generic failure is not evidence that nothing was disclosed, and an automated retry can repeat the disclosure. Rust's [`Write` contract](https://doc.rust-lang.org/std/io/trait.Write.html) permits partial writes.

   Keep the nonzero result and omit a false completion record, but report that output may have been partially disclosed and should be inspected before retrying. Test a writer that fails after accepting bytes and one that fails on flush. Keep the pre-disclosure journal failure closed.

9. **P2: Rules 3, 4, and 7 contradict the stated scope and existing contracts.**  
   Design §1 (`docs/design/unit1a-protocol.md:10`) applies all seven rules to every exchange, but the proposed changes do not do that. Rule 3 would suppress an old session's secret-free completion acknowledgment after B opens, despite PROJECT.md (`docs/PROJECT.md:43`). Rule 4 says every native result after cancellation is discarded, conflicting with already-authorized imports and file publication. Rule 7 promises completion records for every disclosure, while existing reveal and copy paths intentionally record requests without claiming that the user saw or received the value.

   Narrow the rules to protected session-bound results, native state that is actually cancellable, and outcomes the program can know. Explicitly retain origin-owned receipts and application-wide warnings. Rules 1, 2, 5, and the intent of 6 are useful with the failure qualifications above. Test the A-to-B receipt and already-authorized publication cases.

10. **P2: The verification sequence does not bind proof to the final build.**  
    Design §7 (`docs/design/unit1a-protocol.md:96`) calls for checks and then a rebuild whose hashes replace the pinned hashes. It does not require the release-CLI and permitted native-flow proofs to run on those final bytes. Mocking the frontend boundary and separately testing Rust components also cannot establish that the new command is registered and that its actual IPC arguments and results agree.

    Build the final candidate, record its hashes, run the applicable release-CLI and real frontend/native boundary checks against that candidate, and recheck the hashes. Keep owner-gated tests and R4-04 explicitly deferred where authorization or proof is missing. Add a real dispatch/serialization test as well as mocked ordering tests.

11. **P3: Generic JSON parsing can invent an unambiguous declared sequence.**  
    Design §6 (`docs/design/unit1a-protocol.md:90`) would parse `{"seq":7,"seq":8,"ts":"bad"}` into a generic object. The pinned `serde_json::Value` implementation keeps the later duplicate, so the UI could display 8 even though the record declares two sequences. Duplicate object names are an interoperability ambiguity under [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259.html#section-4). The design also says *every* non-negative integer fits `Option<u64>`, which is false for `18446744073709551616`.

    Detect duplicate `seq` members before collapsing the object, including escaped equivalent names, and show no numeric value when ambiguous. Define the supported integer range and handle larger values without rounding, overflow, or echoing other record contents. Test zero, values around `2^53`, `u64::MAX`, one above it, duplicates, and malformed JSON.

12. **P3: The completion-log warning states more than the append result proves.**  
    Design §5 (`docs/design/unit1a-protocol.md:82`) says completion "could not be recorded." AuditLog::append (`crates/akm-core/src/audit.rs:115`) can write the entire record and then fail at `sync_data`. The record may already be present.

    Say the completion record **could not be confirmed**, while retaining the non-retryable export result. Inject a failure after the write and before successful sync, and check both the warning and the possible journal contents.

13. **P3: CLI completion records add metadata without linking outcomes to requests.**  
    Design §5 (`docs/design/unit1a-protocol.md:82`) adds the destination to the plaintext sidecar, which can live beside a synced vault. That newly exposes CLI export paths. Meanwhile, two concurrent CLI exports with the same count have indistinguishable request records, so a reader cannot reliably associate a later completion with its request.

    If per-request pairing is required, put an opaque operation ID in the existing detail field of both new records and avoid recording the destination unless its value justifies the extra exposure. Test interleaved exports. This can retain the existing journal JSON schema; the proposed import and audit changes do not require a KDBX format change.

## Per-section assessment

| Design section | Assessment |
|---|---|
| 1. Rules | **Unsound.** Several universal statements conflict with receipts, authorized publication, or existing disclosure semantics. |
| 2. Listener gate | **Unsound.** Registration, root disposal, and event delivery need a complete ownership and failure protocol. |
| 3. Import reviews | **Unsound.** Session binding, bounded cancellation, lifecycle revocation, and confirm ordering are unresolved. |
| 4. Clipboard cleanup | **Sound with changes.** The three-way read result is useful, but sequence validity and reuse need treatment. |
| 5. CLI completion | **Sound with changes.** Recording known publication is appropriate; partial output, uncertain append, correlation, and metadata need decisions. |
| 6. Damaged sequence | **Sound with changes.** Untrusted display is appropriate; duplicate keys and numeric limits need explicit handling. |
| 7. Checks | **Sound with changes.** Add the missing interleavings and bind real-boundary evidence to the final build. |
| 8. Limits | **Sound with changes.** Add transport-loss, sequence-reuse, process-exit retry, and deferred-proof limits. |

The failure-case tables are **not yet sufficient** as regression specifications. In addition to the cases above, they should cover either listener failing after the other succeeds, old A events during each stage of B opening, repeated cancellation, a wrong token preserving B, both orders of cancel versus confirm, and separate clipboard warning IDs across recovery and recurrence.

**Design ready to implement: NO.** The proposed direction addresses the reported symptoms, but the import protocol can still install work under the wrong session or resurrect cancelled work. Listener readiness does not resolve delivery failure, and several global rules conflict with existing outcome and publication contracts. The design needs those boundaries and their observable failure outcomes settled before it can guide one implementation and review cycle.

## What I did not check

I did not run tests or probes, launch the GUI, access the clipboard or app settings, alter a window or the Windows session, inspect real secrets, build binaries, or verify KDBX interoperability. I did not independently establish real Tauri event timing or real Windows clipboard fault behavior. This was a scoped design review, not a full source acceptance review or security audit. The checkout remained clean.
