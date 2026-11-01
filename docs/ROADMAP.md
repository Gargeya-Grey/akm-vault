# Roadmap

Phase two fixes the remaining privacy, history and compatibility problems without weakening the [project contracts](PROJECT.md#contracts-every-change-keeps). Finishing this list does not guarantee a score. Only a new full review measures one.

**Authorization.** The owner has authorized implementation of unit 1a only. Each other unit needs the owner's explicit authorization in chat before implementation starts. Work units run in this order. Units that share `model.rs`, `vault.rs`, the IPC types or the CLI command definitions run one after another, never in parallel. Do not redo the closed findings in [FINDINGS.md](FINDINGS.md).

## Unit 1a: warning ownership and honest journal outcomes (in progress)

- A warning belongs to the operation and the session that caused it. A post-commit warning from session A never appears in session B. This holds for both ordinary completion and a commit that a lock overtakes. A keeps its secret-free acknowledgment. App-wide warnings (policy, clipboard) are kept apart from vault-session warnings.
- The journal shows these states: absent, damaged, consistent but unauthenticated, and incomplete or unknown. A missing log does not prove that nothing happened.
- A requested action and its known outcome are recorded separately. Verify and display use one locked snapshot.
- Logging before a disclosure, export or launch still fails closed. A logging failure after a commit or publication is a warning with a truthful, non-retryable result.

Status: [STATE.md](STATE.md) and [FINDINGS.md](FINDINGS.md).

## Unit 1b: private per-device journal

Write these decisions down first. Implementation starts only after the owner approves them and authorizes the unit:

- **Location and vault identity.** Use a private non-roaming folder. Specify rename, clone, replacement, rollback, two separate vaults, unclear identity, and `akm audit` without a password. A path alone, a content hash that changes on save, or a root UUID that clones copy is not enough by itself. Never write to an older-format or read-only vault just to give it an ID, and never merge clone histories.
- **What to record.** Record the minimum. Moving titles, scopes and export paths to another plaintext file is not privacy.
- **Permissions.** Create the journal, its mapping and its lock securely, and verify the Windows ACLs. Refuse reparse points, denied permissions and unsupported storage. Never fall back to the synced sidecar.
- **Legacy sidecar.** Detect the old `<vault>.audit.jsonl` and explain that it exposed metadata. Never delete, upload or import it silently. Moving the log does not remove copies that already synced.
- **Damage.** Keep the original bytes. Report truncation, malformed JSON, damaged records and broken links without claiming a cause. Never accept an altered prefix as complete history.
- **Integrity claim.** A local hash chain with no independent anchor cannot prove completeness. Say so. Do not write the vault on each read to anchor it.

Acceptance: private location and permission refusal, no plaintext metadata beside a fresh synced vault, the identity cases above, concurrent mapping, append and read, preserved damaged bytes, older-format and read-only vaults, the legacy log left untouched, and both failure policies. Test through the real CLI and Activity callers.

## Unit 2: rotation, native history and recoverable delete

- Add an explicit secret-change time. Metadata edits, unchanged imports and no-op operations do not reset it. An old entry with no known rotation time shows "unknown", not an invented date.
- Specify the rotate action: unchanged values, timestamps, and an exactly-once result across lock and cancel. Keep secrets out of listings and receipts.
- Use native KDBX history and the Recycle Bin and deletion records that the library and KeePassXC support. Do not invent a history format.
- Keep unrelated history, attachments, foreign groups, custom fields and KDF and cipher settings.
- Retention and purge are separate explicit actions. Nothing purges automatically.
- The pinned `keepass` 0.17.0 adds history on a tracked no-op edit, updates the location time on a tracked move, and stores times to the second. Decide no-op detection, same-second edits, clock rollback, restore parent and UUID, a missing Recycle Bin, and tombstone timing before you build.

Acceptance: notes-only edit, unchanged import, no-op and fast edits, a real secret change, clock rollback, a failed save, a late committed receipt, a missing legacy timestamp, history, recycle, permanent delete and restore. Also test stale-copy merges in both directions with real KeePassXC, with foreign history and shared attachments. Use controlled clocks and literal expected values.

The master-password change (M2) needs its own plan: old backups keep the old password, and crash and recovery need design.

## Unit 3: dotenv, time and CLI selection

- Write an external dotenv profile and pin reference fixtures: comments, quotes, escapes, BOM, CRLF, multiline values and malformed input. Never execute or interpolate file content. Keep the duplicate-name, empty-secret, scope and reserved-name protections.
- Choose one instant for a date-only expiry and use it in the CLI, GUI, exports and reminders. Test time-zone boundaries. Keep display rounding apart from rotation scheduling.
- Fix P2-TIME-REFRESH. Status must update while Keys stays open, after hide and resume, and after a clock change. A refresh does not extend the idle deadline.
- Decide whether the CLI fallback search through notes and URL is safe for disclosure and delete commands.
- Remove the secret-bearing `Debug` and the secret loading for metadata-only callers. Do not claim full memory zeroization.

Acceptance: external parser fixtures as well as round trips, controlled clocks at exact expiry and refresh boundaries, executable CLI regressions, ambiguous selectors, and assertions on metadata and `Debug` output.

## Unit 4: reminders and accessibility

- Specify the unlock summary and the opt-in Windows notification content. A notification on the lock screen shows no secret and no sensitive metadata. Do not add an always-running service as a hidden prerequisite.
- Check focus containment and return for every modal, including import. On the Keys screen, each key row's accessible name or description must announce its expiry and rotation status.
- Fix the Keys-screen layout debt, or list it as kept debt. Functional fixes and visual changes go in separate pull requests.

Acceptance: expired, overdue, unknown-date and no-key states; locked and hidden transitions; denied or disabled notifications; accessible status; and keyboard-only paths. Mocked UI tests do not prove native notification delivery.

## Unit 5: second reader and full rescore

- Keep KeePassXC coverage. Add a pinned kdbxweb reader and writer suite where the format allows.
- Run the cross-reader fixtures for history, delete, attachments, the journal and scoped env on the exact rebuilt build.
- Run a new full review over all nine areas of the baseline rubric, with independent security, architecture and premise, and correctness reviewers. Count each root cause once. State how many reviewers ran.

## Release and open-source work

The owner decides when this starts. It includes:

- a license, `SECURITY.md` and `CONTRIBUTING.md`;
- a review of the whole Git history for secrets and personal data before any public push, or a fresh public history;
- turning on CI and recording a real remote run;
- publisher signing, an updater, and install, upgrade and uninstall tests;
- Windows Hello or DPAPI quick unlock;
- real OS lock, suspend and idle tests with the owner present;
- a professional security review.
