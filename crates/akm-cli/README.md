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
child's exit code is preserved. A `run-start` audit event records a launch request,
not successful execution or completion. Reveal and export audit events are also
written before disclosure. An audit failure prevents disclosure or launch.
Audit warnings after a committed vault mutation are printed without turning the
successful mutation into an error.

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
