# Frontend reads and rows (unit 2 of the gate campaign)

This note records how the desktop frontend reads native state and how it draws a key row. It
also lists the decisions that the owner can veto. The campaign plan is
[gate-campaign.md](gate-campaign.md).

## The read mechanism (fault classes K and L)

One hook, `useNativeRead` in `desktop/src/nativeRead.ts`, owns every read of native state that a
view shows. A view names a read from the `nativeReads` table and never calls a read of `api`
itself. The guard test `desktop/src/readGuard.test.ts` fails when a file does.

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
| App | warnings | mount and the 3 second poll | allowed: the reply arrives through the event that `api.ts` owns |
| Attention notifier | reminder setting, key list | a 30 second timer, focus, blur, visibility | allowed: it shows nothing and stops when it unmounts |

The three allowed callers are listed in the guard test with their reasons. The guard test also
fails when an allowed caller no longer calls a read that the list names, so the list cannot grow
stale. A new member of `api` must be classed as a read or an action, or the guard test fails.

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

## Limits

- The mechanism cannot stop a native command that already ran. It only decides what the screen
  shows. The outcome records in `api.ts` still own what a late mutation reply means.
- The guard test reads source text. It finds `api.<read>` calls, an index into `api` and a
  destructured `api`. It does not follow an alias that another module exports.
- The state of a key row is not varied alone in the fact table. It comes from the expiry and
  the renewal, which are varied.
- A project named "All keys" or "Needs attention" gives its shelf the same page title as that
  shelf. The shelf buttons stay apart by their place under "Projects". This is not fixed here.
