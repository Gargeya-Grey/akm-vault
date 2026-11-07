# AKM: a Windows API-key manager

**Untrusted development build. Use fake keys only.** The initial adversarial
review scored the implementation 40.5/100, rounded to 41. See the
[baseline review](docs/reviews/2026-10-30-baseline.md). Fixing the reported bugs
does not itself establish a new score or certify security. The release gate is an
independent re-review above 90, followed by real-use testing before wider
distribution. Current status: [docs/STATE.md](docs/STATE.md).

AKM is for a Windows developer who wants a local, account-free KeePass file plus
API-key fields and process-environment injection. KeePassXC, 1Password CLI and
Bitwarden Secrets Manager already cover much of this use case. AKM is a narrower
personal workflow, not a replacement for an audited password manager or a team
secrets service. macOS is out of scope.

## Storage, and what is actually borrowed

The source of truth is a KDBX vault. AKM uses the community Rust `keepass` crate
0.17.0, pinned to the version under test. It does not implement a new vault format
or cryptographic primitives. **The format is established; that does not make this
particular integration audited.** The library labels KDBX 4.1 writing experimental.

New vaults use KDBX 4.1, AES-256, ChaCha20 for protected fields, GZip, and Argon2id
version 1.3 with 64 MiB memory, 3 iterations and parallelism 4. New passwords must
have at least 12 Unicode characters in both the CLI and desktop. Length alone is
not strength; use a long, unique passphrase. Existing vaults with shorter passwords
can still be opened.

KDBX 3.x and 4.0 files open read-only until an **explicit format upgrade** to 4.1.
Upgrade preserves the password and KDF/cipher settings and retains the original
encrypted file. It is not a password-strength upgrade. KeePassXC can write an AKM
vault back as 4.0, requiring that confirmation again. Interchange testing uses the
real KeePassXC 2.8.0; clients without 4.1 support need an upgrade.

Entries live in the top-level `API Keys` group. Entry identity is its native KDBX
UUID, not the copyable `AKM_ID` custom field. API metadata uses `AKM_ISSUER`,
`AKM_PROJECT`, `AKM_ENV`, `AKM_SCOPES`, `AKM_ENV_VAR`, `AKM_ROTATION_DAYS`, native tags
and native expiry. Other groups and unrelated custom fields are not AKM's data.

## Writes and cloud folders

The vault defaults to `%APPDATA%\akm\vault.kdbx`. Use `--vault` or `AKM_VAULT` to
choose a different file, including a fully downloaded OneDrive, Dropbox or
Syncthing folder.

- Readers open independent snapshots. The GUI and CLI can be open together.
- A sidecar lock serializes cooperating AKM writers only during a write.
- A mutation is committed in memory only after the encrypted write succeeds.
- A candidate file is decrypted and compared with the intended database before
  replacing the original. This catches integration/writer failures; using the
  same library for that check is not independent cryptographic validation.
- Fingerprints are checked before writing and again just before replacement.
  A stale snapshot must be reloaded, not silently merged.
- The original is retained in encrypted backups. Five dated backups and a
  `.bak` convenience copy are kept beside the vault.

**This is not a distributed lock or an atomic compare-and-swap with your sync
provider.** KeePassXC and cloud clients do not honor AKM's lock. An external writer
can still race the final check. Do not actively edit the same file on two devices.
On conflict, keep both copies and reconcile in KeePassXC. In the app, use
**Reload vault** before retrying a change. No automatic merge is attempted.

If a write completes while locking overtakes its reply, AKM displays a secret-free
completed-operation receipt. **Do not repeat that change.** Unlock and reload to
inspect the saved result. A lock does not retroactively cancel a committed write.

Same-folder backups are not a separate backup strategy. Sync deletion, disk
failure or ransomware can affect them all. Maintain an independent encrypted
backup. The current development build also does not provide a complete secret
version-history, Recycle Bin or rotation workflow.

## Desktop

The app provides key metadata, search, masked reveal, copy, scoped environment
export and reviewed imports. A `.env` import requires a target project and
environment. Matching is by that exact scope and variable name, never by a
variable name across the whole vault. Empty values are refused. Review the names
and replacements, then confirm; the review does not display secret values.

The native backend enforces idle locking after 10 minutes and Windows session
lock/suspend handling. Status polling does not count as activity. **Lock now** is
always available in the navigation bar while unlocked.

Revealed values and export previews hide after 30 seconds. Windows clipboard
copies request exclusion from clipboard history, cloud clipboard and monitoring.
AKM tries to clear its copy after 30 seconds, on lock and on normal exit. It
leaves a later user copy in place. The copy may remain if the clipboard is busy,
or if AKM stops unexpectedly. Another clipboard reader or a third-party clipboard
manager can also keep a copy. Exclusion flags cannot remove secrets copied by
older AKM builds from existing Win+V history.

The window requests screen-capture protection. This is defense in depth for
supported Windows capture APIs, not protection from malware, cameras or every
screen-capture path.

## CLI

Commands below assume a freshly built `akm.exe` is on PATH. Without `AKM_PASSWORD`,
the CLI prompts for the master password. There is no password argument because
command lines are observable by other processes.

```powershell
akm init --name "Personal API keys"
akm add "OpenAI dev" --project my-app --env dev --issuer OpenAI --env-var OPENAI_API_KEY
akm list
akm get "OpenAI dev"                # masked
akm get "OpenAI dev" --reveal       # full value; private screen only
akm get <entry-uuid> --field secret

akm run --project my-app --env dev -- npm test
akm export --project my-app --env dev --output .env.local
akm upgrade --yes                   # explicit older-vault format upgrade
akm audit --verify
```

A key is selected by its entry id or its exact title (any letter case). A partial word,
or a word found in a note or URL, selects nothing: `get`, `rm`, `rotate`, `history`,
`restore` and `purge` refuse it and list candidates by id, title, project and
environment, never a secret, note or URL. Two keys with the same title are refused
until you pass the id.

`run` and `export` require **both** project and environment, or an explicit `--all`
without filters. The GUI deliberately offers only scoped exports. No-match and
duplicate-variable cases fail rather than silently launching with the wrong
credentials. Reserved process-control names such as `PATH`, `NODE_OPTIONS` and
`AKM_*` cannot be injected.

`run` removes every inherited `AKM_*` variable from the child, including
`AKM_PASSWORD`, and releases the vault before launch. Normal parent environment
variables still pass through. A child can read and exfiltrate the specific keys
you intentionally supply; project scoping is least privilege, not a sandbox.
Windows `.cmd` tools are resolved explicitly. Shell/batch argument restrictions
may reject unsafe input rather than attempting to quote arbitrary shell code.

For automation, `AKM_PASSWORD` is still available. It remains sensitive in the
parent process environment even though it is removed from the child. Prefer the
prompt. Windows Hello/DPAPI quick unlock is not implemented.

Environment files are **plaintext secrets**. Use `--output` rather than Windows
PowerShell 5.1 `>` redirection, which can produce UTF-16. Keep exports out of source
control and shared/synced folders. The desktop save destination is chosen by a
native dialog, not a path supplied by JavaScript. File restrictions do not stop
the current Windows account or an administrator from reading an export.

File exports require Windows storage with persistent ACLs. The CLI verifies its
private temporary file's permissions before writing and publishes without replacing
an existing destination. Write/flush/sync/publication failures trigger owned-file
cleanup. If the CLI cannot confirm that it removed its temporary file, it says "could not confirm cleanup", names the file and exits with a failure, so you can delete it yourself. This is not secure erasure or a crash/power-loss cleanup guarantee.
Desktop exports prepare outside the lock-state gate and check authorization before
publication. A lock cancels an export that has not been authorized for publication;
an already-authorized OS operation may finish and must report its completed outcome.

## Activity journal and privacy

The journal lives on this device, in a private folder (`%LOCALAPPDATA%\akm\journal\`).
Nothing is written beside the vault, so a sync provider sees no journal. Only your
Windows account can open the folder. AKM checks that on every use and refuses to use a
folder that other accounts, links or non-local disks could reach. It does not repair
the folder and it does not fall back to another place.

A record holds the time, the action, the key id and a count or an operation id. It holds
no key name, project, environment, path or secret value. The Activity view looks up key
names from the unlocked vault. `akm audit` runs without the password and shows ids only.

The journal belongs to the device, not to the vault. AKM links a vault to its journal
with a device-local mapping. It never writes to the vault for that. A renamed vault
keeps its journal. A copy, a different vault at the same path, and a second vault start
their own journals, and AKM never merges them. When AKM cannot tell which vault it has
(for example, a locked file it has not seen before), it shows the journal read-only and
says so. An older `<vault>.audit.jsonl` file is reported and left untouched. AKM does
not read, move, delete or import it.

The hash chain detects some edits but cannot prove completeness. Truncating the
end, deleting the log or rewriting the whole chain is not authenticated. Treat it
as a convenience journal, not security evidence. A damaged journal is kept as it is,
reported by kind (`truncatedTail`, `malformedJson`, `brokenLink`, `hashMismatch`) with
no claim about the cause, and gets no new records. If a record cannot be saved
before a disclosure, an export or a launch, the action does not happen. If it cannot
be saved after a committed change, AKM prints a warning that says the change is saved
and that you must not repeat it.

Tests and proof scripts set `AKM_JOURNAL_ROOT` to a temporary absolute folder, so they
never touch the real journal.

## Build and verify on Windows

Requires Rust 1.89+, Node 22.12+, Visual Studio C++ Build Tools, the Windows SDK
and WebView2.

```powershell
cargo build --release -p akm-cli --locked
cd desktop
npm ci
npm run tauri -- build -- --locked
```

The CLI is `target\release\akm.exe`. The unsigned development installers go to
`target\release\bundle\msi` and `target\release\bundle\nsis`. Do not distribute
them as trusted software. A checksum identifies a build; it is not a publisher
signature. The full test commands, the KeePassXC interchange test and the native
GUI proof are in [docs/TESTING.md](docs/TESTING.md). Actual OS lock/suspend
delivery is not verified yet.

## Limits

- There is no password reset or recovery service.
- The decrypted vault and plaintext copies exist in process and WebView memory.
  Closing a session is not a promise to erase every allocator or JavaScript copy.
- Malware or a compromised Windows account is outside the protection offered by
  an unlocked password manager.
- Reminders reach you inside AKM: a summary after unlock, and a status on every key
  row. An optional Windows notification (Settings, off by default) says only how
  many keys need attention. AKM runs nothing in the background, so it can show one
  only while it is open and unlocked, and Windows decides whether to display it. A
  key with a rotation interval and no known secret-change time shows "Rotation
  unknown". Native notification delivery is not tested yet.
- No updater, publisher signing or external professional security audit exists yet.
- This source tree's `.gitignore` is not protection for secrets exported into a
  different repository.

## Development

Agents and contributors start with [AGENTS.md](AGENTS.md). The requirements and
contracts are in [docs/PROJECT.md](docs/PROJECT.md), open findings in
[docs/FINDINGS.md](docs/FINDINGS.md) and the remaining work in
[docs/ROADMAP.md](docs/ROADMAP.md).
