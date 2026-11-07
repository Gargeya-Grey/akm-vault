# First use with real keys

This page is for the owner, who decided on 2026-11-07 to use AKM for their own keys before the release gate. That gate is an independent full review above 90/100. The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)). This page does not change that, and the decision and its risk are the owner's.

## Before the first real key

1. **Build from the reviewed head.** Use the commit that [STATE.md](STATE.md) names as checked, and compare the build hashes with the list there. The installers are unsigned. To install one is the owner's own step.
2. **Keep every key recoverable at its provider.** Store in AKM only keys that you can revoke and create again at the provider. Then a bug in AKM costs a rotation, not an outage.
3. **Choose the vault folder.** The CLI default is `%APPDATA%\akm\vault.kdbx`. That is the Roaming profile, and a domain profile can sync it to a server (SEC-02 in [FINDINGS.md](FINDINGS.md)). A folder under `%LOCALAPPDATA%` or a folder that you choose avoids that. AKM sets no file ACL on the vault (SEC-03).
4. **Use a long, unique passphrase.** AKM has no reset or recovery.
5. **Prove the escape route.** Create the vault and add one low-value key. Then open the same file in KeePassXC 2.8 or later and find the key. If AKM fails later, KeePassXC still reads your vault.
6. **Keep a copy outside AKM.** AKM keeps five dated backups next to the vault. Also copy the vault file to a second place by hand after you add keys.

## Daily use

- `akm run --project <p> --env <e> -- <command>` gives a program its keys as environment variables, without a file. Prefer it to `export`.
- `akm export` writes plaintext. Write only into a folder that Git ignores, and delete the file when you are done.
- A copy to the clipboard is cleared after a short time and on lock. A clipboard manager or a fast reader can still keep it ([THREAT_MODEL.md](THREAT_MODEL.md), row 9).
- The journal in `%LOCALAPPDATA%\akm\journal\` records each reveal, copy, export and run. It holds key IDs and times, never a secret or a title. It is not tamper-proof evidence.
- A rotation in AKM does not change the key at the provider. Rotate at the provider first, then run `akm rotate`.

## Tests that need you present

These were never run, because the agent rules allow them only with the owner present. Run them in one session, with an empty clipboard and nothing else open in AKM. The commands are in [TESTING.md](TESTING.md).

| Test | What it proves | How |
|---|---|---|
| Real clipboard test | The Windows clipboard formats that keep a copy out of history and cloud sync | `AKM_TEST_REAL_CLIPBOARD=1`, the one ignored `cargo test` in TESTING.md |
| Full native GUI proof | Copy, clear and lock on the real window | `tools/verify_windows_app.ps1` without `-SkipClipboard` |
| Import dialog | The native dialog lists names and scope, never values; Cancel writes nothing | Import a fake `.env` file in the desktop app; press Cancel once, Import once |
| Windows notification | It shows only a count, and never after a lock | Turn on the notification in Settings with one overdue fake key; lock, then wait |
| Import and lock | The native dialog is the review (P2R2-02): an Import click in it applies the import; a lock before the click refuses it | Start an import; once press Import after you minimize and restore the window, once lock AKM (Win+L or the idle lock) before you press Import |
| R4-04 | The outcome records survive a minimize (open P1) | Choose option A or B in [FINDINGS.md](FINDINGS.md) first |

Report a failure as a new row in [FINDINGS.md](FINDINGS.md), with fake data only.
