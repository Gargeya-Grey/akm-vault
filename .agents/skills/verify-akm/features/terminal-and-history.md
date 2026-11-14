# Terminal with keys and key history

In the desktop app, a user opens a terminal that already holds one project's keys, and sees the earlier versions of a key. Neither needs the command line. Design: [launch-with-keys](../../../../docs/design/launch-with-keys.md).

## Sub-features

- `terminal-scope` enables `Open terminal with keys…` only when a project and an environment are chosen and nothing else is running.
- `terminal-review` asks for a folder in a native picker, then shows a native window that lists the project, the environment, the folder and each variable name in full, never a value, and says that the terminal keeps the keys until it is closed. A name too long to show in full is refused before any window. The window can appear just after a lock. It then starts nothing, even if the user approves it.
- `terminal-open` starts the system command prompt in a new console window in that folder. The child holds the selected variables and no `AKM_*` variable. AKM does not wait for it.
- `terminal-record` writes a `run-requested` journal entry before any value leaves the vault. A value is read inside the vault earlier, to validate it. A journal failure starts nothing. A lock during the write is prompt, and the record can then exist for a launch that never started (owner's option (a)).
- `history-show` lists a key's earlier versions in its drawer: a time and the names of the changed fields, never a value. It clears when the view expires.

## How to get to it (user POV)

- Desktop: Use keys, choose a project and an environment, `Open terminal with keys…`. Choose a folder, then answer the native window.
- Desktop: a key's detail drawer, the History row, `Show earlier versions`.

## Driving it

Preconditions: fake vault only. A terminal window is a visible window and a native dialog needs a click, so the real flow needs **the owner present and the owner's yes in chat for each attempt**.

- **Without the owner.** `npm test` (the scope gate, the request fence, the late reply after a hide, the history list and its expiry), `npm run test:ui` (the same screens in a browser with a mocked boundary), and `cargo test --workspace --offline --locked`. The Rust tests use a fake launcher that records the folder, the names and the environment. They open no window, no dialog and no process.
- **Real flow, owner present.** Start the desktop build against a fake vault that holds a key in project `demo`, environment `dev`. Click `Open terminal with keys…`, choose an empty temporary folder, check that the native window lists the variable name and no `fake-` value, click `Open terminal`. In the new window, `echo %OPENAI_API_KEY%` shows the fake value and `set AKM_` shows no variable. Then `audit` shows `run-requested` once, with a count and no folder. Close the terminal window yourself.
- **Cancel paths, owner present.** Cancel the folder picker: nothing opens, no `run-requested` entry. Choose a folder and cancel the window: the same.
- **History.** Rotate a fake key twice (`Rotate secret`), open its drawer, `Show earlier versions`. Two rows with times and `Changed: secret`. Hide the window or lock: the list is gone.

## Gotchas

- The native window and the picker belong to the AKM process, not to the page. Find them among its top-level windows.
- The terminal keeps its keys after AKM locks or exits. That is the design, and the window says so.
- The terminal starts with handle inheritance off in every build (`launch.rs`), so it gets its own console handles. This is read from the code; no real terminal was started to check it.
- Not verified in this unit: the real console window, the real dialogs, and how the default button behaves on screen. A test reads Cancel as `nDefaultButton` in the real window config (`review_window.rs`). Enter and Escape are an owner-present test.
- A failed review window is an error, not a cancel. The page shows the error and nothing starts.
