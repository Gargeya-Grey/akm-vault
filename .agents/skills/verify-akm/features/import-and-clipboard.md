# Import and clipboard

In the desktop app, a user imports a `.env` file into one project and environment after a native confirmation dialog that lists names and scope, never values. A user copies a secret to the clipboard with Windows privacy exclusions, and AKM later clears its own copy only.

## Sub-features

- `import-review` parses the file with the AKM dotenv profile and size limits before any dialog.
- `import-confirm` shows a native dialog (`Import` and `Cancel`) that names the scope; Import applies the change in the same native command.
- `import-abandon` makes a hidden, closed, edited, reloaded or locked review unusable.
- `disclosure-approval` (FR-03) shows a native window (`Show values` or `Copy values`, and `Cancel`) before the Use keys Preview and Copy to clipboard commands release any value. It names the scope and every variable, never a value. Cancel answers `null`: no value, no clipboard write, and the page says `Cancelled.`.
- `disclosure-budget` (FR-03) lets one session show or copy 5 single keys in a rolling 60 seconds. The next request opens the same kind of window for that one key (`Show value` or `Copy value`). A lock or a new unlock resets the count.
- `copy-private` copies with clipboard-history, cloud and monitor exclusions.
- `copy-cleanup` clears only AKM's own copy, once per public call, notice first on lock.

## How to get to it (user POV)

- Desktop: Use keys, `Import .env…`, then `Review import` and the native dialog.
- Desktop: a key's detail view, its copy button.
- Desktop: Use keys, `Preview` or `Copy to clipboard`, then the native window.

## Driving it with verify_windows_app

Preconditions:

- **Owner present and the owner's yes in chat for each attempt.** The clipboard must be empty (`doctor.ps1 -NeedClipboard`).

- **Full proof.** `tools/verify_windows_app.ps1 -DesktopExe <akm-desktop.exe> -CliExe <akm.exe> -KeePassXC <keepassxc-cli.exe>` runs the import (it clicks `Import` in the native dialog through `Confirm-NativeImport` and checks that the dialog names the scope) and the clipboard flows. Expect `PASS:` lines for the scoped import and for the clipboard checks.
- **Outcome proof (R4-04).** `-OutcomeOnly -OwnerReadyForVisibility` minimizes and restores the window; its pass conditions are in `docs/FINDINGS.md`. One attempt per confirmation; stop on failure.
- **Real clipboard formats.** `$env:AKM_TEST_REAL_CLIPBOARD = '1'; cargo test -p akm-desktop --lib --offline --locked clipboard::windows::actual_windows_private_formats_are_present -- --ignored --exact`, then remove the variable.
- **Disclosure windows (FR-03).** The scripts do not drive them: no step of `tools/verify_windows_app.ps1` clicks `Preview` or `Copy to clipboard`, and its single-key reveals and copies (two reveals in one view, and one copy after each unlock) stay inside the budget of 5, so no window opens. A new proof of `Preview`, `Copy to clipboard` or a sixth single request needs the owner to click `Show values`, `Copy values`, `Show value` or `Copy value` in the native window, and the owner's yes in chat for each attempt. Until then, report the real window as not verified. The Rust and UI tests cover the approval with a scripted window (`fr03_*` in `commands.rs`, the compromised-page case in `tests/ipc_dispatch.rs`).
- **On a disposable runner.** The jobs `real-clipboard`, `full-gui-proof` and `r4-04-attempt` of `.github/workflows/native-proof.yml` run the three steps above on a GitHub-hosted runner, and keep each transcript as an artifact. The owner starts the workflow. A run of `r4-04-attempt` does not pass R4-04 while R4-04-SCRIPT is open.
- **Without the owner.** Only `npm test`, `npm run test:ui` and the Rust tests run; they use a scripted confirm and a fake Win32 clipboard. Report the native dialog and the real clipboard as not verified.

## Gotchas

- The dialog is a native `#32770` window that the AKM process owns, not part of the page. Find it among the process's top-level windows.
- The proof refuses to start while the clipboard has content. Never clear the owner's clipboard to make it start.
- `-SkipClipboard` skips the clipboard checks; report them as not verified.
