# Export and run

A user exports the keys of one project and environment as a dotenv file, or starts a program with those keys as environment variables. Nothing runs or exports without an explicit scope or `--all`.

## Sub-features

- `export-stdout` prints a scoped dotenv to stdout.
- `export-file` writes a new private file with `--output` and never overwrites one.
- `run-inject` starts a child with the scoped variables and without any `AKM_*` variable.
- `scope-required` refuses an export or run with no scope and no `--all`.
- `reserved-names` refuses keys whose variable name is reserved (for example `PATH`, `PYTHONSTARTUP`, `HTTPS_PROXY`).

## How to get to it (user POV)

- CLI: `akm export --project <p> --env <e> [--output <file>]`, `akm run --project <p> --env <e> -- <command>`.
- Desktop: Settings, Export (native save dialog). Covered by `-JournalExportOnly`.

## Driving it with akm-verify

Preconditions:

- A session with the key from [vault-and-keys](./vault-and-keys.md) (project `demo`, environment `dev`, variable `OPENAI_API_KEY`).

- **No scope.** `& $h cli -Session $s -- export`. Non-zero exit, a scope message, no `fake-` value.
- **Export to stdout.** `& $h cli -Session $s -- export --project demo --env dev`. Exit 0, stdout `OPENAI_API_KEY=fake-secret-0123456789`.
- **Export to a file.** `& $h cli -Session $s -- export --project demo --env dev --output <session>\out.env`. Exit 0. Run it again: non-zero, the file is unchanged.
- **Run.** `& $h cli -Session $s -- run --project demo --env dev -- cmd /c "if defined OPENAI_API_KEY (echo has-key) & if defined AKM_PASSWORD (echo leak)"`. Exit 0, stdout `has-key`, no `leak`.
- **Reserved name.** `& $h cli -Session $s -Stdin 'fake-x' -- add 'Fake path' --project demo --env dev --env-var PYTHONSTARTUP --secret-stdin`. Non-zero exit, and the message names the reserved class.
- **Desktop.** `tools/verify_windows_app.ps1 ... -JournalExportOnly` (no owner needed).
- **Proof.** The transcript; `audit` shows an export request before its completion.

## Gotchas

- `--output` refuses a path outside permitted local storage (for example a network share) before it writes a byte.
- Remove `out.env` before `clean` only if the run needs the folder again; `clean` removes it with the session.
- `run` output is not masked. A transcript of `run` can hold a fake secret if the child prints it; the recipe above does not print it.
