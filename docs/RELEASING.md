# Releasing AKM

No release exists. Every build so far is an unsigned development build. This file prepares the release channel. Each step marked **owner gate** needs the owner's yes in chat (AGENTS.md): publishing a release or an installer, code signing, and making the repository public.

## The channel

AKM makes no network connection, by design ([PROJECT.md](PROJECT.md)), so it has no updater. A user gets each new version, and each security fix, by hand. So each release must let the user check two things without trusting the page that they download it from:

1. **Integrity.** The files are the files that CI built from a tagged commit. `SHA256SUMS.txt` lists the SHA-256 of each file.
2. **Origin.** `SHA256SUMS.txt` comes from the AKM maintainer. A signature over that file, checked with a public key that the user got from somewhere else, gives this.

A release is:

- a tag `vX.Y.Z` on a commit of `main` whose CI run is green;
- the files of that CI run: the setup program, `akm.exe`, and `SHA256SUMS.txt`, and a signature of `SHA256SUMS.txt`;
- release notes that name the commit, the CI run and the changes, and say that the setup program can download the WebView2 runtime from Microsoft while it installs (FINDINGS.md SEC-05; the no-network promise covers AKM when it runs, not the setup program);
- `LICENSE`, `THIRD_PARTY_NOTICES.md` and the source offer for the MPL-2.0 crates (SEC-04), which is in `THIRD_PARTY_NOTICES.md`.

A release workflow does not exist yet. It would build from the tag exactly as `build.yml` does and upload the files to a draft release. Writing it is a later step; publishing the draft stays an owner gate.

## The options for a signature

Costs and rules as found on 2026-11-13. Prices change; check them before you buy.

| Option | What a user checks | Cost | Removes the SmartScreen warning? | Limits |
|---|---|---|---|---|
| Authenticode, OV certificate from a certificate authority | Windows shows the publisher name; `Get-AuthenticodeSignature` reports `Valid`. | A yearly fee from each authority (not compared in this file). A certificate issued from 2026-02-27 is valid for at most 459 days. The private key must be on a hardware token or in a cloud key store. | Not at once. Microsoft builds SmartScreen reputation per file and per publisher, over downloads. | The name is a person's or a company's, checked by the authority. A key on a token cannot sign in CI without a cloud service. |
| Authenticode, EV certificate | The same as OV. | More than OV. | No longer at once: Microsoft's SmartScreen documentation says that EV is treated as OV since 2024. | Stronger identity checks only. |
| A cloud signing service (Microsoft Artifact Signing, formerly Trusted Signing) | The same as OV, with a Microsoft-issued certificate. | A monthly fee; billing starts when the account is made. | Not at once. | A Microsoft Q&A answer (marked as written by AI) says that individual developers can use it only in the USA and Canada. Not confirmed. |
| A free programme for open-source projects: SignPath Foundation | Authenticode, with the publisher "SignPath Foundation". | Free for a project that qualifies. | Not at once. | The repository must be public, under an OSI licence (MIT qualifies), with a published signing policy, CI builds and two-factor sign-in for every member. The publisher name is the foundation's, not AKM's. |
| Certum "Open Source Code Signing" | Authenticode, with the publisher "Open Source Developer, <name>". | From EUR 49 (cloud key) or EUR 69 (USB token) a year, on Certum's shop. | Not at once. | The certificate names a natural person. |
| GitHub artifact attestations | `gh attestation verify <file> --repo <owner>/<repo>` shows that the file came from a named workflow run of that repository. | Free for a public repository. A private repository needs GitHub Enterprise Cloud. | No. | It proves the CI origin, not a person. It trusts GitHub and the repository's account. |
| A detached signature with minisign, the public key in the repository | `minisign -Vm SHA256SUMS.txt -P <key>`. | Free. | No. | The user must get the key from the repository, not from the download page. Whoever controls the private key controls the releases. |

## Recommendation

- **Now, at no cost:** sign `SHA256SUMS.txt` of each release with minisign, and publish the public key in this file and in the README. When the repository is public, add GitHub artifact attestations to the release workflow; they cost nothing then.
- **To remove the SmartScreen warning over time:** apply to the SignPath Foundation when the repository is public. If the owner wants AKM's own name as the publisher, a Certum open-source certificate is the cheapest paid path found. No option removes the warning on the first day: reputation grows with downloads.

Both are owner decisions: the publisher name, and any money.

## The owner's steps (prepared, not done)

1. **Owner gate.** Make a minisign key pair on the owner's own computer: `minisign -G -p akm-release.pub -s <a path outside every repository>`. Keep the private key off GitHub. Put only the public key in this file, under "Release key", and in the README, in one commit.
2. **Owner gate.** Before the repository becomes public: run `python tools/make_public_snapshot.py --rev <commit> --scan-only`, clear every hit, then make the snapshot with `--out <new folder> --author "<public name> <public address>"` and publish that history, not the private one (FINDINGS.md FR-26). Turn on GitHub private vulnerability reporting ([SECURITY.md](../SECURITY.md)).
3. Tag the release commit. Wait for its CI run. Download the run's artifact. Check `SHA256SUMS.txt` against the hashes in the run's log.
4. **Owner gate.** `minisign -Sm SHA256SUMS.txt -s <private key>` makes `SHA256SUMS.txt.minisig`.
5. **Owner gate.** Publish the release with the files above and the notes.

## Release key

None yet. Step 1 adds it here.

## How a user checks a download

Use PowerShell. Replace the names with the release's files.

1. Get the public key from this file in the repository, at the release's tag. Do not take a key from the download page alone.
2. Check the signature of the checksum list: `minisign -Vm SHA256SUMS.txt -P <public key>`. It must print `Signature and comment signature verified`.
3. Check each file against the list: `(Get-FileHash .\AKM_X.Y.Z_x64-setup.exe -Algorithm SHA256).Hash.ToLower()` must equal its line in `SHA256SUMS.txt`.
4. When the release has an attestation: `gh attestation verify .\AKM_X.Y.Z_x64-setup.exe --repo <owner>/<repo>`.
5. When the release is Authenticode-signed: `Get-AuthenticodeSignature .\AKM_X.Y.Z_x64-setup.exe` must show `Valid` and the publisher that this file names.

If a check fails, do not run the file. Report it as [SECURITY.md](../SECURITY.md) says.
