# Frontend reads and rows (unit 2 of the gate campaign)

This note records how the desktop frontend reads native state and how it draws a key row. It
also lists the decisions that the owner can veto. The campaign plan is
[gate-campaign.md](gate-campaign.md).

**Note of 13 Nov 2026.** The test audit removed the tests that read source text: `readGuard.test.ts`, `rowFactsGuard.test.ts`, `nativeErrorKinds.test.ts`, `promiseWords.test.ts`, and the parts of `plaintextGuard.test.ts` that read the Rust commands and the import graph (owner's rule: a test stays only when it protects a behaviour or a stated contract). The rules that those tests guarded stay rules of the design, and a review checks them. Every paragraph below that describes the read guard, the row-facts guard, the error-kind scan or the promise-word scan describes a removed test, and stays here as the record of the design. The tests that drive the page stay: the views, the sealed replies and the browser tests.

## The read mechanism (fault classes K and L)

One hook, `useNativeRead` in `desktop/src/nativeRead.ts`, owns every read of native state that a
view shows. A view names a read from the `nativeReads` table. Each entry of that table is a frozen
token that only `useNativeRead` can run, so a view cannot call a read itself.

The reads live in `reads`, in `desktop/src/native.ts`, beside the one boundary function `invoke`.
`api` in `desktop/src/api.ts` holds only actions. Only three files may import `native.ts`, and
only `native.ts` may import the Tauri IPC (U2-R1-05). The guard test `desktop/src/readGuard.test.ts` (removed on 13 Nov 2026; see the note at the top)
reads the import graph of `desktop/src` with the TypeScript parser and checks it by the resolved
module, never by the name of a binding. An alias, a re-export by name or by `export *`, a helper
that passes the reads on, a namespace import, a dynamic import, a `require`, an import-equals, an
import glob, a side-effect import and a Tauri global all count. A type-only import does not: it
gives no value at run time.

The state is a union with three cases:

| Case | Holds | Meaning |
|---|---|---|
| `loading` | nothing | The first read has not ended. |
| `ready` | the data | The newest read succeeded. |
| `failed` | the error text | The newest read failed. |

A separate `busy` flag says that a read is in flight. In `ready` it is a refresh. In `failed` it
is a retry.

The rules:

1. Each read gets a request number. Only the newest request can set the state. A late success
   or a late failure of an older request changes nothing.
2. Each read records the session generation when it starts. A reply that arrives after the
   generation changed changes nothing.
3. A reply that arrives after the view unmounted changes nothing. A retry or a change reply after
   unmount starts no read.
4. Only `ready` holds data. A view says "empty" only from `ready` with zero items. A failed read
   drops the data that an earlier read gave, so no old row or count stays beside an error.
5. A refresh keeps the current state on screen, marked busy. Rows keep their elements, so focus
   stays where it was.
6. A retry control is disabled and says that it is busy while its read is in flight.
7. `apply` takes the reply of a change that the view made (a save, a rotation, a removal, a
   restore, a purge, a saved setting). With no read in flight, it patches the data. With a read
   in flight, that read started before the change, so a new read replaces it.

### Inventory

| Caller | Read | Trigger | What can overlap |
|---|---|---|---|
| Keys | key list | mount | save, rotate and remove replies (`apply`) |
| Use keys | key list | mount, retry, after an import | a retry and the refresh after an import |
| Recycle Bin | recycled list | mount, retry | restore and purge replies (`apply`) |
| Activity | journal | mount, Refresh, after a journal recovery | Refresh is disabled while a read runs |
| Settings | key list, for the count | mount, a new polled count, retry | a new polled count while a read is pending |
| Settings | backup details | mount, Refresh backup info | none: the control is disabled while it runs |
| Key detail | history of one key | the history list opens, retry | close and open again, another key, view expiry |
| App | status | the 3 second poll and events | allowed: the poll keeps its own request, epoch and transition identity |
| App | warnings | mount and the 3 second poll | allowed: the reply arrives through the event that `native.ts` owns |

The guard test names the three files that may import `native.ts`, each with its reason: `api.ts`
for the actions, `nativeRead.ts` for the mechanism and `App.tsx` for the two polls. It also fails
when an allowed file no longer reaches its module, so the list cannot grow stale. A new member of
`reads` or of `api` must be classed as a read or an action, or the guard test fails.

### What each view now shows

- Keys: no count and no project shelf until the first read succeeds.
- Use keys: the project and environment lists stay disabled until the read succeeds. A failed
  read shows the failure with Try again. A chosen scope counts only while its option is on
  screen. An export preview shows only under the scope that it belongs to.
- Recycle Bin: a restore or purge error is separate from the read failure.
- Activity: a journal recovery that failed is separate from one that was declined. The journal
  read failure is separate from both. Refresh is the retry.
- Settings: the key count comes only from the key list. While a newer read is pending, the row
  says that AKM is counting. A failed count says so and offers Try again. Unreadable backup
  details say so. They never say that no backup exists.
- Key detail: the history list reads when it opens and drops its read when it closes. A failed
  history read shows its error and Try again inside the history row.

## Row identity (fault classes M and H)

One module, `desktop/src/rowFacts.ts`, builds the facts that a page draws about a key, as the text
on screen. An unset value draws its fallback: "No project", "No environment" or "No issuer".

Fault class M: two different keys drawn the same. Review 1 of this unit found two pairs that the
look-alike check missed: issuers that differ by a repeated space, and an issuer that spells a
scope count beside a real scope (U2-R1-01). The check compared each value alone, not the text
that the row draws from the values together. So the check is gone. Every key carries its short
ID wherever the page draws it, and a value draws so that it cannot hide a character or imitate
the row. This also covers LOOK-R4-01 (an unset value beside a literal value that reads like its
fallback) and LOOK-R4-03 (rows that differ only by the scope count, the renewal or the expiry):
such rows now differ by their IDs as well.

### How a stored value draws

`drawnText` draws a stored value. The stored value does not change: a form edits it, a search
matches it, and native code gets it as it is.

- A character that draws nothing, moves the text around it or breaks the line draws as its code
  point, for example ⟨U+200B⟩. These are the controls (a newline, a tab), the format marks (a
  zero-width space or joiner, a directional mark, override or isolate, a byte order mark, a soft
  hyphen), the line and paragraph separators, every space other than U+0020, and every other
  character that Unicode marks as ignorable (a Hangul filler, a variation selector).
- ⟨ and ⟩ draw as their code points, so a value cannot imitate a mark. The middle dot draws as
  its code point, so a value cannot imitate the separator between two facts on a row.
- A U+0020 space draws as a space only between two other characters. A leading, a trailing or a
  repeated space draws as ⟨U+0020⟩. So a screen reader that folds spaces still hears two values
  apart.
- Two different values never draw the same text. `rowFacts.test.ts` tests 23 cases, this rule
  over the cases, and a drawing of every character below U+3000, which keeps no character of the
  kinds above.
- Notes keep their line ends as lines (`drawnLines`), and each line draws by the same rule.
- A drawn value sits in a `<bdi>` element, so a right-to-left value cannot reorder the text
  around it.
- Letters that look alike in two scripts draw as they are. The short ID tells such keys apart.

### The short ID

- Every key row shows its short ID. The short ID is the shortest prefix of at least 8 characters
  that no other ID that the page knows starts with. An ID that is the start of another ID draws in
  full. Two different IDs never draw the same text.
- The page knows every ID that a view listed in this session: the live keys, the Recycle Bin and
  the journal. So a recycled key can make the short ID of a live key longer, and every open view
  draws the longer ID at once. A new session starts with no known IDs. `useShortIds` is the one
  source of short IDs.
- The ID is quiet: muted, monospace and small (`.key-id`). At 900 px it shows in full inside its
  row, in both themes and with forced colours on and off (`row-identity.spec.ts`). It is text, so
  forced colours keep it (class J).
- The Recycle Bin draws the full ID (decision 6). The key glyph is not a fact (decision 10).

### Who draws the facts

- A key row draws its title, issuer, scope count, short ID, variable, environment, renewal, expiry
  and state from `keyRowFacts`. Its group heading draws the project from the same facts.
- The accessible description of a row is built from the same facts: project, environment,
  variable, issuer, scope count and ID. Then the expiry and the rotation status follow, in words.
  The name stays "Open <title>" (FR-19).
- The shelf, the group headings and the environment filter use the drawn text. An unset project
  and a literal project "No project" share one shelf and one heading. A filter option draws its
  stored value, and keeps the stored value as its value.
- The search matches the stored values and the drawn values, the short ID included.
- A project page title and a group heading say "Project" before the name, and a project shelf
  button reads as "Project <name>". So a project named "All keys" or "Needs attention" differs
  from that shelf on screen and by its accessible name (decision 19).
- A Recycle Bin row draws its facts from `binRowFacts`: the identity facts, the renewal, the
  expiry, the full ID, the removal time and the history count. Its buttons are described by the
  same facts.
- An Activity row draws the title, the short ID, and the project and the environment of its key
  from `journalKeyFacts`. A record of the vault draws "Vault" and no ID.
- A notice and the attention summary name a key as its title in quotes, then its ID in brackets,
  through `namedKey` (decision 20).
- The key drawer draws its title, issuer, scope names, tags, URL and notes by the rule
  (decision 21).
- Every `.label` wraps. A 1,000-character issuer made the key drawer 9,879 px wide before.

### Every dialog that acts on a key (U2-R1-02)

Fault class M in a dialog: the move to the Recycle Bin and the rotation listed the identity facts
only, so two keys that differ only by the expiry or the renewal gave the same dialog. The rotation
dialog also had no accessible description. `KeyTarget` in `desktop/src/components/FactList.tsx`
now lists every fact of a key row from `keyFactList`: project, environment, variable, issuer,
scope count, renewal, expiry, status and short ID. The dialog names the list with
`aria-describedby`, so a screen reader reads it. A confirmation dialog describes itself with its
body, which holds the list (UI-R2-01, LOOK-R3-01, LOOK-R4-04).

| Dialog | Lists | Described by |
|---|---|---|
| Move to the Recycle Bin | the title and `KeyTarget` | its body |
| Rotation | the title and `KeyTarget` | `KeyTarget` |
| Edit form | "This key": the title and `KeyTarget` (decision 22) | "This key" |
| Key drawer | `KeyTarget`, then the details | `KeyTarget` |
| Permanent delete | the title and `binFactList`: the bin facts with the full ID | its body |
| Restore | no dialog | the button, by `binRowDescription` |

`KeyTarget.test.tsx` gives each of the four key dialogs, the permanent delete and the restore two
keys that differ only by the expiry, only by the renewal or only by the ID. The visible text and
the accessible description differ, and the action sends the ID of the selected key.
`row-identity.spec.ts` tests the move confirmation at 900 px in both themes.

### The guard

`desktop/src/rowFactsGuard.test.ts` (removed on 13 Nov 2026; see the note at the top) read the page files with the TypeScript type checker.

- A read of a drawn field outside `rowFacts.ts` fails: `title`, `issuer`, `project`,
  `environment`, `envVar`, `resolvedEnvVar`, `url`, `notes`, `scopes` and `tags` of a `KeyView`,
  and `keyTitle`, `project` and `environment` of an `AuditEntry`. A read by property, by a string
  index and by destructuring all count.
- A table allows four files their reads, with a reason and a count for each field: the edit form
  starts from the stored values, Use keys selects by the stored project and environment, and the
  key list and the attention groups sort by the stored title. A new read in one of those files
  changes the count, so it fails too.
- A function that reads the title of row facts (`KeyRowFacts.title`, `BinRowFacts.title`,
  `JournalKeyFacts.name`), or draws a `<KeyCut>` glyph, must also read the `id` of the row facts,
  or call `namedKey`, `keyFactList`, `binFactList`, `keyRowDescription`, `binRowDescription` or
  `KeyTarget`.
- Planted files prove each rule: a list that reads raw values by property, by string index and by
  destructuring; a row that draws a title or a glyph with no ID; and one more raw read in the edit
  form. On the tree before this change the guard failed on 20 fields in 6 files.

## The terminal outcome and the two new notices (contract with unit 1)

The contract is in `docs/design/gate-campaign.md`, under "Contract between units 1 and 2".

- `OperationNotice.kind` has six new values: `filePermissions`, `journalLineageUnconfirmed`,
  `terminalStarted`, `terminalSessionEnded`, `terminalFailed` and `terminalDeclined`. Each has
  its own text in `App.tsx`.
- The file-permission notice says that anyone who can read the vault folder can copy the
  encrypted vault and its backups. It says nothing about backup retention.
- The lineage notice says that the vault file changed outside AKM, or that the journal mapping of
  this device was started again (FV-01), and that its activity continues in the journal that AKM
  keeps for this path. It does not say that a change was saved: the core sets this warning on
  any journal entry, and a reveal or a copy writes one too.
- `api.launchTerminal` sends a fresh UUID as `operationId`, as `api.importEnv` does. The terminal
  receipt starts before the request leaves, with that ID.
- A reply acknowledges the native record: a count, a decline and a native error alike. A
  transport failure keeps the record, because it may be the only truth. The next section says
  which rejection is a native error.
- When the reply is lost and the page is still open, the next warning poll folds the record into
  the waiting receipt: started is completed, session ended is completed while locked, failed is an
  error, and declined is a cancellation. A record settles only a receipt of its own operation.
- After a reload no receipt waits, so the record stays a notice. The notice names the terminal.
  For started and session ended it says that the terminal keeps the keys until it is closed. For
  failed and declined it says that no terminal from that request runs. The notices are dismissed by
  their IDs and do not come back.
- A record from session A settles the receipt of session A. The loss signal names the generation
  of session A, so a newer session B stays open (contract 5).
- Each terminal receipt says only what is known. A pending launch may already hold keys. An error
  does not confirm that no terminal runs.

### One owner for a dispatched operation (U2-R1-03)

The receipts in `desktop/src/sessionBoundary.ts` are the one owner of the state of a dispatched
operation: a terminal launch, an import and a file export. A receipt has an outcome and says who
settled it: the reply, the native record, or a failure in transport. A form keeps no waiting flag
of its own. It waits while a receipt of its operation, made in the current session, is pending.
It shows a reply only when the reply settled the receipt, and the outcome text when the record or
a transport failure settled it. A preview and a copy have no receipt and no record, so their
`busy` flag stays in the view: native refuses a second confirmation window while one is open
(`busy`).

| Row | Events, in order | Receipt | The form waits | The form shows | The record |
|---|---|---|---|---|---|
| 1 | the reply | settled by the reply | until the reply | the reply: a count, a decline or a native error | acknowledged by the reply |
| 2 | the record, and no reply | settled by the record | until the record | the outcome of the record | acknowledged when it settles the receipt |
| 3 | the record, then a late reply | settled by the record | until the record | the outcome of the record; the late reply changes nothing | acknowledged |
| 4 | the reply, then the record | settled by the reply | until the reply | the reply | the reply acknowledged it, so it is dropped |
| 5 | neither | pending | while the session lasts | that it waits for the native windows | kept by native |
| 6 | a reload between | gone with the page | a new page does not wait | nothing | stays a notice until the user dismisses it |
| 7 | a transport failure | `lost` | until the failure | that the window got no answer | kept, and a notice of its own |
| 8 | a lock between | pending, or settled by the reply as completed while locked | the view unmounts; a new session does not wait for an older one | nothing | settles only the receipt of its own session (contract 5) |
| 9 | a second launch | a new receipt | for its own receipt only | its own outcome; a late reply of the first changes nothing | each record settles only its own receipt |
| 10 | a hide, an expiry or a change of view while it waits | pending | yes: the button stays disabled until the receipt settles | nothing of the reply, which may be protected | unchanged |

`UseKeys.operations.test.tsx` tests each row with the real api, boundary and receipts over a mocked
IPC. `terminal-outcome.spec.ts` tests row 2 in Edge for the terminal and for the import. Before
this change, rows 2, 3, 7, 9 and 10 failed: the form waited until the reply, so a record left it
waiting for ever, and a hide ended the wait while the native window was still open.

The sweep of the class: only `env_import` and `env_launch_terminal` have a native record, and both
now wait on their receipt. A file export has a receipt and no record, and it waits on its receipt
too. A preview, a copy, a reveal, a journal recovery and the key, bin, settings and unlock actions
have no receipt and no record, so their local wait has no second owner. The preview and the copy
also end their wait on a hide. If the user then asks again while the first native window is
open, native refuses with `busy`.

### The error contract with native code (U2-R1-07)

The one boundary function, `invoke` in `desktop/src/native.ts`, decides where a rejection came
from. This is the contract for the native unit.

- A native command rejects only with a `CommandError`: a plain object with the two string fields
  `kind` and `message`.
- `kind` is one of the 25 values of `NATIVE_ERROR_KINDS` in `desktop/src/types.ts`: ambiguous,
  busy, changedOnDisk, clipboard, clipboardLost, clipboardNotReleased, clipboardUncertain,
  completedWhileLocked, config, internal, invalid, invalidPath, io, journal, launch, locked,
  notFound, open, reviewFailed, save, sessionUnavailable, stalePreview, tooLarge, upgradeRequired
  and viewChanged.
- A new kind goes on that list in the change that adds it to native code. The test
  `desktop/src/nativeErrorKinds.test.ts` (removed on 13 Nov 2026; see the note at the top) read `desktop/src-tauri/src`. It fails when the two lists
  differ, when native code builds a kind in a way that the scan cannot read, and when a command
  rejects with another error type. The mocked boundary of the Edge tests keeps to the same list.
- Every other rejection is a transport failure: a string, an `Error` (also one that carries a
  listed kind), an object with an unknown kind or with no string message, an array, null and
  undefined. No native command sent it, so the page cannot know what the request did.
- After a transport failure the page does not acknowledge the outcome record. The receipt has the
  outcome `lost`. Its text says that the window got no answer, that a terminal may hold keys, and
  that a saved-operation notice gives the outcome when AKM recorded one. A record that arrives
  later stays a notice of its own, and it is still there after a reload.
- A transport failure after the session changed is a cancellation, as before, and it keeps the
  record too.
- A caller gets only `kind` and `message`, and nothing else that the rejection carried. A transport
  failure has the kind `transport`. No native command may use that kind.
- The page makes some errors itself after it has a reply: `declined`, `locked` after a session
  change, and `completedWhileLocked` after a lock. `timeout` comes from a deadline around a call.
  None of them passes through this rule.

## Plaintext that the page holds (U2-R1-04, contract 4)

Fault class: a stale value. Before, Use keys bound its export preview to the project and the
environment only. An import changed the values, a list read failed and a retry succeeded, and the
old preview came back without a new request.

Now one mechanism holds every plaintext that comes from the vault.

- `native.ts` seals the reply of each command in `PLAINTEXT_COMMANDS` (the reveal of one key and
  the export preview). The seal is a frozen token with no fields. It carries a stamp that the
  request took when it left: the session, the generation of the vault data, and the scope (the
  key, or the project and the environment).
- `sealed.ts` keeps the text beside the token. Only `native.ts` and `plaintext.tsx` may reach it.
- A view keeps a token in `useSealed(scope)` and draws it with `<Plaintext>`, both in
  `plaintext.tsx`. The text shows only while the session, the data generation and the scope are
  those of the stamp. A value that fails this check is dropped from `sealed.ts` at once, so it
  never shows again. Only a new request shows plaintext again.
- These events make held plaintext stale: a lock or a new session (`invalidateSession`); a change
  of vault data (each command in `DATA_CHANGES`, when it leaves and again when it ends, and an
  import record that settles its receipt); a scope change; a list read that fails or starts
  again, because the view then names no scope; a hide; the 30-second timer; an unmount.
- A reply asked for before such an event arrives stale and does not show. Use keys then says
  that the keys changed before the preview arrived.
- `plaintextGuard.test.ts` kept the class closed with scans that are now removed (see the note at
  the top). It still checks that each plaintext reply reaches its caller sealed, that
  `plaintext.tsx` exports no function that returns the text, and that each mutation is in
  `DATA_CHANGES` or named as one that changes no key. Before the removal: it read the Rust commands, and every command
  whose reply holds a `String` must be in `PLAINTEXT_COMMANDS` or named as not plaintext (a session
  reservation, a saved path). It checks that each plaintext reply reaches its caller sealed, that
  only the two files reach `sealed.ts` (by the import graph, as the read guard does), that
  `plaintext.tsx` exports no function that returns the text, and that each mutation is in
  `DATA_CHANGES` or named as one that changes no key. A view therefore never gets the text, so it
  cannot keep it in its own state.
- Tests: `plaintext.test.tsx` tests the holder. `Plaintext.views.test.tsx` runs the review's
  sequence, a preview reply delayed across an import, a preview asked for while an import is on
  its way, the 30-second hide with fake timers, a revealed secret across a rotation, a reveal
  delayed across a rotation, a reveal asked for while a rotation is on its way, and a lock.

The sweep of the class: the reveal and the preview were the two holders, and both use the
mechanism. A copy writes the clipboard in native code and answers a time or a count. A file save
answers a path. The journal, the history and the notices hold no values. The text that the user
types (the import text, the secret of a new key or a rotation, the vault password) does not come
from the vault, so it is not under the mechanism. Each form clears it when it closes or sends it.

## Smaller findings and their guards

- FR-11: the unlock screen names KDBX 4.1, AES-256 and Argon2id only in create mode. In unlock
  mode it says that a vault you open keeps its own encryption settings, and that AKM changes an
  older format only when you upgrade it. The core test
  `older_formats_require_explicit_upgrade_and_preserve_the_database_and_kdf` proves the second
  claim. `Unlock.promises.test.tsx` checks both modes, also after a switch.
- UI-R2-03, UI-R2-05 and LOOK-R3-03 had unit tests. `row-identity.spec.ts` adds their closing
  tests at 900 px in both themes: an all-unfiled vault, an empty vault, a search that leaves only
  unfiled keys, and the project total with a key that has no expiry date.
- LOOK-R2-04 and LOOK-R3-06 had Edge tests. A mutant of each now proves that the test fails when
  the guarded rule breaks.
- FR-14: the recovery tests now check that the result after an approved recovery still says that
  history before the damage cannot be proved complete, and that nothing on the screen says that
  the old history was verified. A decline and a failure each have their own tone and words.

## Class sweeps

- Class I: the stacking guard in `look-guards.spec.ts` lists every dialog in the app. The
  rotation dialog and the format upgrade confirmation were missing.
- Class J: the marks added in this unit are text or layout. None is drawn with a fill or a shadow
  alone, so none needs a forced-colours rule. The line between the row facts and the details of
  the key drawer is a border, and the forced-colours group of `look-guards.spec.ts` checks it.
- Class M, two keys drawn the same (U2-R1-01): every place that lists or names a key now draws it
  from the row facts, with its ID. In Keys: the rows, the shelf, the page title, the shelf
  buttons, the group headings, the filter options, the notices and the attention summary. In the
  key drawer: the header, the facts, the details and the notes. The rotation dialog, the edit form
  and the move to the Recycle Bin. In the Recycle Bin: the rows, the buttons, the confirmation and
  the notices. The rows of Activity. The options and the texts of Use keys. Judged safe: Use keys
  lists no key, only scopes, and its preview comes from native code. The vault name in Settings is
  not a key. The native windows are class K and
  native code. The format-upgrade and journal-recovery dialogs name no key. The reminder sort
  draws nothing. The fields of the edit form keep the stored values. `rowFactsGuard.test.ts`
  kept the class closed (removed on 13 Nov 2026; see the note at the top).
- Class M in dialogs (U2-R1-02): the move to the Recycle Bin, the permanent delete, the rotation,
  the edit form and the key drawer list every row fact. The restore has no dialog, and its button
  is described by the bin facts. The format-upgrade, journal-recovery and import dialogs name no
  key.
- Class D on the screen: `promiseWords.test.ts` (removed on 13 Nov 2026; see the note at the top) scanned the text of the page for always, never,
  verify, verified, complete, secure, safe, guarantee and "is cleared". Each use names its
  evidence. Two uses had none and changed (decisions 14 and 15).
- Class D on the screen, again (U2-R1-06): an approved text matched as a part, so a second
  promise after an approved sentence passed. The scan now reads the source with the TypeScript
  parser: strings in either quote, template strings (an expression reads as {…}) and the text of
  each JSX element (a child element or an expression reads as {…}). It splits each text into
  sentences. A sentence with a promise word must equal an approved sentence of the same file in
  full. Tests: the review's appended sentence, a promise in a template string and in a
  single-quoted string, an approved sentence in another file, and a longer sentence that holds an
  approved one. With the review's sentence planted, the old guard passed and the new one names it.
- Destructive dialogs: Cancel takes the first focus in the move to the Recycle Bin and in the
  permanent delete, so the first Enter cancels and sends nothing (`Modals.keyboard.test.tsx`).
- Stale values: every view handler that awaits native code acts on the identity it captured (an
  ID or the target item), never on an index. The Recycle Bin disables all its row actions while
  one runs, so one finally block cannot free a newer action.

## Decisions for the owner's veto

Each decision takes the option that removes state or removes a promise.

1. Settings shows no key count from the status poll. Before, it showed the polled count while a
   list read was pending and after a list read failed. Two sources gave two different counts
   (UI-R2-02, LOOK-R3-02).
2. A failed refresh drops the data that the view showed before. The view shows the failure and
   no old rows.
3. A failed history read no longer goes to the drawer notice. It shows inside the history row,
   with Try again. Show earlier versions is not offered again until the list closes.
4. Activity keeps one Refresh control as the retry for a failed journal read.
5. The unmount check inside the reply path stays, although no test can see it: React drops an
   update to an unmounted view. The checks in `reload` and `apply` are tested.
6. The Recycle Bin keeps the full ID on every row, as the owner chose after look review 3. The
   key list draws a short ID on every row (U2-R1-01). A bin row also draws its renewal and its
   expiry now, so the permanent-delete confirmation lists them (U2-R1-02). No fact leaves a screen
   that showed it (class H).
7. Every key row reads its short ID to a screen reader, as it draws it. Before this unit every row
   read its full ID. After review 1 of this unit only a look-alike row read an ID. The description
   says what the screen says.
8. A key row without an environment draws "No environment". Before, it drew a dash. The dash
   was not a word, and a literal dash looked the same.
9. The environment filter offers "No environment" when a key has none. It finds the rows that
   draw that text.
10. The key glyph is not a fact. Two keys can share a glyph, so the identity never relies on it.
    The test pairs use IDs that draw the same glyph, and their rows still differ.
11. A terminal launch that the user declined leaves a cancellation receipt, as an import and a
    file export do. Before, it left no receipt. The brief folds a declined record like an import,
    so the reply and the record give the same outcome.
12. A terminal launch that answers with an error says "This does not confirm that no terminal
    runs." The contract says that a failed record means that no terminal runs, but it says nothing
    about an error reply. The receipt keeps the weaker claim until unit 1 confirms the stronger one.
    The record notice for `terminalFailed` keeps the contract's claim.
13. The section and its button keep the name "saved-operation notices", which
    `tools/verify_windows_app.ps1` uses. Terminal notices appear there too.
14. The unlock headline reads "Your keys, one place." Before, it read "one safe place". The threat
    model names attacks that AKM does not stop, so "safe" was a promise with no evidence. This is
    wording, and the owner may want other words.
15. The secret field of the Add key form says "Encrypted inside the KDBX entry." Before, it also
    said "It is never written to settings or logs." No test proves that sentence.
16. A form waits for its receipt also after a hide, a view expiry or a new mount (U2-R1-03).
    Before, a hide ended the wait while the native window was still open, so the user could start
    a second request. Now Save .env… says "Saving…" and Import .env… stays disabled until the
    reply, the record or a transport failure settles the receipt.
17. A preview or a revealed secret goes when the vault data changes: an add, an edit, a rotation,
    a removal, a restore, a purge, an import, an upgrade or a reload, by any view (U2-R1-04).
    Before, only a scope change, a hide, the timer and an unmount hid them. A preview that arrives
    after such a change does not show, and Use keys says that the keys changed before it arrived.
18. A stored value draws by the rule of `drawnText` (U2-R1-01). A character that draws nothing,
    moves text or breaks a line draws as its code point. So do a leading, a trailing or a repeated
    space, ⟨, ⟩ and the middle dot. Before, the page drew a value as it was, so two different
    values could draw the same text, and a newline or a directional mark could move the text of
    the row. The stored value does not change.
19. A project page title says "Project" above the name, small and muted. A group heading says
    "Project" to a screen reader only, and a project shelf button reads as "Project <name>".
    Before, a project named "All keys" or "Needs attention" gave its shelf the same page title as
    that shelf. "No project" gets no label.
20. A notice and the attention summary name a key as its title in quotes, then its ID in brackets.
    Before, they named the title only, so two keys with one title gave the same notice.
21. The key drawer calls the variable "Variable", as every other fact list does, and the scope
    chips "Scope names", because its fact list already has "Scopes" for the count. Its fact list
    draws an unset value as its fallback, such as "No issuer". Before, the drawer said
    "Environment variable" and "Scopes".
22. The edit form has a section "This key" with the title and every row fact, and the form is
    described by it. Before, the form showed only its fields, so two keys with the same values gave
    the same form. The fields keep the stored values.

## Limits

- The mechanism cannot stop a native command that already ran. It only decides what the screen
  shows. The outcome records in `native.ts` still own what a late mutation reply means.
- The read guard reads the import graph of `desktop/src`. A computed module name or an import glob
  counts as a reach of every module, so it fails. The guard does not see code that a file builds
  from text at run time (`eval`, `new Function`). It does not check what the four allowed files do
  with the reads.
- The error-kind scan reads Rust source text with patterns, not a Rust parser. It reads a kind
  given as a literal, as a constant of the same file, or passed on from another error. It reports
  every other form, so a new form fails the test until the scan learns it.
- The plain-object test uses the type tag of the object, so it holds across browser realms. The
  real Tauri IPC was not run here: the claim that it rejects with a plain object comes from the
  mocked boundary and the IPC design, not from a run.
- The state of a key row is not varied alone in the fact table. It comes from the expiry and
  the renewal, which are varied.
- A short ID is unique among the IDs that the page knows in this session. An ID that no view
  has listed yet does not make a short ID longer, so a short ID can be the start of a key that the
  page has not seen. A short ID can grow while a view is open, when another view lists an ID with
  the same start.
- Inside a sentence, such as a notice, a title can imitate the text of the sentence around it.
  The real ID in brackets follows each name. A notice is plain text, so a right-to-left title is
  not isolated there as it is in a `<bdi>`. It holds no directional control, because those draw as
  their code points.
- Letters that look alike in two scripts draw as they are. Only the short ID tells such keys
  apart.
- The row-facts guard does not see a value that leaves a key through a spread, `Object.values` or
  `JSON.stringify`. No page file does that today.
- The plaintext mechanism decides what the page shows. It cannot take back a value that native
  code already put on the screen, and it does not wipe memory: a dropped string stays in memory
  until the JavaScript engine frees it, as before.
- In today's screens a list read starts again only after a change of vault data, which already
  makes held plaintext stale. So the rule that a list read that starts again names no scope has no
  test of its own on a screen. `plaintext.test.tsx` tests that a view that names no scope drops its
  value. Two mutants of that rule (Use keys and the key drawer) pass every test for this reason.
- The Edge tests use a mocked native boundary. They prove the page side of the contract. The
  native record of a terminal launch needs the native tests of unit 1 and the owner-present proof.
- The promise-word scan reads strings, template strings and JSX text in `desktop/src`. It does not
  read text that the page joins at run time from parts in separate places (a constant joined in a
  function), text from the vault or from native errors, or the texts of the native windows in
  `desktop/src-tauri`. A sentence break is a full stop, a question mark or an exclamation mark
  before a space, so two sentences with no space between them count as one.
