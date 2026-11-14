# AKM CLI

The master password is read from a hidden terminal prompt or `AKM_PASSWORD`.
There is no `--password` argument. A prompt is safer because other local processes
may inspect an environment. New vaults require at least 12 password characters.
Existing vaults with shorter passwords can still be opened.

`add` and `rotate` read a key's secret from a hidden prompt, or from stdin with
`--secret-stdin`. There is no `--secret` argument: the process list and the shell
history keep a command line.

```text
akm add "OpenAI dev" --project demo --env dev    # prompts for the secret
echo fake-value | akm add "CI key" --project demo --env ci --secret-stdin
```

## Edit a key's metadata

```text
akm edit "OpenAI dev" --project other --env-var OPENAI_KEY
akm edit "OpenAI dev" --scope read --scope write    # replaces the stored scopes
akm edit "OpenAI dev" --clear notes --clear expires
```

- `edit` changes metadata only. Each option replaces one field: `--title`, `--project`,
  `--env`, `--env-var`, `--issuer`, `--username`, `--url`, `--notes`, `--expires` and
  `--rotation-days`. `--scope` and `--tag` can repeat, and the values given replace the stored
  list. `--clear <field>` empties an optional field and can repeat. `--clear env-var` gives the
  key the variable name that AKM derives from its title. A title cannot be cleared.
- A field that the command does not name stays as stored. The id, the secret, the time of the
  last secret change and the earlier versions stay too. `rotate` changes the secret.
- `edit` selects the key by id or exact title, as `get` does. It checks each value as `add`
  does: the variable name, the expiry and the rotation interval. A refused edit saves nothing.
- An edit that changes nothing saves nothing, writes no journal record and says `No change`.
  Any other edit keeps the earlier version in the key's history, records `update-completed`
  with the key id in the journal, as a desktop edit does, and prints the key masked, as `get`
  does.
- A KDBX tag holds no `;`, `,` or tab, and no space at either end, because KeePass splits and
  trims tags when it reads them. `add` and `edit` refuse such a tag before they unlock the
  vault.

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

## Rotation, history and the Recycle Bin

Design: `docs/design/unit2.md`.

```text
akm rotate "OpenAI"                    # prompts for the new secret
echo new-value | akm rotate "OpenAI" --secret-stdin
akm history "OpenAI"                   # earlier versions: times and field names, no values
akm rm "OpenAI" --yes                  # moves the key to the Recycle Bin
akm bin                                # lists recycled keys
akm restore "OpenAI"                   # same id, secret, history and group
akm purge "OpenAI" --yes               # deletes a recycled key for good
akm purge --older-than-days 90 --yes   # the retention sweep, only when you run it
```

- `akm get` shows `Secret changed:` with the time of the last real secret change, or
  `unknown`. A key that an older AKM wrote has no such time and shows `unknown`; AKM never
  invents a date. Only `rotate`, an edit that changes the secret and an import that changes
  it set the time. An edit of notes or other metadata, an unchanged import and an edit that
  changes nothing do not. The rotation age in `list` and `get` counts from this time and
  shows `unknown` without it.
- `rotate` takes the new secret from a hidden prompt or from stdin, never from an argument.
  It refuses an empty secret and a secret equal to the current one, so repeating a rotation
  whose reply you lost cannot rotate twice. The old secret stays in the vault history. Rotating
  here does not change the secret at the provider.
- `rm` never deletes for good. The vault gets a KeePassXC-style Recycle Bin when it has none.
  A vault whose Recycle Bin is turned off refuses `rm`; turn it on in KeePassXC or use
  `purge --live`.
- `restore` refuses a key whose variable name a live key now uses in the same project and
  environment.
- `purge` is the only way to delete for good, and it needs `--yes` without a terminal. It stores the
  KDBX deletion record. Other readers apply it, but KeePassXC 2.8.0 `merge` does not: it brings a
  deleted key back from an older copy, also when KeePassXC made the deletion. Older copies and
  backups can still hold the key. Nothing purges by itself.
- The journal records `rotate-completed`, `restore-completed`, `purge-completed` and
  `remove-completed` with the key id only.
- A debug build of `akm` reads `AKM_TEST_CLOCK` (an RFC 3339 time) as its clock. Only tests set it.
  A release build ignores it.

## Export files

`--output` writes UTF-8 directly, without a BOM or PowerShell redirection. The
destination must be a new local file. Existing files are never overwritten.
UNC paths, Windows device paths, reserved device names, and alternate data streams
are refused.

File export is Windows-only. Files are created with a protected, non-inherited
owner-rights DACL. Capability queries and permission read-back must succeed before
writing plaintext. AKM reads the location of its open file from the file handle, before
the first plaintext byte and again after publication. If the destination folder was
moved or replaced after the check (for example by a junction), the export stops and
AKM removes its file through the handle. Unsupported storage or ACL setup failures fail closed rather
than falling back to inherited permissions. Stdout export is unchanged.
Exported files contain plaintext secrets and must be protected and removed when
no longer needed.

## Explicit format upgrades

Older KDBX formats remain readable. Writes that require KDBX 4.1 fail with an
instruction to run `akm upgrade`. Confirm interactively, or use `akm upgrade --yes`
for automation. Reading or writing a vault never silently confirms an upgrade.
