# AKM CLI

The master password is read from a hidden terminal prompt or `AKM_PASSWORD`.
There is no `--password` argument. A prompt is safer because other local processes
may inspect an environment. New vaults require at least 12 password characters.
Existing vaults with shorter passwords can still be opened.

## Select keys explicitly

Both `run` and `export` require a non-empty project and environment, or `--all`
without either filter. A selection with no matching keys fails without launching
a child or disclosing secrets.

```text
akm run --project demo --env dev -- npm run dev
akm export --project demo --env dev --output keys.env
akm export --all --output all-keys.env
```

Children inherit the normal tool environment, except every `AKM_*` variable is
removed case-insensitively. Vault keys cannot reintroduce these names or override
reserved process-control variables such as `PATH`, `PATHEXT`, or `COMSPEC`.
Windows bare commands are resolved using `PATH` and `PATHEXT`. The working
directory is not searched implicitly. Use an explicit relative path or a PATH
entry to run a project-local executable. Batch files use Rust's escaped argument
handling rather than a hand-built shell command.

The vault and master-password buffer are dropped before a child starts. The
child's exit code is preserved.

The activity log records these actions:
- `run-requested`, before the child starts. It records a launch request, not
  successful execution or completion.
- `reveal-requested`, before `get --reveal` or `get --field secret` shows a secret.
- `export-requested`, before any export byte is written.
- `export-completed`, after the export is written. If this record cannot be
  confirmed, the export still succeeded: AKM prints a warning and exits 0. Do not
  repeat the export.

A failure to record a request prevents the disclosure or the launch.

The default `get` view shows `Secret:      ********` for every secret. It shows no
character and no length of the secret.

Audit warnings after a committed vault mutation are printed without turning the
successful mutation into an error. The warning says that the change is saved and that
you must not repeat it.

The journal is a private per-device folder, `%LOCALAPPDATA%\akm\journal\`. It is never
beside the vault. `AKM_JOURNAL_ROOT` replaces the folder with an absolute path. Only tests
and proof scripts set it. A value that is not absolute makes the journal unavailable. The
journal does not fall back to the default folder or to the old sidecar. `akm audit` and
`akm audit --verify` run without a password, create nothing, and show key ids only. An old
`<vault>.audit.jsonl` is reported and never read, moved or deleted. A damaged journal is
reported by kind and keeps its bytes.

## Export files

`--output` writes UTF-8 directly, without a BOM or PowerShell redirection. The
destination must be a new local file. Existing files are never overwritten.
UNC paths, Windows device paths, reserved device names, and alternate data streams
are refused.

File export is Windows-only. Files are created with a protected, non-inherited
owner-rights DACL. Capability queries and permission read-back must succeed before
writing plaintext. Unsupported storage or ACL setup failures fail closed rather
than falling back to inherited permissions. Stdout export is unchanged.
Exported files contain plaintext secrets and must be protected and removed when
no longer needed.

## Explicit format upgrades

Older KDBX formats remain readable. Writes that require KDBX 4.1 fail with an
instruction to run `akm upgrade`. Confirm interactively, or use `akm upgrade --yes`
for automation. Reading or writing a vault never silently confirms an upgrade.
