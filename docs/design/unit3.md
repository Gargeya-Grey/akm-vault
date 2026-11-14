# Unit 3: dotenv, time and CLI selection

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed (the independent review follows the whole chain). The owner may veto any decision under "Decisions for the owner's veto".
- Tier: `Tier 2: 6 units, risk: policy, privacy, public API, delete`. The units: reserved variable names, the dotenv profile, one instant for expiry, live states and the monotonic reveal clock, exact CLI selection, and the visible export cleanup failure.
- Scope: M5, L3, L6, P2-TIME-REFRESH, P3-REVEAL-CLOCK, L2, 1A-D2-09 and the rest of L1 ([FINDINGS.md](../FINDINGS.md)), plus two rows found while reading the source for this unit: 3-SA-01 (reserved variable names) and 3-SA-02 (dotenv size limits).
- Inputs kept: [unit 1c](unit1c.md), [unit 1h](unit1h.md) (every secret sink needs a proof from the journal), [unit 2](unit2.md).
- Public API change: yes, inside the workspace crates and the desktop IPC.
  - `akm-core`: `ApiKey::rotation_due_in` is removed. New `ApiKey::rotation_due_at`, `rotation_remaining` and `expiry_remaining`. New `schedule` module (`parse_expiry`, `end_of_day`, `Remaining`, `rotation_due`). New `model::reserved_class` and `ReservedClass`. New `dotenv::MAX_BYTES`, `MAX_LINE_BYTES`, `MAX_ENTRIES`; `dotenv::parse` refuses more input. `Vault::find` and `Vault::find_recycled` select by id or exact title only. New `Vault::candidates` and `Vault::recycled_candidates` (metadata only). The text of `Error::Ambiguous` changed.
  - `akm-cli`: `list` and `get` show the remaining time as "in 3d", "due now", "overdue" or "2d overdue". `--expires YYYY-MM-DD` now stores 23:59:59 UTC (it stored 00:00:00 UTC). Every selector refusal lists candidates. A failed file export can say "could not confirm cleanup".
  - `akm-desktop` IPC: `KeyView` drops `expired` and `rotationDueIn` and gains `rotationDueAt`. The page computes the states from the due times and a clock.
  - Not changed: the KDBX format, the journal format, the disclosure capability, the export file format.

## 1. Facts the design uses

- python-dotenv 1.2.2 is the reference reader. `parse_stream` was run with no interpolation. The version and the results are recorded in `crates/akm-core/tests/fixtures/dotenv/expected.json`.
- The pre-unit parser was a line splitter. It kept `# note` in `TOKEN=abc # note`, accepted any name, skipped malformed lines, and had no size limit.
- The CLI stored `--expires 2026-11-10` as 2026-11-10T00:00:00Z. The page stored the same date as 23:59:59Z.
- Rotation days came from a truncating division in Rust and from a rounding up in the page.
- `Vault::find` fell back to a substring search over title, issuer, project, environment, username, URL, notes, tags and scopes. A blank query matched every key, so a vault with one key gave that key to `get --reveal ""`.
- `ApiKey` has no secret field. `Vault::list`, `get_by_id`, `find`, `candidates`, `recycled` and `expiring` read no secret (unit 1h, D9).

## 2. Decisions for the owner's veto

### D1. The external dotenv profile (M5)

AKM dotenv profile v1. The reference reader is python-dotenv 1.2.2 (no interpolation). Where the two differ, AKM is stricter, and the fixture says why. A secret must never change silently on the way in.

| Rule | AKM |
|---|---|
| Encoding | UTF-8. One leading byte order mark is dropped. |
| Line end | LF or CR LF. A lone CR outside a quoted value is an error. A NUL is an error. |
| Blank and `#` lines | ignored |
| `export NAME=value` | the `export ` prefix is accepted |
| Name | `[A-Z_][A-Z0-9_]*`. Anything else is an error. |
| Spaces | allowed around `=` and at the line ends; not part of an unquoted value |
| Unquoted value | ends at the line end or at a `#` that follows a space or tab (a comment) |
| Single quotes | literal, may span lines, no escapes. A single quote ends the value. |
| Double quotes | decode `\n \r \t \" \\` only. Any other backslash stays as written. May span lines. |
| After a closing quote | only spaces and an optional comment |
| Interpolation, `$(...)`, backticks | none. The text stays literal. The parser runs nothing and reads no environment variable or file. |
| Duplicate name | an error naming the line |
| Malformed line, unterminated quote | an error naming the line. Never a skip. |
| Errors | name the line number. They never repeat file content. A name is repeated only when it is plain letters, digits and underscores, up to 64 characters. |

Where AKM differs from python-dotenv 1.2.2 on purpose (each is a fixture with a note; the recorded reader output in `crates/akm-core/tests/fixtures/dotenv/expected.json` is the source of truth for this table):

| Case | python-dotenv | AKM | Reason |
|---|---|---|---|
| `EMPTYC= # only a comment` (a comment right after `=` and spaces) | keeps `# only a comment` | empty | a shell does this. Both readers give `abc` for `TOKEN=abc # note`. |
| `\a \b \f \v \'` in double quotes | decoded | kept as written | a secret with a backslash and such a letter must not change |
| `'a\\b'` | `a\b` | `a\\b` | single quotes are literal, as in a shell and in Node dotenv |
| `'it\'s'` | reads `\'` as a quote | error | there is no escape in single quotes |
| a name with no `=` | value `None` | error | no silent empty secret |
| a repeated name | last wins | error | no silent overwrite |
| `lower`, `1ABC`, `A-B` | accepted | error | the names must be valid environment variable names |
| a malformed line | skipped | error | no silent loss |
| CR LF inside a quoted value | turned into LF | kept | AKM reads bytes, not text-mode lines |
| a lone CR | a line end | error | an ambiguous line end |
| a leading BOM | kept in the first name | dropped | Windows editors write one |
| NUL | accepted | error | no process can receive it |

The protections that existed before are kept: a duplicate name, an empty secret, the project and environment scope of an import, and the reserved names (D3). Import keeps its all-or-nothing rule: any error leaves the vault unchanged.

Test: `akm-core/tests/dotenv_profile.rs` (8 tests) over 34 fixture files; the writer table is read back by python-dotenv; a test fails when a "differs" note is missing or stale.

### D2. Size limits (3-SA-02)

`MAX_BYTES` 256 KiB for the document, `MAX_LINE_BYTES` 16 KiB for one physical line, `MAX_ENTRIES` 500. A document over a limit is refused before any key is created, with a message that names the limit and the line. Why these numbers: a real `.env` file is a few KiB; a 256 KiB file is already not a dotenv file. The numbers are constants, not settings. The desktop refuses the text before it opens any dialog (`a_hostile_or_oversized_env_text_is_refused_with_a_clear_message_before_any_dialog`).

### D3. The reserved variable list (3-SA-01)

The old check held 9 names and 3 prefixes, compared them case-sensitively (`Path` passed) and ran only when a `run` or `export` was planned. The new rule is one table in `akm_core::model`, used at import, add, edit, plan (`run` and `export`) and in the CLI child builder. Names compare without regard to case, as Windows does. Seven classes, each with a one-line reason shown to the user:

| Class | Names |
|---|---|
| AKM internal | `AKM_*` |
| Process control | `PATH`, `PATHEXT`, `COMSPEC`, `SYSTEMROOT`, `WINDIR`, `PSMODULEPATH`, `LD_*`, `DYLD_*` |
| Startup hook | `NODE_OPTIONS`, `NODE_PATH`, `PYTHONPATH`, `PYTHONHOME`, `PYTHONSTARTUP`, `PYTHONINSPECT`, `PYTHONBREAKPOINT`, `PYTHONUSERBASE`, `PERL5OPT`, `PERL5LIB`, `PERLLIB`, `RUBYOPT`, `RUBYLIB`, `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, `JDK_JAVA_OPTIONS`, `CLASSPATH`, `DOTNET_STARTUP_HOOKS`, `DOTNET_ADDITIONAL_DEPS`, `ELECTRON_RUN_AS_NODE`, `COR_PROFILER*`, `COR_ENABLE_PROFILING*`, `CORECLR_*` |
| Helper program | `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_ASKPASS`, `SSH_ASKPASS`, `GIT_EXEC_PATH`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF`, `GIT_TEMPLATE_DIR`, `GIT_CONFIG_*` |
| Shell startup | `BASH_ENV`, `ENV`, `PROMPT_COMMAND`, `PS4`, `SHELLOPTS` |
| Traffic redirect | `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, `FTP_PROXY`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`, `NODE_TLS_REJECT_UNAUTHORIZED`, `GIT_SSL_NO_VERIFY`, `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH`, `NPM_CONFIG_*`, `PIP_*` |
| User location | `TEMP`, `TMP`, `TMPDIR`, `USERPROFILE`, `HOME`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA` |

Add and edit also check the variable that a key will get when `env_var` is empty (derived from the title), so a title such as "Path" is refused with the advice to set another variable name.

A vault entry that already breaks the rule (made by an older AKM, by import, or by KeePassXC) is not changed or hidden. `run` and `export` refuse it and name the key by title, never by value: `key "Old proxy" cannot be used: "HTTPS_PROXY" is reserved: ...`. The user renames the variable (edit), and the key works again (`a_key_stored_under_a_newly_reserved_name_is_refused_for_use_but_can_be_viewed_and_renamed`).

Rejected: (a) a deny list per command only (it drifts); (b) refusing on the key's title (a title is free text); (c) an allow list of "safe" names (API key names are free: `ACME_TOKEN`).

Limit: this list is not a sandbox. A child that gets a key can still misuse it, and an unlisted variable can still change some tool. The strong fix is a clean child environment (roadmap item 19). The list closes the known ways that a stored value runs code or moves traffic.

### D4. One instant for a date-only expiry (L3, L6)

A date-only expiry means the last second of that date in UTC: `2026-11-10` is 2026-11-10T23:59:59Z. A full RFC 3339 instant is kept exactly. The CLI, the page, the exports and `expiring` (the reminders) all use this. The rule lives in `akm_core::schedule::parse_expiry`, and the page has the same rule in `dateOnlyExpiry`. Both are tested against `crates/akm-core/tests/fixtures/time/cases.json`, so they cannot drift.

Scheduling is exact. Only the display uses days:

| State | Display | Rule |
|---|---|---|
| time left > 0 | "in Nd" | N is the milliseconds left divided by one day, rounded up, so N is at least 1 |
| time left = 0 | "due now" | exactly zero |
| time left < 0 | "overdue" (day 0) or "Nd overdue" | N is the full days late, rounded down |

An expired or overdue key never shows "in 0d". The date text for a date-only expiry is the entered date in every time zone (the page formats it in UTC).

Limit: a vault written by an older CLI holds `00:00:00Z` for a date-only expiry. AKM cannot tell it from an intended midnight, so it does not migrate it. Such a key expires at the start of that date, as before. Edit the key to store the new instant.

Tests: `akm-core/tests/schedule.rs` (7), `akm-cli/tests/time.rs` (3, five `TZ` values), `desktop/src/schedule.test.ts` (6 `TZ` values).

### D5. The states follow the clock (P2-TIME-REFRESH)

The page no longer receives `expired` and `rotationDueIn`. It receives `expiresAt` and `rotationDueAt` and computes the states from a clock (`useNow`). The clock wakes at the next moment that a state or a day count can change, at most every 60 seconds, and again on `visibilitychange` and `focus`. It calls no native command. So a refresh is not user activity (contract 6), cannot extend the idle deadline, and shows nothing the page did not already hold.

Limit: the browser sends no event when the system clock changes. A change is seen within 60 seconds, at the next wake, or at once on a focus or visibility event.

### D6. The reveal countdown uses a monotonic clock (P3-REVEAL-CLOCK)

The 30 second countdown measures `performance.now()`, not the wall clock. A wall clock moved back 40 seconds does not keep the text on screen; a clock moved forward does not hide it early. This fixes the page timer only.

### D7. CLI selection (L2)

Decision: a single key is selected by its entry id or its exact title (any letter case), and by nothing else. This holds for every command that takes a selector: `get` (all fields, masked or revealed), `rm`, `rotate`, `history`, `restore` and `purge`. `run` and `export` do not take a query word; they select by project and environment, or `--all`.

Why not keep the fallback for read-only commands: the masked `get` already prints notes and URL, but a metadata search is an oracle over free text, and one rule is easier to explain, test and keep than two. A refusal does the job of the search: it lists the keys whose metadata contains the word, as id, title, project and environment, at most 12, with the count of the rest. It never prints a secret, a note or a URL. A blank selector has no candidates. Two keys with the same title are refused; the user passes an id.

Rejected: (a) keep the fallback when exactly one key matches (a guess becomes a disclosure or a delete); (b) keep it for read-only commands only (two rules); (c) a `--fuzzy` flag (nobody would need it after the candidate list).

Tests: `akm-cli/tests/selection.rs` (6, through the real binary, with notes-only, URL-only, blank and duplicate-title selectors, and a check that the vault bytes and the journal do not change).

### D8. A failed export cleanup is visible (1A-D2-09)

When a file export fails and the removal of its plaintext temporary file is not confirmed, the error says "could not confirm cleanup", names the temporary file, says it may hold the exported values, and exits 1. If the removal is confirmed, the message is the original error only. "Confirmed" means the operating system accepted the delete request on the open handle. It does not mean secure erasure. The test injects a failed delete (`ExportIo::discard`).

### D9. L1 remainder

`ApiKey` has no secret field; `Secret` prints `Secret(<redacted>)` and serializes to `"***"` (unit 1h). This unit swept every `derive(Debug)` type in the three crates for a field that holds a secret, a password, an environment value or clipboard text, and found none that derives `Debug`. The types that hold plaintext (`Secrets` in the CLI, `FileExport`, `Lease`, `KeyDraft`) do not derive `Debug` or `Serialize`. The new candidate lists use `ApiKey` only. Test: `disclosure.rs::candidate_lists_and_the_recycle_bin_hold_no_secret_bytes`.

No full memory zeroization is claimed. Zeroizing wrappers shorten the life of some buffers. Copies inside the `keepass` library, the clipboard and the child process are not covered.

## 3. Failure cases

| Case | Result |
|---|---|
| `.env` with `A=1` twice | error, line named, nothing imported |
| `.env` with `LD_PRELOAD=x` | error naming the variable and the class reason, nothing imported |
| `.env` over 500 entries or 256 KiB | refused with the limit named |
| Expiry `2026-13-40` | refused with the accepted forms |
| Key expires at 23:59:59 and the list stays open | state flips to "Expired" in that second |
| Machine clock moves 3 days forward | states update within 60 seconds, or at once on focus |
| Wall clock moves back 40 s while a secret is shown | hides at 30 s of real time |
| `akm get alp --reveal` | refused, candidates listed, no journal record, vault unchanged |
| `akm rm ""` | refused |
| Two keys titled "Twin" and "TWIN" | refused by title, listed by id |
| Legacy key with `HTTPS_PROXY` | `run` and `export` name its title and refuse |
| Export fails and the temp file cannot be deleted | "could not confirm cleanup", file named, exit 1 |

## 4. Limits

- The reserved list is a list, not a sandbox (D3).
- A date-only expiry from an older CLI stays at midnight UTC (D4).
- A system clock change is seen within 60 seconds (D5).
- Cleanup "confirmed" means the delete request was accepted, not that the bytes are gone (D8).
- No memory zeroization claim (D9).
- The python-dotenv fixtures prove the profile against one reader. Node dotenv and a second reader are not compared.
