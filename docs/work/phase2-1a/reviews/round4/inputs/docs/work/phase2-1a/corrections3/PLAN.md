# Recalibrated unit1a cycle: outcome ownership, warning episodes, reply fencing

Owner authorization on 2026-10-31: "Yes, please recalibrate yourself and then do it." One new bounded cycle for **1A-R3-01/02/03**, followed by one fresh scoped acceptance. Other phase-two units, publication, installation and real secrets remain excluded. All three rejected reviews stay immutable.

Tier2: the three findings cross API/React lifetimes and native clipboard-session handling. Goal: retain exactly one truthful, secret-free outcome per dispatched import/file save while rejecting obsolete protected replies and preserving separate warning episodes.

## Recalibration before implementation

The earlier checks tested hide and unmount mostly in isolation. A view request counter correctly cancels protected results but is not an owner for a dispatched operation's acknowledgment. Native warning IDs correctly protect delivery, but message equality is not an episode identity. Root invalidation was added without removing all exceptions/error bypasses from the API fence. Passing example tests did not prove those combined transitions.

This cycle therefore fixes ownership at the dispatch/episode boundary, not by adding a second delayed callback to Settings. Its tests compose hide, navigation, return, root teardown, lock/openB, result/error/cancel, retry/recovery and acknowledgment.

## Serial ownership and model policy

Main-session host-default Sol owns all implementation, tests and collection. Files share outcome contracts, so implementation workers would duplicate context or conflict. One new read-only `openai/gpt-6.1-terra#max` reviewer/collector gets frozen plan/full combined diff/pinned source/verification, never the chat or implementer transcript. Verify availability before dispatch. This exact reviewer model is owner-authorized; active effort may not be observable. No model, effort or routing configuration changes and no unrelated model selection. Main-session recalibration is the escalation here, with the owner's renewed bounded budget, not an unauthorized stronger implementation delegate.

## Fixed contracts

### Application-owned outcomes (R3-01)

- Track `env_import_confirm` and `env_save` **at API dispatch**, independent of Settings lifetime. Allocate one ID and origin frontend generation. Retain only `{id,generation,operation,outcome}`; never retain the import text/token/scope, counts, native error, export path or protected results in this registry.
- Operations are `import`, `export`, or the existing generic late-commit `operation`. States are `pending`, `completed`, `reportedError`, `cancelled`, `unconfirmed`, `completedWhileLocked`. A successful import/file-publication reply is known completion; native `completedWhileLocked` and fenced successful committed replies keep the existing nonretryable receipt. Null native file-save reply is cancellation, not saved success. Ordinary rejection means an error was reported, not proof that nothing was saved. A stale ordinary error is unconfirmed and sanitized.
- Pending and final state use the **same ID**, with one settlement per native invocation. Pending cannot be acknowledged away; settled IDs remain until explicit dismissal. Dismissing a displayed snapshot cannot erase a later unseen request. No browser persistence/crash-durable receipt claim is added.
- All text rendered from this registry is fixed, path/scope/value-free. Never classify a rejected/cancelled request as saved, or suggest blind retry. Counts/destination/error details may still appear only in the valid originating Settings view as local details, not a second retained receipt.
- Separate outcome-state updates from the existing immediate session-loss signal: normal success, pending, cancellation/error, replay and acknowledgment **cannot lock** current B. Only a known completed-while-locked outcome may invoke the existing generation-owned lock signal, and only for its still-current generation.
- Hide/unmount/edit invalidate protected local preview, token and file-save result display without cancelling the registry's dispatched operation. Normal mounted in-context details remain usable. Hide must not leave stale local busy state stuck. Already-dispatched confirmation/file save settles even after navigating to Keys, returning to Settings, or remounting the root.
- Do not store secret data to make this reliable. Do not move filesystem publication under a security gate, change native persisted-result semantics or weaken scoped import/export authorization.

### Cleanup warning episodes (R3-02)

- Keep native ID-based delivery/acknowledgment and app-global versus operation-origin separation unchanged.
- Track an unresolved **clipboard cleanup episode by owned lease generation**, not by equality of pending message strings. All Session cleanup entry points use one outcome handler. First failed cleanup for an episode creates one warning ID; retries of that same unresolved episode do not create unbounded warnings, even after its ID is dismissed.
- Actual cleanup success/no owned lease ends the episode while its historical warning remains pending. A later failure, or failure cleaning a newly published owned lease, creates a new ID even if the earlier identical-message warning is still pending.
- A successful timer tick that did not attempt cleanup because a lease is not due is **not recovery**. Observing a newer user-owned clipboard and relinquishing our old lease is recovery without clearing the user's contents.
- Add only opaque lease-generation introspection if needed; preserve native Windows transaction, sequence/text matching and cleanup/publication behavior. Generic one-off `Session.warn` calls are explicit occurrences, not timer retries; they are not deduplicated by old pending strings.
- Use fake backend fault injection for failure/retry/recovery/new-copy/failure; no real clipboard access is authorized for these tests.

### Complete API fencing (R3-03)

- Capture origin generation for every ordinary API invocation, including status and lock. On both success and ordinary rejection, obsolete callers receive a fixed sanitized cancellation, never original path/name/value/error text. Protected status is not an exception.
- Preserve truthful completion for a known committed mutation overtaken by locking; native completed-while-locked errors are normalized to fixed receipt/error payloads. Safe ID-only warning snapshot/dismissal channels remain explicitly outside protected result fencing.
- A current-generation error remains actionable in its originating view. A stale error cannot overwrite B's newer error or claim a mutation failed unsaved. Warning/outcome delivery is not activity.
- Do not bootstrap a locked screen using obsolete status metadata. If the initial status was overtaken by a lock, expose a safe visible retry; a fresh status request can populate the current locked screen. No infinite loading or automatic stale reopen. Existing current-session lock, native cancellation, upgrade/import fencing and root-generation behavior remain required.

## Work/check sequence

1. Main freezes this plan, checks baseline ownership/fingerprints, updates the graph and adds failing-first combined-lifecycle regressions. Existing application/build/test/proof source still matches the rejected round3 pin; five guidance files differ as recorded.
2. Main owns frontend `api.ts`, `sessionBoundary.ts`, App/Settings and tests. Check actual App+Settings+API import/file-save success/error/cancel in normal/hide/navigation/return and root/lockB transitions; check no values/scopes/path leaks, exactly one ID, pending/settled acknowledgment and no false lock. Check API stale success/error/status between/after roots and cold-lock startup retry.
3. Main owns native `session.rs`, minimal clipboard-lease introspection and fake-backend tests. Exercise uninterrupted retry, dismissal before recovery, recovery then recurrence before old dismissal, new lease before old warning dismissal and not-due ticks. Keep old ownership/partial-publication tests passing.
4. Collect affected checks, then one stable full relevant policy, rebuild/hash GUI/bundles and run fake native outcome proof plus journal/export checks on that exact hash. The CLI/core persistence source is unchanged by this cycle; retain its pinned artifact/interchange evidence unless an actual dependency change invalidates it. No new clipboard or OS lock/suspend/installation test.
5. Pin full unit diff plus this cycle's diff/source identity; one fresh Terra acceptance. Another rejection returns to the owner, not an automatic retry loop. Never infer points or readiness from a scoped approval.

## Exact verification

- Editing: targeted `npx vitest run` actual App/API/Settings/lifecycle files in desktop; `cargo test -p akm-desktop --lib --offline --locked cleanup_episode` and other affected warning/clipboard fake tests. New tests must fail on the rejected source for the correct behavior, not just a fixture/config error.
- Stable: root `cargo test --workspace --offline --locked`; `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`; `cargo fmt --all -- --check`. Desktop `npm test`, `npm run build`, `npm run test:ui`, `npm run tauri -- build -- --locked --offline`. Record actual subprocess exits; do not confuse normal compiler stderr with PowerShell failure.
- Native: temporary fake vaults only; owned settings preimage/creation identity/hash and recovery recorded. Focused outcome proof may hold a test-owned journal lock to delay reply after a fake import commit, and minimize/restore only the owned AKM window before navigation. Use no global keystrokes or user clipboard. Observe both CLI saved result and app-owned acknowledgment. Native file save uses its approved dialog and no fabricated path authority. Report any unsupported/minimize behavior or automation failure rather than inventing a pass.
- Retain old failed review/build/proof outputs. Exact source/artifact/cleanup/skip provenance and a concrete next action go into STATE.md before interruption or handoff. Mocked UI, deterministic native faults and actual OS delivery remain distinct.
