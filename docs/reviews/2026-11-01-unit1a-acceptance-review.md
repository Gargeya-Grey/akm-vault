# Unit 1a acceptance review, after the repair (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: branch `fix/unit1a-sound-findings` at `8ff78f8` (application source `2664e51`), judged against the repair design revision 3 (`docs/design/unit1a-repair.md`), as the owner decided when the work was split into units 1a and 1h
- Build hashes checked by the reviewer (all matched STATE.md, before and after its probes): `akm-desktop.exe` 243b5997…f48b, `akm.exe` 8cfd85c7…3819, MSI d1d6af12…e978, NSIS 5caeb532…b817
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 4 findings (all P3), recorded as 1A-A1-01 to 1A-A1-04 in FINDINGS.md. This is the ninth rejection of unit 1a. Patching stops; the result goes to the owner (STATE.md).

The text below is the reviewer's report. The reviewer's links to repository files became plain text with the repository path and line. Two links to files in the reviewer's local temporary folder (a preimage audit and a handoff receipt) became "(local temporary file, not kept)". Other differences cannot be excluded.

---

**Ranked findings**

1. **P3: A failed copy can falsely say the earlier copy is still on the clipboard.** `desktop/src-tauri/src/clipboard.rs:327` checks only whether AKM retains a lease. If the user replaced copy A and a later copy B fails before opening the clipboard, the error nevertheless says A "is still on the clipboard." **Reproduced** with the production lease code and a fake backend. Revision 3 prescribes this sentence, but it conflicts with the design's requirement to report only what was observed.
   **Closing test:** Through `Session::copy_stamped` and the fake Win32 publisher, replace A with a user copy, then fail B before publication. Require uncertainty wording and preservation of the user copy.

2. **P3: A failed copy can report cleanup failure after cleanup succeeded.** `desktop/src-tauri/src/clipboard.rs:311` attempts immediate cleanup after a failed rollback, then unconditionally says it "could not remove the partial copy." **Reproduced** with the production lease code and a fake backend that successfully clears it. The real-publisher test table also expects this message when its fake clipboard is empty, by inspection.
   **Closing test:** Through `copy_stamped`, fail a secret-format write and rollback, then allow immediate cleanup to succeed. Assert the resulting message reflects that outcome; retain a separate busy-cleanup case and warning.

3. **P3: The required Session cleanup matrix is incomplete.** `desktop/src-tauri/src/session.rs:3073` tests a changed-sequence cleanup failure at a due tick; the later lock occurs after the lease is gone. Zero-sequence and unreadable cases are tested directly on the lease, rather than through both Session cleanup paths. The production-caller table also checks warning text without asserting the specified sequence values and warning IDs. **Found by inspection; this is a coverage gap, not a reproduced lock defect.**
   **Closing test:** Run the revision 3 cleanup cases through the real publisher over fake Win32 at both tick and lock, asserting sequences, lease state, warning IDs, retries and preservation of a newer identical copy.

4. **P3: The action-name test does not check the documented order.** `crates/akm-cli/tests/security.rs:1935` converts README action names into a set. A README change that keeps the same names but says export completion precedes publication would still pass. The current implementation and README ordering are correct by inspection.
   **Closing test:** Check the documented ordering and instrument the export sink so the caller test observes the request record before the first byte and the completion record only after publication.

**Source acceptance: NO.** The two copy-error paths make unsupported claims about clipboard state, and the prescribed caller-level coverage has gaps.

**Work unit complete: NO.** In addition to the source findings, R4-04 remains deferred under the owner's decision. The real clipboard test and full native GUI copy proof were not run because they require the owner's presence. KeePassXC interchange was not run on this build. The earlier retained reader proof used a different CLI artifact and different audit/dependency inputs, although the vault source matches that earlier record.

**What the review did not cover**

I did not launch the GUI, touch the real clipboard or settings, change the Windows session, run full builds or suites, or rerun the affected-version and mutant campaigns. I did not exhaustively inspect every historical test and unchanged caller in the whole-unit diff. Units 1c and 1h remain outside this review.

A targeted fake-vault check of the pinned release CLI passed the fixed-mask and damaged-journal fail-closed cases. The checkout was clean at `8ff78f8`, with reviewed source `2664e51fe138dfa449e6eb99791349e14e46dab0`. The four artifacts matched these SHA-256 hashes before and after the probes:

```text
akm-desktop.exe  243b5997bb98d1c537834aa05ee1c3fad2e8b148a7ffdf083968a2dacfc8f48b
akm.exe          8cfd85c702ccbc1168844745698a318d618315f78aec0b2053887a57f5c93819
MSI              d1d6af127d96a915a5611667c5cb2e89a0554a6f34b1637ff9f1b9795880e978
NSIS             5caeb532e8061b9f04ac0a166ae96993cc84f0aa8a897d34720b52d644b1b817
```

All 139 archived baseline-manifest hashes matched. The scoped snapshot diff contains no deletions; pre-unit preimages for 22 added-looking paths and deletions outside the recorded inventories remain unproved. The paths are recorded in the preimage audit (local temporary file, not kept).

The shared observer requires a project-local artifact root and registry writes, which conflict with this temporary-folder-only review. The task-local handoff receipt (local temporary file, not kept) and fake probe artifacts are retained. No project files were changed.
