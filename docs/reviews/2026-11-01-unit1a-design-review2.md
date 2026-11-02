# Unit 1a protocol design review, revision 2 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: `docs/design/unit1a-protocol.md` at `9c814c0` (revision 2), with the application source at the same commit
- Verdict: **Design ready to implement: NO.** 9 findings (7 P2, 2 P3), no P0 or P1. Of the 13 revision-1 findings, 8 are resolved and 5 partly resolved. This is the second failed review of the design. Under AGENTS.md, revision stops, and the design question goes to the owner (STATE.md).

The text below is the reviewer's report. Typographic quotation marks, apostrophes and dashes became plain ASCII, and the reviewer's links to local files became plain text with the repository path. Other differences cannot be excluded.

---

**Revision 2 is not ready to implement.** The lease protocol still permits an abandoned request to replace a newer review, and recovery from a failed cancellation can affect a later vault session. The event and verification protocols also leave important gaps.

All findings, dispositions, and verdicts below are **by inspection**. I ran no probes or tests. The checkout remained clean at `9c814c0`.

## Ranked findings

1. **P2: A delayed lease-open request can replace a newer review.**
   **Design §3:** unit1a-protocol.md:56 (`docs/design/unit1a-protocol.md:56`). **By inspection.** In one vault session, start `env_import_open` for review A, close A while that IPC is delayed, then open and install review B. When A's open reaches native code, its `sessionId` is still current, so it creates a lease and replaces B. The frontend did not yet know A's native-generated lease ID when it closed A. Similarly, an open sent before a presentation expiry can arrive afterward and bind itself to the *new* presentation. A delayed open can also arrive after the page-load hook cleared the old lease.

   **Needed:** Bind admission to the originating document, presentation, and attempt, and define cancellation before native registration. An obsolete open must never replace a newer lease. Test the same-session A/B ordering, cancellation before the open reply, presentation expiry, and page reload with the old open delayed across each boundary.

2. **P2: A silently lost view-expiry event is not replayed by status.**
   **Design §§2, 8:** unit1a-protocol.md:30 (`docs/design/unit1a-protocol.md:30`). **By inspection.** The health check detects missing status replies, but a successful status contains no presentation identity. If native presentation expiry occurs, `vault-view-expired` is not processed by JavaScript, and the independent `document.hidden` path also does not fire, successful status polls keep the channel healthy without telling the UI that its presentation expired. A reveal completed natively before expiry but delivered afterward can then be displayed by the old view. The current KeyDetail.tsx:79 (`desktop/src/components/KeyDetail.tsx:79`) relies on its local presentation counter being advanced.

   **Needed:** Define replay or reconciliation for presentation expiry, including how a delayed protected reply is fenced before display. State any unavoidable interval before reconciliation. Test a lost expiry event with successful status polls and a reveal reply delivered both before and after reconciliation.

3. **P2: Cancellation recovery can lock a replacement session and claim an unconfirmed lock succeeded.**
   **Design §3:** unit1a-protocol.md:61 (`docs/design/unit1a-protocol.md:61`). **By inspection.** A close from session A can fail after session B opens. The prescribed fallback calls the existing `lock()`, which first changes the current view and then obtains the *current* owner through api.ts:80 (`desktop/src/api.ts:80`). It can therefore hide and lock B. The proposed message also says "The vault was locked" before the awaited lock has been confirmed; a hanging call can leave that message visible indefinitely.

   **Needed:** Carry A's owner and frontend generation through cancellation and recovery. Do not invalidate or lock B. Show an unconfirmed-lock message until an owned native lock is acknowledged. Test both rejected and hanging A cancellations after B opens.

4. **P2: Revocation depends on callbacks and IPC calls that can remain pending.**
   **Design §§2-3:** unit1a-protocol.md:26 (`docs/design/unit1a-protocol.md:26`), unit1a-protocol.md:59 (`docs/design/unit1a-protocol.md:59`). **By inspection.** A listener registration that never resolves makes neither `ready` nor `failed` true. After a webview reload, a native vault may already be open while the UI waits indefinitely. Likewise, a close request that hangs never reaches the prescribed retry or lock fallback. The page-load hook is not a complete substitute: the pinned Wry implementation maps Tauri's `Started` event to WebView2 `ContentLoading`, and Microsoft says `ContentLoading` may be absent in navigation error cases. [Microsoft WebView2 navigation events](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/navigation-events)

   **Needed:** Specify deadlines and an explicit unconfirmed state for registration, close, and lock. Keep the affected workflow disabled until revocation is confirmed or provide an independently enforced native revocation mechanism. Test hung calls and navigation that does not produce the relied-on callback.

5. **P2: The stated frontend timing bounds are not guaranteed.**
   **Design §§2, 8:** unit1a-protocol.md:34 (`docs/design/unit1a-protocol.md:34`). **By inspection.** The three-second poll and one-second health timer can run late even when the script has not stopped. The HTML timer specification explicitly does not guarantee scheduled execution times. [HTML Standard timers](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers) Also, the design does not define how the health timestamp is initialized for each newly unlocked owner or how old, delayed status replies affect it. The existing App can have overlapping polls, and its request counter discards older replies after the API wrapper has processed them.

   **Needed:** Describe these as best-effort scheduling bounds, or provide a mechanism capable of enforcing the claimed deadline. Define per-owner initialization, stale-reply handling, and request deadlines. Test delayed timer execution, a long stay on the locked screen followed by unlock, overlapping polls, and a pre-lock status reply arriving late.

6. **P2: The specified confirmation outcomes do not match the current receipt path under session loss.**
   **Design §3:** unit1a-protocol.md:64 (`docs/design/unit1a-protocol.md:64`). **By inspection.** The native winner is now defined, but the design says a close-first rejection produces `reportedError` and an accepted confirmation produces its true terminal result. In api.ts:48 (`desktop/src/api.ts:48`), an error received after the frontend generation changes settles the receipt as `unconfirmed`. A native `stalePreview` delivered after a lock therefore does not meet the stated close-first result. If IPC is lost entirely, the true result cannot always be known.

   **Needed:** Define which secret-free native acknowledgments can safely settle an old receipt, and explicitly retain `unconfirmed` when delivery itself is unknown. Test both gate orders with a session change before the reply, plus a lost reply.

7. **P2: The verification plan can pass without testing the production frontend-to-handler wiring.**
   **Design §7:** unit1a-protocol.md:142 (`docs/design/unit1a-protocol.md:142`). **By inspection.** The existing ipc_dispatch.rs:86 (`desktop/src-tauri/tests/ipc_dispatch.rs:86`) builds its own `generate_handler!` list, separate from the production list in `lib.rs`, and supplies handwritten JSON. Extending that harness alone can pass while production omits a command or `api.ts` sends different arguments. The plan also requires release-CLI fault cases, but the current post-write journal sync fault hook is compiled only under `cfg(test)`; the plan does not identify a release-process fault mechanism.

   **Needed:** Share or otherwise verify the production handler registration, exercise the actual frontend argument shapes against it, and name the final-build native flow that covers the new import commands. Separate executable release checks from deterministic injected-fault tests and state the evidence each provides.

8. **P3: Clipboard warning settlement conflicts with the required retained-warning case.**
   **Design §4:** unit1a-protocol.md:107 (`docs/design/unit1a-protocol.md:107`). **By inspection.** The design says a successful retry "settles the warning, as R4-03 does today," but the current session.rs test:1689 (`desktop/src-tauri/src/session.rs:1689`) retains the first warning after recovery, then requires a distinct second warning before the first is acknowledged. If "settles" means removing it, the proposed behavior conflicts with that requirement; if it means only ending the cleanup episode, the wording is ambiguous.

   **Needed:** Define the warning's visible and acknowledgment lifecycle. Preserve the first warning until acknowledgment while giving a later episode a distinct ID. Test recovery, recurrence, and acknowledgment in that order.

9. **P3: The failed-file-export specification makes an impossible destination claim.**
   **Design §5:** unit1a-protocol.md:122 (`docs/design/unit1a-protocol.md:122`). **By inspection.** An export to an existing destination must fail without overwriting it, so a failed export can leave a file at that destination: the user's pre-existing file. The source also attempts cleanup of its owned plaintext temporary file but ignores failure from the Windows deletion request in export.rs:267 (`crates/akm-cli/src/export.rs:267`).

   **Needed:** Require no *new* destination and byte-for-byte preservation of a pre-existing destination. Specify and test the observable result when temporary-file cleanup cannot be confirmed.

## Earlier findings

All dispositions in this table are **by inspection** of revision 2, not claims that an implemented fix has passed review.

| Earlier finding | Disposition | Reason |
|---|---|---|
| 1. Event delivery | Partly resolved | Lock-event failure has polling and health recovery, but the hard timing claim and lost presentation-expiry case remain. |
| 2. Disposed root | Resolved | The root guard is checked before dispatch and before status adoption, with late-listener cleanup. Hanging registration remains a separate failure-path gap. |
| 3. Preview rebound to session B | Resolved | An existing A lease, and an A open carrying A's owner, are rejected after B owns the native session. Same-session and presentation ordering remain defective. |
| 4. Cancelled-set eviction | Partly resolved | One current lease removes the bounded-set eviction problem, but cancellation before lease registration is still undefined. |
| 5. Cancellation and lifecycle | Partly resolved | More abandonment paths and failed-close recovery are named, but pending IPC, page-load limits, and owner-scoped recovery remain. |
| 6. Confirm/cancel order | Partly resolved | The native linearization point and both gate orders are defined. Receipt behavior after session change or lost delivery is incomplete. |
| 7. Clipboard zero and reuse | Resolved | Zero is treated as uncertain, changed sequences preserve newer copies, and wrap and process-exit limits are explicit. The captured-zero case deliberately keeps a warning rather than an unusable retry lease. |
| 8. Partial stdout disclosure | Resolved | Partial writes and flush failures get a nonzero result and an explicit partial-output warning. |
| 9. Overly universal rules | Resolved | The rules now distinguish protected results, origin-owned receipts, cancellable state, and knowable outcomes. |
| 10. Final-build proof | Partly resolved | Hash-before/check/hash-after is specified, but production handler and frontend wiring, and fault-injection evidence, are not adequately bound. |
| 11. Duplicate and out-of-range sequence | Resolved | The visitor counts decoded names and specifies numeric range and representation. |
| 12. Completion warning wording | Resolved | "Could not be confirmed" accounts for an append that wrote before sync failed. |
| 13. Correlation and destination metadata | Resolved | Both records get an opaque operation ID and omit the destination. |

## Per-section verdict

| Section | Verdict | Reason |
|---|---|---|
| 1. Rules | Sound with changes | The narrowed rules are useful, but their cancellation and uncertainty requirements need the missing failure protocols. |
| 2. Session status and events | Unsound | Presentation-event loss, pending registration, and claimed timing bounds remain unresolved. |
| 3. Import reviews | Unsound | Lease admission, lifecycle revocation, owner-scoped recovery, and receipt delivery have unresolved races. |
| 4. Clipboard | Sound with changes | The ownership decision table is substantially improved; warning settlement needs precise semantics. |
| 5. CLI export | Sound with changes | Completion and partial-output handling are sound in direction; failed file and cleanup outcomes need correction. |
| 6. Damaged sequence | Sound | The stated duplicate, range, format, and untrusted-display rules address the earlier defect. |
| 7. Checks | Unsound | The proposed layers do not yet establish production wiring or specify the required release fault evidence. |
| 8. Limits | Unsound | It omits the lost view-expiry and timer-scheduling limits. |
| 9. Finding mapping | Sound with changes | It identifies the intended changes, but overstates closure of findings 1, 4, 5, 6, and 10. |

The failure-case tables are **not sufficient regression specifications**. They need the interleavings in the findings above and real B-lock events at each stage of B's opening, alongside the old A-event cases. They also need observable assertions tied to the affected baseline. For example, preserving a newer identical clipboard copy already passes in the old source; that case must be paired with the unreadable same-sequence case that exposes the defect. A test that merely finds the newly added command absent on `cfeb1b0` does not by itself reproduce the old cancellation behavior.

## What I did not check

I did not run tests, probes, builds, the GUI, or any clipboard, settings, window, or Windows-session operations. I did not verify actual WebView2 event timing, Win32 clipboard faults, final binaries, KDBX interoperability, or the full application security posture. No repository files were edited.

Design ready to implement: NO
