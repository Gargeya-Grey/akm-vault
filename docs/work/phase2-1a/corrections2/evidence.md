# Second correction cycle

Round two rejected the 153/43/10 candidate. Its raw report/probes and frozen inputs remain under `../reviews/round2/`; that verdict has not been superseded by an approval.

## Failing-first evidence

- `red-whitespace.log`: the core byte-boundary regression failed on an unterminated tab. The accepted correction contract intentionally refuses every nonempty unterminated fragment, even valid JSON whitespace, rather than repairing it.
- `red-cli.log` and `red-native.log`: actual CLI/native request callers accepted unsafe skipped-whitespace fragments. The reviewer independently reproduced form-feed on the previous release CLI.
- `red-precision.log`: native DTO serialization emitted numbers instead of exact decimal strings.
- `red-frontend.log`: five failures plus the expected unknown-action render exception reproduced both completion-handoff cases, one-shot application-warning loss, inherited action lookup and the delete confirmation's unconditional backup promise. The already-string-based UI precision cases passed; the failing DTO test covers their native input conversion.
- The interrupted multi-file implementation patch failed its context check without applying changes. Source inspection on resumption confirmed production code was unchanged; the red tests/logs were retained, not rerun merely to regain context.

## Implemented and targeted checks

- Append/read now accepts blank lines only with JSON whitespace and a terminating newline. The core regression exercises all 256 single-byte values, terminated/unterminated, before and after a valid journal (1,024 cases), preserving every refused input byte.
- Completion receipts have frontend in-memory IDs and origin generations, replay across React roots without re-emitting a lock, and explicit acknowledgment. Root teardown invalidates old API work. Tests cover native `completedWhileLocked` and stale successful replies, both between and after remount, with no protected path display or false lock of B. No crash-durable receipt persistence is claimed.
- App-global warnings now have native IDs, non-destructive snapshots and explicit ID dismissal. They remain separate from origin-specific operation notices. Native and App/API tests cover remount, failed dismissal, stale poll replay and a newer recurrence with the same message but new ID.
- Activity uses an own-key action lookup. Literal unknown actions `__proto__`, `constructor`, `toString` survive core snapshots and render safely.
- Delete success/confirmation describe the retained dated encrypted original, not a guaranteed `.bak` refresh. Actual App/KeysView/API regression displays deletion success plus a separate backup-refresh warning.
- Native audit sequence serializers emit decimal strings (journal-on-disk format stays numeric and unchanged). DTO/UI tests cover `2^53-1`, `2^53`, `2^53+1`, `u64::MAX` and absent sequence. This changes IPC, not the persisted journal format.
- `targeted-checks.json` records all affected core/CLI/native targeted checks exit0. `targeted-frontend.log` records 39 passes in five affected files. No earlier full suite is counted as proof of these new changes.

## Stable corrected candidate

- Full frontend: 55 tests passed. Production build initially caught two test-only Testing Library `ByRoleOptions` fields copied from the Playwright convention (`exact`); removed the unsupported options without changing application code. Affected delete test passed again, then production build and browser10 passed. These overlap, not 56 unique frontend tests. Both failed build and corrected result remain in `frontend-checks.json` / logs.
- Full Rust workspace **157 passed**, zero failed, two gated ignored tests; strict Clippy and rustfmt exit0 (`rust-checks.json`, `suite-summary.json`). These were renewed once for this stable cross-cutting candidate.
- Release CLI/GUI/MSI/NSIS builds exit0 (`build-exits.json`). GUI SHA-256 `42f3f63afae5c6dfa426c92da7635972d5e0642c2d6e8482681588dfc14510c8`; CLI `0024f9005a68bf5b5ab9d32297c9d6eb4043ee5616a7ae6c465a054adfddcdc0`; bundle hashes in `artifacts-sha256.json`. Nothing installed.
- Extended exact-GUI `-JournalOnly` passed: journal states/byte preservation, delete success with a real `.bak` refresh failure and a retained dated encrypted original, literal unknown action labels from valid journal records, exact large sequence through real IPC, saved-write confirmation, A acknowledgment across B, explicit dismissal/subsequent polls and normal exit. Startup required the already-recorded owned UI Automation root refresh; no new application build/fix was required.
- Exact-GUI `-JournalExportOnly` passed first attempt in this cycle: approved native destination, scoped fake content, requested/completed ordering, unchanged encrypted vault, refusal before publication when request logging fails, and normal exit. Both proofs exit0 and restored settings byte-for-byte (`native-proofs.json`). Clipboard skipped entirely.
- Final CLI real KeePassXC interchange passed (`keepassxc.log`, `keepassxc-exit.json`). No library audit is claimed.
- Cleanup observation: no AKM/matching WebView processes remained (`cleanup.json`). Prior failed startup evidence from the preceding cycle remains retained, not overwritten by these passes.
- Layout/screenshot checks renewed with a separate app-global warning visible. Graph no findings; unchanged Activity5 and Keys/notice21 finding lines per theme remain reported. No new notice overlap/overflow was reported. Screenshots at390/1440 were inspected; this is not a layout-checker pass.

Fresh bounded acceptance **failed**, with one P2 and two P3 findings in the [third raw report](../reviews/round3/review-result.md). Seven reviewer records are archived, including the initial shell-capture output and clean final probe output. Reviewer verified all pinned application/build/test/proof inputs and artifacts. No implementation followed; the cycle is returned to the owner for a decision.

Original failed logs remain. Completion handoff tests are mocked native IPC; browser reload tests are retained mocked-native stores, not actual Tauri reload. Late-lock/disk-sync faults remain deterministic injections. No real clipboard, actual OS lock/suspend, wall-clock idle, installation, publication or real-secret testing. Passing suite counts do not override the rejected acceptance verdict or authorize another correction cycle.

## Final bounded-cycle handoff

Tier2 because this unit crosses core, CLI, native IPC and React lifecycle. Graph: main-session Sol implementation/collection in sequence, three separate fresh read-only Terra contexts. No implementation delegates, git repository, PR, commit, installation or publication. Graph and ledger retain the rejected gates. All application source remains as reviewed; this handoff changes status/evidence only.

- Round1 reviewer `ses_e6f5c4454ffepv9w2kH46BB2x0`: requested `openai/gpt-6.1-terra#max`; first-run acceptance **no**, seven findings; actual effort/tokens unavailable.
- Round2 reviewer `ses_e6f30bf5fffeLlzRq56ccVDfTX`: requested `openai/gpt-6.1-terra#max`; first-run acceptance **no**, six findings; actual effort/tokens unavailable.
- Round3 reviewer `ses_e6edc74a6ffesMbDh6Y7lfBTJm`: requested `openai/gpt-6.1-terra#max`; first-run acceptance **no**, three findings; actual effort/tokens unavailable.

The first thirteen findings have implementation/regression evidence, but whole-unit acceptance remains rejected by the three latest findings; no earlier raw failure was replaced with approval. The owner's above90 full-assessment gate remains unchanged and unfulfilled. Next action is the owner's decision about another bounded correction cycle, not another automatic edit or test rerun.

Handoff checks compared all145 pinned paths and confirmed no application/build/test/proof input changed; they verified28 raw review/probe records, all139 baseline files, four artifact hashes and59 local links. Only the five expected guidance/state documents differ from the pin. The local-only metrics report records overall failure, not suite failure. Handoff graph initially overflowed at900px with a fifth gate; combining the review/owner gate preserved its decision while restoring light/dark layout checks. Initial measurement logs are retained; no application code changed.
