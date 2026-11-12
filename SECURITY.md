# Security policy

## Status

AKM is an untrusted development build. It has had no professional security audit. Its latest measured review score is 66.25/100 ([full rescore](docs/reviews/2026-11-09-full-rescore.md)), and the release gate is an independent full review above 90/100 followed by real-use testing ([AGENTS.md](AGENTS.md)). **Real secrets wait for that gate.** Use fake keys and temporary vaults only. Do not send a real secret, vault or master password in a report.

## Report a vulnerability

Use GitHub's private vulnerability reporting on this repository:

1. Open the **Security** tab of the repository.
2. Choose **Report a vulnerability**.
3. Describe the problem, the steps to reproduce it with fake data, and the AKM build (the commit hash, or the SHA-256 of `akm.exe`).

Do not open a public issue or a pull request for a vulnerability. If the Security tab shows no **Report a vulnerability** button, private reporting is not yet turned on for the repository. Then open a public issue that asks for a private contact, and put no details in it.

AKM is a personal project without a service level. A report gets a reply when the maintainer can give one.

## In scope

- The Rust core (`crates/akm-core`), the CLI (`crates/akm-cli`) and the desktop app (`desktop`), as built from this repository.
- Flaws that expose a stored secret or the master password, that write a secret outside the vault or an export the user asked for, that pass a secret to a program the user did not name, or that make AKM overwrite or lose vault data.
- Flaws in the clipboard, export, session lock, `.env` import and journal handling that break a contract in [docs/PROJECT.md](docs/PROJECT.md).
- Errors in the build and check tools under `tools/` and `.github/` that could publish or delete data that they do not own.

## Out of scope

The attackers and limits that [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) lists as "Does not help" or "Disclosed limits": malware or a person that runs as the same Windows account, a keylogger, a memory or page-file dump, a camera, and a compromised operating system. Also out of scope:

- Defects already open in [docs/FINDINGS.md](docs/FINDINGS.md).
- The KDBX format and the `keepass` crate themselves (report those upstream). A flaw in how AKM uses them is in scope.
- The WebView2 runtime, Windows and other third-party software.
- A weak master password. AKM checks length only.

## Supported versions

Only the newest commit on `main`. No release exists, and there is no updater.
