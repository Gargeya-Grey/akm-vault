# Frontend reads and rows (unit 2 of the gate campaign)

This note records how the desktop frontend reads native state and how it draws a key row. It
also lists the decisions that the owner can veto. The campaign plan is
[gate-campaign.md](gate-campaign.md).

## The read mechanism (fault classes K and L)

One hook, `useNativeRead` in `desktop/src/nativeRead.ts`, owns every read of native state that a
view shows. A view names a read from the `nativeReads` table. Each entry of that table is a frozen
token that only `useNativeRead` can run, so a view cannot call a read itself.

The reads live in `reads`, in `desktop/src/native.ts`, beside the one boundary function `invoke`.
`api` in `desktop/src/api.ts` holds only actions. Only four files may import `native.ts`, and
only `native.ts` may import the Tauri IPC (U2-R1-05). The guard test `desktop/src/readGuard.test.ts`
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
| Settings | reminder setting | mount, retry | the reply of a saved setting (`apply`) |
| Key detail | history of one key | the history list opens, retry | close and open again, another key, view expiry |
| App | status | the 3 second poll and events | allowed: the poll keeps its own request, epoch and transition identity |
| App | warnings | mount and the 3 second poll | allowed: the reply arrives through the event that `native.ts` owns |
| Attention notifier | reminder setting, key list | a 30 second timer, focus, blur, visibility | allowed: it shows nothing and stops when it unmounts |

The guard test names the four files that may import `native.ts`, each with its reason: `api.ts`
for the actions, `nativeRead.ts` for the mechanism, `App.tsx` for the two polls, and the attention
notifier. It also fails when an allowed file no longer reaches its module, so the list cannot grow
stale. A new member of `reads` or of `api` must be classed as a read or an action, or the guard
test fails.

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

One module, `desktop/src/rowFacts.ts`, builds the facts that a row draws, as the text on screen.
An unset value draws its fallback: "No project", "No environment" or "No issuer".

- A key row draws its title, issuer, scope count, variable, environment, renewal, expiry and
  state from `keyRowFacts`. Its group heading draws the project from the same facts.
- The look-alike check compares the drawn text of every fact. Two rows that match in every
  drawn fact both show their short unique ID. A row that differs in any drawn fact shows none.
  An unset value and a literal value that reads like its fallback draw the same text, so the two
  rows count as look-alikes (LOOK-R4-01). The scope count, the renewal and the expiry count too
  (LOOK-R4-03).
- The short ID is the shortest prefix of at least 8 characters that no other ID in the list
  starts with. Two different IDs never draw the same text.
- The accessible description of a row is built from the same facts: project, environment,
  variable, issuer, scope count, and the ID when the row shows one. Then the expiry and the
  rotation status follow, in words. The name stays "Open <title>" (FR-19).
- The shelf, the group headings, the environment filter and the search use the drawn text. An
  unset project and a literal project "No project" share one shelf and one heading.
- Every dialog that names a key lists its identity facts, and its ID when the row shows one: the
  move to the Recycle Bin, the rotation, and the key drawer. The confirmation dialog describes
  itself with its body, so a screen reader reads those facts (UI-R2-01, LOOK-R3-01, LOOK-R4-04).
- A Recycle Bin row draws its facts from `binRowFacts`, with its ID on every row. Its buttons and
  the permanent-delete confirmation are described by the same facts.
- Every `.label` wraps. A 1,000-character issuer made the key drawer 9,879 px wide before.

## The terminal outcome and the two new notices (contract with unit 1)

The contract is in `docs/design/gate-campaign.md`, under "Contract between units 1 and 2".

- `OperationNotice.kind` has six new values: `filePermissions`, `journalLineageUnconfirmed`,
  `terminalStarted`, `terminalSessionEnded`, `terminalFailed` and `terminalDeclined`. Each has
  its own text in `App.tsx`.
- The file-permission notice says that anyone who can read the vault folder can copy the
  encrypted vault and its backups. It says nothing about backup retention.
- The lineage notice says that the vault file changed outside AKM and that its activity continues
  in the existing journal. It does not say that a change was saved: the core sets this warning on
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
  `desktop/src/nativeErrorKinds.test.ts` reads `desktop/src-tauri/src`. It fails when the two lists
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
  alone, so none needs a forced-colours rule.
- Class D on the screen: `promiseWords.test.ts` scans the text of the page for always, never,
  verify, verified, complete, secure, safe, guarantee and "is cleared". Each use names its
  evidence. Two uses had none and changed (decisions 14 and 15).
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
   rule "an ID only for look-alikes" applies to the key list. The bin row stays as it was, so no
   fact leaves a screen that showed it (class H).
7. A key row reads its ID to a screen reader only when it draws one. Before, every row read its
   full ID. The description now says what the screen says.
8. A key row without an environment draws "No environment". Before, it drew a dash. The dash
   was not a word, and a literal dash looked the same.
9. The environment filter offers "No environment" when a key has none. It finds the rows that
   draw that text.
10. The key glyph is not a fact. Two keys can share a glyph, so the check never relies on it.
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
- A project named "All keys" or "Needs attention" gives its shelf the same page title as that
  shelf. The shelf buttons stay apart by their place under "Projects". This is not fixed here.
- Until unit 1 maps the two core warnings to their own kinds, native still sends them as
  `backupRetention` and `journal`, and the page shows those texts.
- The Edge tests use a mocked native boundary. They prove the page side of the contract. The
  native record of a terminal launch needs the native tests of unit 1 and the owner-present proof.
- The promise-word scan reads double-quoted strings, template literals and JSX text in
  `desktop/src`. It does not read single-quoted strings, text built from parts at run time, or the
  texts of the native windows in `desktop/src-tauri`.
