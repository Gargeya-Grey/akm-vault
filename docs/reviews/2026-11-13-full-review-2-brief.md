# Brief of the second full review, 13 November 2026

> Note of 13 November 2026: `<build>` stands for the collector's build folder on the review computer. The reviewers received its real path.

Each reviewer got this text and its row of the table, and nothing from the implementer's chat. Model and effort: Codex GPT-6.1 Sol, max ([REVIEWING.md](../REVIEWING.md)). Two reviewers got the Security row, as in the [first full review](2026-11-13-full-review.md) of the same day: there the two security reviewers found different defects.

---

You are one of several independent reviewers of a full rescore of AKM. You see no other reviewer's work. Do not start subagents.

AKM is a personal Windows API-key manager: a Rust core and CLI, and a Tauri + React desktop app, over a KeePass (KDBX) vault. Its owner will trust it with real secrets only after an independent adversarial review scores it above 90 of 100, followed by the owner's own tests. Your scores decide part of that number, so they must be what the code and the documents earn: not kind, not harsh. The owner also wants to know if the number itself tells the truth about the product, so this brief asks you two questions with no points at the end.

## Your areas

Your task message names your reviewer row. The rows:

| Reviewer | Areas | Points |
|---|---|---:|
| Architecture and premise | A. Premise and positioning; B. Architecture and design | 10 + 15 |
| Security | C. Security at rest; D. Security in use | 15 + 15 |
| Data and correctness | E. Data integrity and durability; F. Functional correctness | 15 + 10 |
| Desktop quality | G. Desktop app quality | 10 |
| Verification and shareability | H. Verification and engineering hygiene; I. Shareability | 5 + 5 |

The baseline review describes each area.

The other areas belong to other reviewers. If you find a defect that belongs to another area, report it in a separate list with no points.

## What to read first (all in your checkout)

1. `docs/REVIEWING.md`, sections "Method" and "Allowed actions". They bind you. The return format of this brief replaces the one in that file.
2. `docs/PROJECT.md`: the goals, the required workflows, the non-goals and trust boundaries, and the contracts.
3. `docs/THREAT_MODEL.md` and `docs/CLAIMS.md`: what AKM says that it does, with the evidence that it names, and the limits that it discloses.
4. `docs/reviews/2026-10-30-baseline.md`: the rubric (nine areas, 100 points) and the first scores.
5. `docs/reviews/2026-11-13-full-review.md`: the last measured scores (83.5 of 100, on an earlier commit), and every deduction of your areas with its points. Use the same scale: there a Low finding cost 0.25 to 0.5 points, a Medium 1 to 2.5, a High 2 to 3. `docs/reviews/2026-11-09-full-rescore.md` is the review before it.
6. `docs/FINDINGS.md`: the status of every finding. "Fixed, not accepted" means that a fix exists and that you must judge it. The rows of the last review are the FV rows; most of them have a fix that no reviewer has read.
7. `docs/TESTING.md` and `docs/STATE.md` ("Destination, measures and path", "Last full check run", "Builds under test").

## Method

- Assume that a defect, or a claim without evidence, exists, and find it. Read the code of your areas, not only the documents.
- Start each area at its full points. Deduct for each defect or unsupported claim that you establish, by its demonstrated effect on a person who uses AKM with real keys, inside the trust boundaries of `docs/PROJECT.md`.
- For each deduction of the last review (`docs/reviews/2026-11-13-full-review.md`) in your areas: try to break its fix with a new scenario. Say "holds" (the points come back), "partly holds" or "does not hold", with your evidence. A deduction with no fix (a disclosed limit, an owner gate) stays, unless you judge its points differently and say why.
- A limit that the documents disclose can still cost points when it is a real weakness for the user. Say so, and say how many points, as the earlier reviewers did.
- After the last review the project removed tests, by a rule of its owner: a test stays only when it fails if a behaviour or a stated contract breaks, and still passes when only the internals change. The removed tests read source text, pinned wording, repeated another test, tested a test helper, or could not fail. `docs/STATE.md` ("Destination, measures and path", step 5) gives the counts and names the rules that no test enforces now, and `git log --grep="do not protect behaviour" --grep="documents named"` lists every removed test with its reason. Judge this as you judge any other change: if a removal left a behaviour or a contract of your areas without protection, that is a finding.
- One root cause costs points once.
- Mark each finding "reproduced" (you ran it with fake data) or "inspection".
- Give each deduction one kind. The kind changes no points. `defect`: the code does a wrong or unsafe thing. `design`: a design or threat-model choice. `missing`: a required workflow of `docs/PROJECT.md` is absent or cannot be completed on a surface. `claim`: a document or a screen says more than the code does. `limit`: a weakness that the documents disclose. `proof`: a behaviour that nobody ran on the real surface. `release`: signing, licence, CI, install, publication.
- Score what is in this commit and in the builds below. Facts that the project states and that you may count: the builds are unsigned; CI (GitHub Actions) has not run since 8 Nov 2026; the tests that need the owner present (real clipboard, real windows, the native import and terminal windows, OS lock and suspend, a real idle wait, install on a clean computer, a real screen reader) were not run.

## Check results that the project reports

The builds are of commit `20b9359`. The later commits of your checkout change documents only. The project reports these results for `20b9359`, all with fake data:

- `cargo fmt --check` and Clippy (`-D warnings`) clean. `cargo test --workspace --offline --locked --no-fail-fast`: 698 passed, 0 failed, 2 ignored. Three tests that need a real symbolic link printed NOT RUN, because the account has no symbolic-link privilege.
- `npm run build` exit 0. `npm test`: 738 passed in 43 files. `npm run test:ui`: 157 passed in headless Edge, with a mocked native boundary. The Tauri release build exit 0; the setup program was built and never installed.
- KeePassXC 2.8.0 interchange and kdbxweb interchange pass against the CLI build.
- 37 CLI steps on the release build pass through the verification skill's helper (create, add, list, masked view, reveal, audit, scoped export, `run`, reserved names, `edit`, a refused `add --secret`, rotate, history, Recycle Bin, restore, purge, a refused reveal when the journal cannot be written).
- Two focused real-window proofs pass on the desktop build: `-JournalOnly` 27 checks, `-JournalExportOnly` 8 checks. They drive the unlock screen, the key list, Activity and Save .env. No other desktop flow has run on a real window.
- `check_docs.py` passes in the fast and the full mode; the tool tests and the no-network check exit 0. `check_dependencies.ps1` exits 0 when it runs alone; inside the full run it failed twice, because `cargo audit` could not finish its lookup of yanked crates in time.

Builds (unsigned, SHA-256), in `<build>\release`:

- `akm.exe` `f0acb4b9ee636dee08c1225988c7c7c82945bbe0beb99a3e1f9fe1c84b0a602c`
- `akm-desktop.exe` `411881ab91bd93520ff9f4c4006f4d74450297adcae812be2743b84aba50823f`
- `bundle\nsis\AKM_0.1.0_x64-setup.exe` `83ae22757ce42b06736a78a321f70907b9d83f7e6a2f56ff55f4b1f12576584c`

A check result is evidence only for the inputs that it ran.

## Allowed actions (hard rules)

- Read-only on the checkout. Do not edit, commit or push.
- Probes with fake data only, in one new folder under `%TEMP%`. Never use a real vault or a real secret.
- You may run the CLI build `<build>\release\akm.exe` on fake vaults in your probe folder. Every run of it, also from a probe program of your own, must set `AKM_JOURNAL_ROOT` to a folder inside your probe folder (`.agents/skills/verify-akm/SKILL.md` explains this rule and has a helper). Never write `%LOCALAPPDATA%\akm\journal\`, and never read or change `%APPDATA%\com.akm.desktop\settings.json`.
- Do not start the desktop app or any window. Do not touch the clipboard. Do not lock, suspend or sign out of Windows. Do not install anything. Do not use the network. Do not start a workflow.
- Do not run the full builds or the full test suites. A single test or a small probe program in your own build folder under `%TEMP%` is allowed.
- Desktop quality reviewer only: you may load the built page in a headless browser on the loopback address, with your own mock of the native boundary. The built page is in `desktop\dist` of the main checkout, whose path your task message gives; read it and its `node_modules`, and write nothing there.
- At the end, delete your probe folder, or name it in your report.

## Return format (one message)

For each of your areas:

1. `Area X: N / max`.
2. A table of deductions: points, severity (Low, Medium, High), kind, the finding in one or two sentences with file and line, "reproduced" or "inspection", and the test or change that would win the points back.
3. A table of the deductions of the last review (13 Nov) in that area: holds, partly holds or does not hold, with one line of evidence.

Then:

4. Findings outside your areas, with no points.
5. What you did not cover.
6. Two answers with no points, in plain words.
   - (a) Think of the part of AKM that your areas cover. Would you keep your own API keys in this build today, when each key can be revoked at its provider: yes, yes with named precautions, or no? Name the one failure that you expect a user to meet first.
   - (b) For each required workflow of `docs/PROJECT.md` that your areas touch: complete, partly complete or missing, on the CLI and on the desktop app. For each, say if you ran it, read its code, or took the project's word.
7. Do your points say the same thing as your answer in 6(a)? If not, say what the points miss and what they overstate.

Write plain sentences. Give no total for the whole project.
