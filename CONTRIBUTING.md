# Contributing to AKM

AKM is a Windows API-key manager over a KeePass (KDBX) vault. It is not ready for real secrets (see [SECURITY.md](SECURITY.md)). Read [AGENTS.md](AGENTS.md) and [docs/PROJECT.md](docs/PROJECT.md) before you change behaviour: they hold the safety rules and the contracts that every change keeps.

## Fake data only

- Use temporary vaults and fake keys. A fake secret starts with `fake-`, so a search for `fake-` finds every leak in a log.
- Never commit a vault, a backup, an `.env` file, a key, a screenshot, a log or a proof transcript. `.gitignore` refuses many of them, but it is not a secrets scanner. Read your staged diff before each commit.
- Never let a test or a script write `%LOCALAPPDATA%\akm` or `%APPDATA%\com.akm.desktop`. Each process that a test starts gets its own `AKM_JOURNAL_ROOT`.
- Keep the clipboard and visible windows out of tests that run without a person watching ([docs/TESTING.md](docs/TESTING.md)).
- Report a vulnerability privately, never in an issue ([SECURITY.md](SECURITY.md)).

## Done means the checks pass

Run these from the repository root. [docs/TESTING.md](docs/TESTING.md) explains each one, and which checks a smaller change needs.

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python tools/check_docs.py
python tools/test_check_docs.py
python tools/check_no_network.py
python tools/generate_third_party_notices.py --check
```

And in `desktop`:

```powershell
npm ci
npm test
npm run build
npm run test:ui
```

A change to a tool runs that tool's own test (`tools/test_*.py` and `tools/test_*.ps1`). A defect fix comes with a test that fails before the fix and passes after it. Never weaken a test to make it pass.

## Documents

Write short sentences in the active voice, with one name for each thing and plain ASCII punctuation. Name a Rust test in a document only when that `fn` exists: `tools/check_docs.py` fails otherwise. A promise to users about security, privacy or data safety goes in [docs/CLAIMS.md](docs/CLAIMS.md) with its evidence and its limit.

## Licence

AKM is under the MIT licence ([LICENSE](LICENSE)). By contributing, you agree that your contribution is under the same licence. A new dependency must have a licence that `deny.toml` allows, and `THIRD_PARTY_NOTICES.md` must be generated again (`python tools/generate_third_party_notices.py`).
