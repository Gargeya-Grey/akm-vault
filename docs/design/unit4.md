# Unit 4: reminders and accessibility

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed (the independent review follows the whole chain). The owner may veto any decision under "Decisions for the owner's veto".
- Tier: `Tier 2: 5 units, risk: privacy, settings, public API, dependency`. The units: the shared key state and the accessible row status, the unlock summary, the native count-only notification, the reminder setting and notifier, and focus handling for every modal.
- Scope: M13 and M12 ([FINDINGS.md](../FINDINGS.md)), plus one defect found while running the page tests (4-W-01) and one stale manifest value (4-W-02).
- Inputs kept: [unit 2](unit2.md) (the secret-change time is `known`, `unknown` or `changed_outside`; AKM never invents a date; a key with a rotation interval and no known time is "Rotation unknown") and [unit 3](unit3.md) (a date-only expiry is the last second of that date in UTC; the page computes the states from due instants and a clock that calls nothing native, so a refresh is not user activity).
- Public API change: yes, inside the desktop app.
  - `akm-desktop` IPC: new commands `reminders_status`, `reminders_set(enabled)` and `reminders_notify(sessionId, count)`. New reply shape `ReminderStatus { enabled, permission, delivery, blocked }`.
  - New dependency: `tauri-plugin-notification` 2.8.0, called from Rust only. No capability entry (D4).
  - Frontend: `keyState` returns a `kind`; `KeysView` takes an optional `summary` control; new `reminders.ts`, `AttentionSummary`, `AttentionNotifier`.
  - Not changed: the vault, the KDBX format, the journal, the CLI, `settings.json`.

## 1. Facts the design uses

- The vault is encrypted while the session is locked. AKM cannot know which keys are due without the password. So there is no count to show while locked, and a notification while locked would need either the vault or a copy of its dates outside it.
- A session locks after 10 minutes without activity, on Windows session lock and on suspend (contract 6). A background window sends no activity. So the time in which a notification can be useful is short: an unlocked session whose window is behind another window or minimized.
- `tauri-plugin-notification` 2.8.0 on desktop: `permission_state()` and `request_permission()` always answer `Granted` (read in the crate source). `show()` returns `Ok` when the payload is prepared and sends on a background task, so `Ok` does not mean that Windows showed a toast. The crate notes say that on Windows it works for installed apps and shows the PowerShell name and icon in development.
- Network check, target `x86_64-pc-windows-msvc`: `cargo tree --target x86_64-pc-windows-msvc -p akm-desktop` shows that the plugin adds `tauri-plugin-notification` 2.8.0, `notify-rust` 4.18.2, `tauri-winrt-notification` 0.8.1, `rand` 0.10.3 and `rand_core` 0.10.1. `cargo tree -i reqwest` and `cargo tree -i hyper` print "nothing to print". `cargo tree -i` for `ureq`, `rustls` and `native-tls` says that no such package is in the lock file. The same result held before the change (commit 448a675). The check covers the Windows target only. The app is Windows-only.
- The workspace `rust-version` is 1.89, but `tauri-plugin-dialog`, `tauri-runtime` and others already needed 1.90 before this unit (row 4-W-02). The plugin needs 1.90 too, so it changes nothing.

## 2. Decisions for the owner's veto

### D1. One state function for the list, the detail, the summary, the count and the notification

`format.ts` `keyState(key, now)` returns one of six kinds. The order is the order of precedence.

| Kind | Rule | Label |
|---|---|---|
| `expired` | the expiry instant has passed (`isPast`) | Expired |
| `overdue` | the rotation due instant has passed | Rotation overdue |
| `expiresSoon` | expiry is in the future and at most `EXPIRES_SOON_DAYS` (30) days away | Expires soon |
| `rotateSoon` | rotation falls due in at most `ROTATE_SOON_DAYS` (14) days | Rotate soon |
| `unknown` | a rotation interval and no known secret-change time (`rotationDueAt` is null) | Rotation unknown |
| `ok` | none of the above | OK |

**Due soon** means `expiresSoon` or `rotateSoon`: the two thresholds that the Keys screen already used. They are named constants now and are not settings. The days come from the same `statusOf` and `remainingOf` that unit 3 tested against `cases.json`, so a day count is rounded in one place.

`reminders.ts` `attentionOf(keys, now)` groups keys by `keyState(...).kind`. The summary, the "need attention" number on the Keys screen and the notification count all read it. Each key counts once, under its main state, so the groups add up to the number on the screen. A key that is expired and also has an unknown rotation shows as expired.

Rejected: new thresholds for reminders (two answers to "is this key due soon?"); a setting for the thresholds (nobody asked, and it is more to test); counting a key in every group it fits (the sum would differ from the number on the screen).

### D2. The unlock summary

- Where: at the top of the Keys view, above the vault numbers. Not a dialog, no focus capture, no sound. The view is the first view after an unlock.
- Content: the heading ("4 keys need attention", singular "1 key needs attention") and one line per non-empty group: "Expired (1): A", "Rotation overdue (n): …", "Due soon (n): …", "Rotation unknown (n): …". Up to five titles per group, then ", and N more". Counts and key titles only: no project, environment, variable name, note, URL, username or value.
- Not shown when no key needs attention, or when the vault has no key.
- It follows the clock while open (it reads the same `now` as the list).
- Dismiss: a button, reachable by keyboard. Dismissing moves focus to the search field (a button that disappears must not drop focus to `<body>`). The dismissal belongs to the unlock session: the app keeps it keyed by `status.sessionId`. Leaving the Keys view does not dismiss it, and coming back after a dismissal does not show it. A lock removes the view (the page unmounts it with the other unlocked views), and the next unlock has a new session id, so the summary appears again.
- A hidden or minimized window neither dismisses nor re-shows it. The summary holds titles only, the same data as the key list that stays on screen; it is not a protected view like a revealed secret or an env preview.

Rejected: a summary in every view (needs the keys outside the Keys view, and a second source of data); showing it again after the window returns; a modal (focus capture for information the user did not ask for); an "all clear" message when nothing is due (noise).

### D3. "Unknown" needs attention

A key with a rotation interval and no known secret-change time counts, in the summary and in the notification, as needing attention. Unit 2 decided that AKM cannot say such a key is on schedule. A key with no rotation interval and no expiry is "OK" and never counts.

### D4. The opt-in Windows notification: built now

**Decision: build it**, because the check in section 1 shows no networking crate on Windows, and the design below keeps every rule of the brief.

| Rule | How it holds |
|---|---|
| Off by default | `reminders.json` (one flag) beside `settings.json`; missing or unreadable means off. `settings.json`, which holds the saved vault path, is not read or written by this feature. |
| Text holds no secret and no sensitive metadata | `reminders_notify(sessionId, count: u32)` is the only call. The native layer builds the text: title "AKM", body "N API keys need attention" ("1 API key needs attention"). The page cannot pass a title, a project, an environment or any text; a test sends extra fields and checks the toast is unchanged. |
| Only while the app runs | No service, no scheduled task, no startup entry, no autostart plugin (a test reads `tauri.conf.json` and `Cargo.toml`). The notifier is a React component that exists only while the app runs and the vault is unlocked. |
| Never while locked | `reminders_notify(sessionId, count)` names the session that the page counted in. Native hands the text to the notifier only inside `Session::while_session`. That call holds the gate that lock and a newer opening take, after it checked that this session is still the current one (review 1, finding 7). A lock or a new opening that arrives while the call runs waits for it. One that arrived earlier makes the request answer `locked`, and nothing is shown. A count made in an older session never reaches the notifier. The page also unmounts the notifier on a lock. |
| Denied or disabled permission is a state | `ReminderStatus.permission` is `granted`, `denied` or `prompt`. A permission other than `granted` blocks the run: no toast, no retry. A failed `show` does the same (`delivery: failed`). Turning the setting off and on clears the block. Settings shows each state in words (D5). |
| No retry loop | `blocked` holds until the user turns the setting off and on. The page asks `reminders_status` before it reads any key, and stops there when the setting is off or blocked. |
| Capability entry | None. The plugin is called from Rust only. `capabilities/default.json` still holds `core:default` and `dialog:default`, so the webview has no `notification:*` permission and cannot send a notification itself. A test reads the file. (The brief expected an entry. The permissions are not needed: a permission for the page to send arbitrary notification text is the thing to avoid.) |

When the page sends the count: the notifier looks at start, every 30 seconds, and on focus, blur and visibility changes. A look reads the keys (a read; not activity) and computes `attentionOf(...).total`. `nextAnnouncement(announced, count, inFront)` decides:

| announced | count | window | result |
|---|---|---|---|
| any | above `announced` | in front | remember `count`; send nothing (the summary and the Keys screen show it) |
| any | above `announced` | behind or hidden | remember `count`; send one notification |
| any | not above | any | remember `count` (so a later rise is sent again); send nothing |

`announced` starts at 0 at each unlock. So an unlock with the window in the background sends once, a key that enters the due-soon window while the window is behind sends once, and the same count never sends twice.

Honest limits:
- The window of use is short (section 1): the vault locks after 10 idle minutes. The notification helps while the window is behind other windows or minimized in that time. It is not a reminder for days when AKM is locked or closed, and it cannot be one without a copy of the dates outside the vault or a background program.
- No lock and no newer opening lands between the session check and the hand-over to the notifier (finding 7). The limit is what Windows does after the hand-over. `show()` returns when the payload is prepared and sends on a background task (crate source, `desktop.rs`). So a toast that the notifier already accepted can appear after a later lock, and AKM cannot withdraw it. The text is the same count that was true at the hand-over. A test cannot prove the Windows side. That needs the owner present.
- The notifier runs under the session gate. On Windows, `show()` does not wait for the main thread (checked in the crate source), so the gate is held for a short time only. A notifier that blocked would block lock. The test `review1_7_lock_and_a_new_opening_wait_for_a_notification_that_passed_validation` shows that lock waits for the call.
- `delivery: sent` means "handed to Windows". Focus assist and the per-app notification switch decide whether a toast shows, and AKM cannot read either. Settings says so.
- On a lock screen Windows may show the toast depending on its own setting. The text is a count of API keys, no title.
- In a development build Windows shows the PowerShell name and icon (crate note). Installed builds were not tested here.
- Mocked UI tests and fake notifiers do not prove native delivery. FINDINGS keeps M13 at "Partial" for this reason.

Rejected: the JavaScript plugin (needs `notification:*` permissions for the page and lets it name a text); a scheduled task, a startup entry or a service so that a notification can come while AKM is locked (a hidden prerequisite, and it needs the dates outside the vault); a notification at every unlock whatever the window state (it announces what the user is looking at); per-key notifications with titles (sensitive metadata on a lock screen); storing the setting in `settings.json` (a second writer to a file that holds the saved vault path, and that file must stay as it is); persisting due instants after a lock (dates of every key outside the vault).

### D5. Settings: the states

A "Reminders" section sits after "Vault" in Settings: a checkbox ("Show a Windows notification when keys need attention"), the note ("The notification says only how many keys need attention. It names no key, project or environment. AKM runs nothing in the background: it can show one only while AKM is open and unlocked."), and one `role="status"` sentence:

| State | Sentence starts |
|---|---|
| off | "Off. AKM shows a summary after you unlock the vault. It sends no Windows notification." |
| on, nothing sent | "On. AKM sends one count-only notification when more keys need attention while this window is in the background and the vault is unlocked." |
| on, sent | "On. The last notification was handed to Windows. AKM cannot see whether Windows showed it: Focus assist and the per-app notification switch decide that." |
| on, permission not granted | "On, but Windows has not allowed notifications for AKM. AKM does not try again. Allow them in the Windows notification settings for AKM, then turn this setting off and on." |
| on, failed | "On, but Windows did not accept the last notification. AKM does not try again in this run. Turn this setting off and on to try again." |
| cannot read | an error notice, and the box is disabled |
| cannot save | an error notice; the box returns to the saved choice |

The box is disabled while a change is saved, so a second click cannot send a second request.

### D6. Accessible key status (M12)

The row button keeps its name "Open <title>" (existing tests and users know it). The status is its accessible description: an `aria-describedby` that points to a visually hidden sentence in the row's list item: "<expiry sentence>. <rotation sentence>." Both sentences come from `format.ts`, from the same `statusOf` as the visible text:

| Expiry | Sentence |
|---|---|
| none | No expiry date |
| in N days | Expires in N days |
| exactly now | Expires now |
| less than a day late | Expired less than a day ago |
| N days late | Expired N days ago |

| Rotation | Sentence |
|---|---|
| no interval | No rotation interval |
| interval, no known time (unknown, or changed outside AKM) | Rotation unknown, no known secret-change time |
| due in N days | Rotation due in N days |
| exactly now | Rotation due now |
| less than a day late | Rotation overdue by less than a day |
| N days late | Rotation overdue by N days |

The description follows the clock because it is rendered from `now`. A key is never described as on schedule when AKM has no change time.

Rejected: putting the status into the name (every query for "Open <title>" in the tests and the e2e checks would change, and a long name is read on every focus); a live region (the clock would announce changes the user did not ask for).

### D7. Focus in every modal (M12)

The inventory is `grep -rn 'role="dialog"' desktop/src --include=*.tsx`: Key Detail, the add and edit key form, Rotate secret, the confirmation dialog (used for "Move to the Recycle Bin", "Delete for good" and the format upgrade) and the Settings import dialog. There is no other modal: export uses the native save dialog, and the unlock summary is not a dialog.

`Modals.keyboard.test.tsx` drives the production App with the keyboard only (Tab, Shift+Tab, Escape, Enter) through each. On the unit 3 code, 6 of 10 failed: containment, Escape and Enter were right, but focus went to `<body>` in two cases.
1. A dialog opened from a control inside another dialog (Key Detail to edit, rotate or move to the Recycle Bin) took the control as its opener. The control is gone when the first dialog closes, so the end of the chain lost focus.
2. A dialog whose opener vanishes on success (a deleted row, a purged row, an upgrade that removes its own button).

The fix is in `useDialogFocus`: a dialog opened from inside another dialog inherits that dialog's opener (a `WeakMap` of anchors), and focus that is lost after the dialog closes goes to the `[data-focus-fallback]` element of the view (the Keys search field, the Recycle Bin heading). The synchronous return to a live opener is unchanged. The fix is a rule of the hook, so every dialog has it.

Behaviour that stays, and is tested: a dialog with a pending request ignores Escape and its buttons are disabled (Rotate, import, confirmations); Enter submits a form once.

### D8. Kept layout debt on the Keys screen

This unit does not change the look of the Keys screen (ROADMAP: functional and visual changes go in separate pull requests). The summary is new content, not a restyle. The debt below is read from `desktop/src/styles.css` and `KeysView.tsx`. No pixel measurement was made in this unit.

1. Each row is its own grid (`.key-row`, last columns `auto 12px`). The status column is `auto`, so its width depends on the label in that row. The columns of one row can sit at other x positions than the same columns of the next row, and the `.key-head` headings can drift from the cells.
2. A long title is cut with an ellipsis (`.key-title`, `white-space: nowrap`). The list has no tooltip or other way to read the full title. The detail dialog shows it.
3. The list is sorted by title only (`localeCompare`). Keys that need attention are not grouped or marked at the top. The summary now lists them, but the list order does not help.
4. The contrast of `--clay` text on `--paper` and `--paper-raised`, in light and dark, was not measured against WCAG. The status label carries the meaning in text, so colour is not the only signal.

These stay as kept debt. FINDINGS lists them under M12. A later visual unit owns them.

## 3. Data shapes

| Shape | Where | Content |
|---|---|---|
| `KeyStateKind` | `format.ts` | `expired`, `overdue`, `expiresSoon`, `rotateSoon`, `unknown`, `ok` |
| `Attention` | `reminders.ts` | `expired`, `overdue`, `dueSoon`, `unknown` (key lists) and `total` |
| `ReminderStatus` | IPC | `enabled`; `permission`: `granted`, `denied`, `prompt`; `delivery`: `none`, `sent`, `failed`; `blocked` |
| `reminders.json` | app config folder | `{"enabled": bool}` |
| `reminders_notify` | IPC | arguments `{sessionId, count}` only; the reply is a `ReminderStatus` |

## 4. Failure cases

| Case | Result |
|---|---|
| Vault has no key | no summary; the count is 0; no notification |
| Every key is OK | no summary; no notification |
| A key is expired | summary "Expired"; counted |
| Rotation overdue | summary "Rotation overdue"; counted |
| Rotation interval, no known change time | summary "Rotation unknown"; row says "Rotation unknown, no known secret-change time"; counted |
| Key with expiry in 10 days | summary "Due soon" |
| Lock while the summary is open | the summary is gone with the view; titles are not left in the page; the next unlock shows it again |
| Window hidden or minimized | the summary stays or stays dismissed; no protected value is involved |
| Setting off | `reminders_status` only; no key read; no toast |
| Notification permission denied or not granted | a state in Settings; no toast; no read of the keys; no retry until off and on |
| Windows refuses a toast | `delivery: failed`; blocked for the run; shown in Settings |
| Setting cannot be saved | an error in Settings; the old choice stays |
| Reminder file unreadable | off |
| Native call arrives after a lock | `locked`; nothing shown |
| Page sends a title with the count | ignored; the toast is the fixed text |

## 5. Verification

- Page tests: `KeysView.status.test.tsx` (11), `Modals.keyboard.test.tsx` (10), `Reminders.summary.test.tsx` (11), `Reminders.notify.test.tsx` (10), `Settings.reminders.test.tsx` (9), `useNow.test.tsx` (2). Mutants are in the commit messages and in FINDINGS.
- Headless Edge: `e2e/reminders-ui.spec.ts` (5 tests in light and dark): `toHaveAccessibleName` and `toHaveAccessibleDescription` on every row, the summary dismissed by keyboard, a lock removing the summary and the next unlock showing it, focus in Key Detail and Rotate secret, the setting and a denied permission.
- Rust: `reminders.rs` (10 tests) and the new wire checks in `tests/ipc_dispatch.rs`.
- Full run before the last commit: `cargo test --workspace --offline --locked` 410 passed, 0 failed, 2 ignored (built in a target folder outside the repository because the W: drive was full); `npm test` 299 passed (27 files, default workers); `npm run build` clean; `npm run test:ui` 50 passed.
- Not tested: a real Windows toast, Focus assist, the lock screen, an installed build, the permission switch of Windows, and the real window states (minimized or covered). The GUI proof was not run.

## 6. Limits

- Section D4 lists the limits of the notification.
- The summary and the clock are page state. They do not extend the idle deadline and they call nothing native, as in unit 3.
- The screen reader behaviour was checked through the accessibility tree that Testing Library and Playwright compute, not with NVDA, JAWS or Narrator.
- Layout debt: D8.
