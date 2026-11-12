# AKM verification map

This folder is the maintained source for proving AKM's user-facing behaviour. Read the index, then use the matching feature file as the recipe.

## Baseline preconditions

- A release build of `akm.exe` (and `akm-desktop.exe` for GUI features) from the commit under test. Set `CARGO_TARGET_DIR` to a drive with about 10 GB free when the repository's drive has less (SKILL.md, Launch).
- `./.agents/skills/verify-akm/scripts/doctor.ps1` prints no `STOP` line.
- Every CLI command goes through `scripts/akm-verify.ps1 cli`, which supplies a fresh fake vault, a fake password and a private test journal root.
- Fake values only. A fake secret starts with `fake-`, so a transcript search for `fake-` finds every leak.

## Driving conventions

- Start each recipe from a new session (`akm-verify.ps1 new`).
- Select a key by its id or its exact title. A substring is refused on purpose.
- GUI recipes run only through `tools/verify_windows_app.ps1` and its focused modes.
- Mark each step that needs the owner. Do not run it without the owner's yes in chat.

## Proof and skip reporting

- CLI proof: the transcript holds each command, stdout, stderr and the exit code, and a second read-only command shows the stored result.
- GUI proof: the `PASS:` lines of the proof script, its `SKIPPED:` line, and the settings hash before and after.
- Report an entry point that you did not drive as not verified. Never report it as verified through another entry point.

## Features

- [Vault and keys](./vault-and-keys.md): create a vault, add, list, show masked, reveal.
- [Journal and Activity](./journal-and-activity.md): the private journal, `audit`, `audit --verify`, the Activity view.
- [Export and run](./export-and-run.md): scoped dotenv export to stdout or a file, and `run` with injected variables.
- [Rotation and Recycle Bin](./rotation-and-recycle-bin.md): `rotate`, `history`, `rm`, `bin`, `restore`, `purge`.
- [Import and clipboard](./import-and-clipboard.md): the desktop `.env` import with its native confirmation, and private clipboard copy. Owner present.
- [Terminal and history](./terminal-and-history.md): the desktop terminal that holds one project's keys, and a key's earlier versions. The real flow needs the owner present.
