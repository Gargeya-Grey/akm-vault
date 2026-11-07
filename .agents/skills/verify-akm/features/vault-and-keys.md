# Vault and keys

A user creates an encrypted vault, adds API keys with their metadata, lists them without secrets, shows one key masked, and reveals its secret only on request, with a journal record first.

## Sub-features

- `vault-init` creates a KDBX 4.1 vault that needs a master password of 12 or more characters.
- `key-add` stores a key with project, environment, variable name, expiry and rotation interval.
- `key-list` lists keys, never their secrets, as text or `--json`.
- `key-get` shows one key with a fixed mask and no secret characters.
- `key-reveal` prints the secret with `--reveal` or `--field secret`, after a journal record.

## How to get to it (user POV)

- CLI: `akm init`, `akm add <title>`, `akm list`, `akm get <id or exact title>`, `akm get <id> --reveal`.
- Desktop: Unlock screen (create or open a vault), the Keys screen, a key's detail view and its Reveal button. The GUI path is covered by the full GUI proof, which needs the owner (it uses the clipboard).

## Driving it with akm-verify

Preconditions:

- A new session: `$s = & $h new -Feature vault-and-keys -Akm <akm.exe>`.

- **Create.** `& $h cli -Session $s -- init --name 'Fake vault'`. Exit 0. The session folder then holds `vault.kdbx`.
- **Add.** `& $h cli -Session $s -Stdin 'fake-secret-0123456789' -- add 'Fake OpenAI' --project demo --env dev --env-var OPENAI_API_KEY --expires 2027-03-15 --rotation-days 90 --secret-stdin`. Exit 0.
- **List.** `& $h cli -Session $s -- list --json`. Exit 0. One object with title `Fake OpenAI`, project `demo`, environment `dev`; stdout holds no `fake-secret`.
- **Show masked.** `& $h cli -Session $s -- get 'Fake OpenAI'`. Exit 0. stdout holds no part of `fake-secret-0123456789`.
- **Substring refused.** `& $h cli -Session $s -- get Fake`. Non-zero exit, a refusal that lists candidates by id and title, and no secret.
- **Reveal.** `& $h cli -Session $s -- get 'Fake OpenAI' --field secret`. Exit 0, stdout `fake-secret-0123456789`.
- **Journal first.** `& $h cli -Session $s -- audit --verify`. Exit 0; the newest record is a disclosure request for the key's id, with no title and no secret.
- **Proof.** The transcript holds all of the above. `Get-ChildItem <session>` shows only `vault.kdbx`, its `.bak` and dated backups, `vault.kdbx.lock`, `journal\` and `session.json`: no plaintext file beside the vault.

## Gotchas

- `--expires 2027-03-15` means 23:59:59 UTC on that day. Compare with that instant, not midnight.
- `init` refuses a password shorter than 12 characters; the helper's fake password is long enough.
- A run without `AKM_JOURNAL_ROOT` would write the real profile folder. Use the helper, never the bare binary.
