# Import and clipboard

In the desktop app, a user imports a `.env` file into one project and environment after a native confirmation dialog that lists names and scope, never values. A user copies a secret to the clipboard with Windows privacy exclusions, and AKM later clears its own copy only.

## Sub-features

- `import-review` parses the file with the AKM dotenv profile and size limits before any dialog.
- `import-confirm` shows a native dialog (`Import` and `Cancel`) that names the scope; Import applies the change in the same native command.
- `import-abandon` makes a hidden, closed, edited, reloaded or locked review unusable.
- `copy-private` copies with clipboard-history, cloud and monitor exclusions.
- `copy-cleanup` clears only AKM's own copy, once per public call, notice first on lock.

## How to get to it (user POV)

- Desktop: Use keys, `Import .env…`, then `Review import` and the native dialog.
- Desktop: a key's detail view, its copy button.

## Driving it with verify_windows_app

Preconditions:

- **Owner present and the owner's yes in chat for each attempt.** The clipboard must be empty (`doctor.ps1 -NeedClipboard`).

- **Full proof.** `tools/verify_windows_app.ps1 -DesktopExe <akm-desktop.exe> -CliExe <akm.exe> -KeePassXC <keepassxc-cli.exe>` runs the import (it clicks `Import` in the native dialog through `Confirm-NativeImport` and checks that the dialog names the scope) and the clipboard flows. Expect `PASS:` lines for the scoped import and for the clipboard checks.
- **Outcome proof (R4-04).** `-OutcomeOnly -OwnerReadyForVisibility` minimizes and restores the window; its pass conditions are in `docs/FINDINGS.md`. One attempt per confirmation; stop on failure.
- **Real clipboard formats.** `$env:AKM_TEST_REAL_CLIPBOARD = '1'; cargo test -p akm-desktop --lib --offline --locked clipboard::windows::actual_windows_private_formats_are_present -- --ignored --exact`, then remove the variable.
- **Without the owner.** Only `npm test`, `npm run test:ui` and the Rust tests run; they use a scripted confirm and a fake Win32 clipboard. Report the native dialog and the real clipboard as not verified.

## Gotchas

- The dialog is a native `#32770` window that the AKM process owns, not part of the page. Find it among the process's top-level windows.
- The proof refuses to start while the clipboard has content. Never clear the owner's clipboard to make it start.
- `-SkipClipboard` skips the clipboard checks; report them as not verified.
