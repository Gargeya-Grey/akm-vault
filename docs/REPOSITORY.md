# Repository and evidence policy

The owner authorized local Git followed by a **private** GitHub repository under `Gargeya-Grey` on 2026-11-01. The repository is [Gargeya-Grey/akm-vault](https://github.com/Gargeya-Grey/akm-vault), on branch `main`. The initial baseline commit is `b187e55f9c83ca96488a1bd9db8af49bd0feba7a`. This does not authorize public visibility, releases, installation, real secrets or a new native minimize test.

## What Git tracks

- Application source, dependency lockfiles, build/test configuration and regression tests.
- Requirements, current state, plans, review findings and safe verification manifests. Failed findings remain preserved; an initial baseline commit does not invent a history of the earlier engineering work.
- Only the two existing synthetic KDBX interoperability fixtures under `crates/akm-core/tests/fixtures/`. Never place an owner vault or backup there to bypass the ignore rules.

`.gitattributes` disables newline normalization so Git preserves the exact source bytes used by the recorded fingerprints. Repository-local author settings may use the selected GitHub account's no-reply address; global Git identity and the default GitHub account are not changed.

Setup reviewed 582 selected files (about 6.1 MB), found no configured credential-pattern matches, verified exact staged bytes and 16 ignore-rule probes, and pushed only after checking owner/private visibility and disabling Actions. This is a bounded upload check, not an exhaustive secrets audit. Existing whitespace in historical `.diff`/browser diagnostic evidence was preserved; the source-only whitespace check passed. Authentication used the existing `Gargeya-Grey` credential only in command-scoped process memory, with no token in the remote URL or project files.

## What stays local

Vaults and backups, credentials, `.env` files/exports, private keys, settings preimages, process/UI recovery data, raw logs, source/evidence archives, screenshots, build products and dependency directories are excluded. Existing excluded files are **not deleted**.

Some versioned evidence documents link to these intentionally local records. A clone is a source/development-history backup, **not a complete evidentiary archive** and not proof that every referenced run can be reconstructed from Git alone. Keep the existing local hashed archives and raw failed reports; broader encrypted backup is a separate decision. Do not copy runtime settings/vaults into Git to repair missing links.

## Safe workflow

1. Review `git status` and the exact staged diff before each commit. Ignore rules are a guard, not a secrets scanner or an audit.
2. Commit small, truthful work units; attach exact verification scope and retain failed evidence. Do not rewrite rejected reviews into approvals.
3. Push only to the verified private repository. Keep GitHub Actions disabled until remote CI is separately authorized; the workflow source can remain tracked without running.
4. Do not make the repository public or publish artifacts without explicit approval, the owner's independent above-90/testing gate and a separate history-wide secrets/privacy review. Removing a secret in a later commit does not remove it from history.

Creating or pushing this repository does not accept unit1a, establish a new numerical score, or satisfy the pending owner-ready outcome test.
