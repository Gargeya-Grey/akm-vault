# AKM: a Windows API-key manager

**Untrusted development build. Use fake keys only.** The initial adversarial
review scored the implementation 40.5/100, rounded to 41. See [REVIEW.md](REVIEW.md).
Fixing the reported bugs does not itself establish a new score or certify security.
The owner's release gate is an independent re-review above 90, followed by real-use
testing before wider distribution.

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
AKM clears its current copy after 30 seconds, on lock and on normal exit, without
deliberately clearing a later user copy. A crash, forced termination, another
clipboard reader or a third-party clipboard manager can defeat cleanup. Exclusion
flags cannot remove secrets copied by older AKM builds from existing Win+V history.

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
cleanup. This is not secure erasure or a crash/power-loss cleanup guarantee.
Desktop exports prepare outside the lock-state gate and check authorization before
publication. A lock cancels an export that has not been authorized for publication;
an already-authorized OS operation may finish and must report its completed outcome.

## Activity journal and privacy

The current journal is plaintext `<vault>.audit.jsonl`, beside the encrypted vault.
It contains entry titles, projects, environments, timing and action metadata.
**It syncs if that folder syncs, and exposes that metadata to the provider.** It
does not intentionally contain secret values.

The hash chain detects some edits but cannot prove completeness. Truncating the
end, deleting the log or rewriting the whole chain is not authenticated. Treat it
as a convenience journal, not security evidence. Moving to a private, per-device
journal is phase-2 work. A journal failure after a successful vault commit must
produce a warning, not an error that encourages retrying an already-saved change.

## Build and verify on Windows

Requires Rust 1.89+, Node 22.12+ (or a newer supported Node), Visual Studio C++
Build Tools, Windows SDK and WebView2. Microsoft Edge is used for headless UI
regressions; those mock the native boundary and do not replace native tests.

```powershell
cd desktop
npm ci
npm test
npm run build
npm run test:ui
cd ..
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo fmt --all -- --check
cargo build --release -p akm-cli --offline
cd desktop
npm run tauri -- build -- --locked --offline
```

Omit `--offline` on the first dependency fetch if needed. Windows installers go
to `target\release\bundle\msi` and `target\release\bundle\nsis`. They are unsigned
development artifacts. Do not disable Windows security checks or distribute them
as trusted software. Checksums identify a build; they are not publisher signatures.

Run the repeatable independent-reader regression with a portable KeePassXC:

```powershell
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc "C:/path/to/keepassxc-cli.exe"
```

It creates only temporary vaults and fake keys. Windows CI also downloads the
pinned KeePassXC 2.8.0 portable build, checks its recorded SHA-256 and runs this
test. The workflow tests the desktop crate as well as core, CLI and frontend.
It uploads development artifacts, not a public release.

`tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe
target/release/akm.exe -KeePassXC C:/path/to/keepassxc-cli.exe` drives only its own
test process with fake vaults. The optional KeePassXC argument adds a cloned-entry
GUI-edit fixture. It also tests the native save dialog, rejected imports and explicit
upgrade confirmation. Synthetic lock/suspend messages are not proof of actual
Windows lock or power-event delivery.

The GUI proof refuses to overwrite a non-empty clipboard. Add `-SkipClipboard`
to preserve its contents while running the other flows; that explicitly skips
interactive clipboard-copy and cleanup verification.

The proof prints its scratch directory and saves `recovery.json` plus the prior
settings bytes before opening AKM. If the harness is interrupted, preserve that
directory; check its recorded process/vault ownership before restoring settings.
Do not guess a previous vault preference or delete another process's files.

For an interrupted run whose earlier checks remain valid, `-RemainingOnly` creates
a fresh fake CLI fixture and runs only native export cancellation/save, clipboard,
synthetic session/power, normal exit and capture-affinity checks. It does not rerun
GUI creation, rejected/scoped imports, cloned-entry edits or format upgrades and
is not a substitute for the full proof on materially changed code. The proof
refuses another open AKM process and records native startup diagnostics on failure.

The final correction review and resumed native proof passed; see the final
checkpoint and artifact hashes in [PHASE1.md](PHASE1.md). Actual OS lock/suspend
delivery remains unverified. This is still a fake-keys-only development build,
not an above-90 readiness verdict.

## Limits

- There is no password reset or recovery service.
- The decrypted vault and plaintext copies exist in process and WebView memory.
  Closing a session is not a promise to erase every allocator or JavaScript copy.
- Malware or a compromised Windows account is outside the protection offered by
  an unlocked password manager.
- Rotation indicators and reminders are still development features. Metadata edits
  currently affect the rotation clock, and reminders do not reach you outside AKM.
- No updater, publisher signing or external professional security audit exists yet.
- This source tree's `.gitignore` is not protection for secrets exported into a
  different repository.

The phase-1 implementation and verification contract is in [PHASE1.md](PHASE1.md).

## Project context and continuation

For an agent resuming after compaction or a restart, begin with [AGENTS.md](AGENTS.md)
and [docs/STATE.md](docs/STATE.md). Stable requirements and contracts are in
[docs/PROJECT.md](docs/PROJECT.md). The [review ledger](docs/REVIEW_LEDGER.md) separates
closed scenarios from remaining work, and [docs/history](docs/history/) preserves
the surviving reviews and evidence with hashes. [PHASE2.md](PHASE2.md) contains the
recalibrated draft plan. No old projected score is a current readiness verdict.
