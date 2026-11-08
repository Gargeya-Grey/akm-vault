# Journal and Activity

AKM records each disclosure, export, run and saved change in a private per-device journal, never beside the vault. The CLI shows it with `audit`, and the desktop shows it in Activity with an honest state: consistent but unauthenticated, damaged (by kind), missing, or unclear identity.

## Sub-features

- `journal-private` keeps the journal under the journal root, with a private ACL; nothing appears beside the vault.
- `journal-audit` lists records with key ids and counts only, and needs no password.
- `journal-verify` checks the hash chain and names damage without claiming a cause.
- `journal-fail-closed` refuses a disclosure, export or run when the record cannot be written.
- `activity-view` shows the journal state and the records in the desktop app.

## How to get to it (user POV)

- CLI: `akm audit`, `akm audit --verify`, `akm audit --limit <n>`.
- Desktop: the Activity screen and its `Refresh` button.

## Driving it with akm-verify

Preconditions:

- A session with a vault and one key, made as in [vault-and-keys](./vault-and-keys.md).

- **Record.** Reveal the key: `& $h cli -Session $s -- get 'Fake OpenAI' --field secret`. Exit 0.
- **Audit.** `& $h cli -Session $s -- audit`. Exit 0; records show actions and key ids, never the title `Fake OpenAI`, a project name or a secret.
- **Verify.** `& $h cli -Session $s -- audit --verify`. Exit 0 and a consistent-chain message that does not claim the history is complete.
- **Fail closed.** Make the journal folder unusable: replace `<session>\journal` with a plain file (`Remove-Item <session>\journal -Recurse; Set-Content <session>\journal x`). Then `& $h cli -Session $s -- get 'Fake OpenAI' --field secret` exits non-zero and prints no `fake-` value.
- **Activity view.** `tools/verify_windows_app.ps1 ... -JournalOnly` (no owner needed). Its `PASS:` lines cover the consistent, damaged, missing and second-vault states.
- **Proof.** The transcript, and the `-JournalOnly` output in the evidence folder.

## Gotchas

- The journal cannot detect a deletion at its end; no check can prove completeness. Do not report "complete history".
- A legacy `<vault>.audit.jsonl` beside an old vault is reported and never changed. Check its bytes are unchanged.
- The `-JournalOnly` proof writes its own fake journal records with the unit 1b hash chain; if the chain changes, update the fixture in the script.
