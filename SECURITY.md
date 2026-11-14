# Security policy

## Status

AKM is an untrusted development build. It has had no professional security audit. Its latest measured review score is 66.25/100 ([full rescore](docs/reviews/2026-11-09-full-rescore.md)), and the release gate is an independent full review above 90/100 followed by real-use testing ([AGENTS.md](AGENTS.md)). **Real secrets wait for that gate.** Use fake keys and temporary vaults only. Do not send a real secret, vault or master password in a report.

## Report a vulnerability

Never open a public issue, a discussion or a pull request about a vulnerability.

**While the repository is public:** use GitHub's private vulnerability reporting.

1. Open the **Security** tab of the repository.
2. Choose **Report a vulnerability**.
3. Describe the problem, the steps to reproduce it with fake data, and the AKM build: the commit hash, or the SHA-256 of `akm.exe` or of the setup program.

The report is visible only to you and the people who administer the repository. GitHub offers this route on public repositories only. The maintainer turns it on before the repository becomes public ([docs/RELEASING.md](docs/RELEASING.md), the owner's steps). If the button is missing on a public repository, open a public issue with the title "Security contact request" and no details, and the maintainer will open a private advisory and invite you to it.

**While the repository is private:** only people whom the maintainer invited can read it. Report to the maintainer in the private channel where you got your access, or ask there for a draft security advisory. Put no details in an issue.

## What happens after a report

AKM is a personal project without a service level, and the maintainer gives no fixed reply time. The process is:

1. The maintainer confirms that the report arrived, and tells you whether they can reproduce it with fake data.
2. The fix is prepared in private: in the advisory's temporary private fork when the repository is public.
3. A fixed build is published as a release with `SHA256SUMS.txt` and its signature ([docs/RELEASING.md](docs/RELEASING.md)). AKM has no updater, so the advisory tells users which version to install by hand.
4. The advisory is published after the fixed release, with credit to you if you want it. GitHub can assign a CVE identifier to the advisory.

## In scope

- The Rust core (`crates/akm-core`), the CLI (`crates/akm-cli`) and the desktop app (`desktop`), as built from this repository.
- Flaws that expose a stored secret or the master password, that write a secret outside the vault or an export the user asked for, that pass a secret to a program the user did not name, or that make AKM overwrite or lose vault data.
- Flaws in the clipboard, export, session lock, `.env` import and journal handling that break a contract in [docs/PROJECT.md](docs/PROJECT.md).
- Errors in the build and check tools under `tools/` and `.github/` that could publish or delete data that they do not own.
- A claim in [docs/CLAIMS.md](docs/CLAIMS.md) that the code does not keep.

## Out of scope

The attackers and limits that [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) lists as "Does not help" or "Disclosed limits": malware or a person that runs as the same Windows account, a keylogger, a memory or page-file dump, a camera, and a compromised operating system. Also out of scope:

- Defects already open in [docs/FINDINGS.md](docs/FINDINGS.md).
- The KDBX format and the `keepass` crate themselves (report those upstream). A flaw in how AKM uses them is in scope.
- The WebView2 runtime, Windows and other third-party software.
- A weak master password. AKM checks length only.

## Supported versions

Only the newest commit on `main`. No release exists, and there is no updater.
