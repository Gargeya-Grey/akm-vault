# First use with real keys

This page is for the owner, who decided on 2026-11-07 to use AKM for their own keys before the release gate. That gate is an independent full review above 90/100. The latest measured score is 66.25/100 ([full rescore](reviews/2026-11-09-full-rescore.md)). This page does not change that, and the decision and its risk are the owner's.

## Before the first real key

1. **Install AKM.** Get the setup program `AKM_0.1.0_x64-setup.exe` from the agent, or from the build of the newest CI run on `main` (artifact `akm-windows-x64-untrusted-development`: `akm.exe`, the setup program and `SHA256SUMS.txt`), and compare the files with `SHA256SUMS.txt`. A developer can build it with the commands in the [README](../README.md#build-and-verify-on-windows). A hash identifies one build, so a build made on another machine can have a different hash. Double-click the setup program. It installs for the current user, and needs no administrator rights. It is unsigned, so Windows SmartScreen warns before it runs. It holds Microsoft's WebView2 bootstrapper, which downloads the WebView2 runtime from Microsoft while AKM installs, when the runtime is missing (SEC-05). The no-network promise covers AKM when it runs, not the installer. To install it is the owner's own step.
2. **Keep every key recoverable at its provider.** Store in AKM only keys that you can revoke and create again at the provider. Then a bug in AKM costs a rotation, not an outage.
3. **Choose the vault folder.** The CLI default is `%APPDATA%\akm\vault.kdbx`. That is the Roaming profile, and a domain profile can sync it to a server (SEC-02 in [FINDINGS.md](FINDINGS.md)). A folder under `%LOCALAPPDATA%` or a folder that you choose avoids that. In the desktop app, choose it with Browse on the first screen. On the command line, use `--vault` or `AKM_VAULT`. Each save writes the vault and its backups as files that only your Windows account can use. On a drive that cannot keep Windows permissions (FAT, exFAT, some network shares), AKM saves and warns that the files are not private. An administrator can still read them.
4. **Use a long, unique passphrase.** AKM has no reset or recovery.
5. **Prove the escape route.** Create the vault and add one low-value key. Then open the same file in KeePassXC 2.8 or later and find the key. If AKM fails later, KeePassXC still reads your vault.
6. **Keep a copy outside AKM.** AKM keeps five dated backups next to the vault. Also copy the vault file to a second place by hand after you add keys.

## Daily use

- In the desktop app, **Use keys** gives keys to your projects. **Open terminal with keys…** opens a command prompt in a folder that you choose, with the keys of one project and environment. The program you start there reads them as environment variables, and no file is written. Prefer it to a `.env` file. The window keeps the keys until you close it, even after AKM locks.
- On the command line, `akm run --project <p> --env <e> -- <command>` does the same for one program.
- `akm export` writes plaintext. Write only into a folder that Git ignores, and delete the file when you are done.
- AKM tries to clear its clipboard copy after a short time, on lock and on normal exit. A copy can remain if the clipboard is busy or if AKM stops unexpectedly. A clipboard manager or a fast reader can also keep it ([THREAT_MODEL.md](THREAT_MODEL.md), row 9). Do not rely on the clear.
- The journal in `%LOCALAPPDATA%\akm\journal\` records each reveal, copy, export and run. It holds key IDs and times, never a secret or a title. It is not tamper-proof evidence.
- A rotation in AKM does not change the key at the provider. Rotate at the provider first, then run `akm rotate`.

## Tests that need you present

These were never run, because the agent rules allow them only with the owner present. Run them in one session, with an empty clipboard and nothing else open in AKM. The commands are in [TESTING.md](TESTING.md).

| Test | What it proves | How |
|---|---|---|
| Real clipboard test | The Windows clipboard formats that keep a copy out of history and cloud sync | `AKM_TEST_REAL_CLIPBOARD=1`, the one ignored `cargo test` in TESTING.md |
| Full native GUI proof | Copy, the attempt to clear it, and lock on the real window | `tools/verify_windows_app.ps1` without `-SkipClipboard` |
| Import dialog | The native dialog lists names and scope, never values; Cancel writes nothing | Import a fake `.env` file in the desktop app; press Cancel once, Import once |
| Windows notification | It shows only a count, and never after a lock | Turn on the notification in Settings with one overdue fake key; lock, then wait |
| Import and lock | The native dialog is the review (P2R2-02): an Import click in it applies the import; a lock before the click refuses it | Start an import; once press Import after you minimize and restore the window, once lock AKM (Win+L or the idle lock) before you press Import |
| Open terminal with keys | The native windows list the folder and the names, never a value; Enter and Escape in the review open nothing; the terminal holds the fake keys and no `AKM_` variable | With a fake vault, in **Use keys**: cancel the folder window once, press Enter in the review once, press Escape in it once, then click Open terminal. Run `set` in the terminal. |
| R4-04 | The outcome records survive a minimize (open P1) | Choose option A or B in [FINDINGS.md](FINDINGS.md) first |

Report a failure as a new row in [FINDINGS.md](FINDINGS.md), with fake data only.
