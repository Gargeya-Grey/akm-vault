# Brief of the full review, 13 November 2026

> Note of 13 November 2026: `<build>` stands for the collector's build folder on the review computer. The reviewers received its real path.

Each reviewer got this text and its row of the table, and nothing from the implementer's chat. Model and effort: Codex GPT-6.1 Sol, max ([REVIEWING.md](../REVIEWING.md)). Two reviewers got the Security row, so that the spread between two reviewers is measured once.

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
5. `docs/reviews/2026-11-09-full-rescore.md`: the last measured scores, and every deduction of your areas with its points. Use the same scale: on 9 Nov a Low finding cost 0.25 to 0.5 points, a Medium 1 to 2.5, a High 2 to 3.
6. `docs/FINDINGS.md`: the status of every finding. "Fixed, not accepted" means that a fix exists and that you must judge it.
7. `docs/TESTING.md` and `docs/STATE.md` ("Destination, measures and path", "Last full check run", "Builds under test").

## Method

- Assume that a defect, or a claim without evidence, exists, and find it. Read the code of your areas, not only the documents.
- Start each area at its full points. Deduct for each defect or unsupported claim that you establish, by its demonstrated effect on a person who uses AKM with real keys, inside the trust boundaries of `docs/PROJECT.md`.
- For each deduction of 9 Nov in your areas: try to break its fix with a new scenario. Say "holds" (the points come back), "partly holds" or "does not hold", with your evidence.
- A limit that the documents disclose can still cost points when it is a real weakness for the user. Say so, and say how many points, as the 9 Nov reviewers did.
- One root cause costs points once.
- Mark each finding "reproduced" (you ran it with fake data) or "inspection".
- Give each deduction one kind. The kind changes no points. `defect`: the code does a wrong or unsafe thing. `design`: a design or threat-model choice. `missing`: a required workflow of `docs/PROJECT.md` is absent or cannot be completed on a surface. `claim`: a document or a screen says more than the code does. `limit`: a weakness that the documents disclose. `proof`: a behaviour that nobody ran on the real surface. `release`: signing, licence, CI, install, publication.
- Score what is in this commit and in the builds below. Facts that the project states and that you may count: the builds are unsigned; CI (GitHub Actions) has not run since 8 Nov 2026; the tests that need the owner present (real clipboard, real windows, the native import and terminal windows, OS lock and suspend, a real idle wait, install on a clean computer, a real screen reader) were not run.

## Check results that the project reports

The builds are of commit `d3b6fb1`. The later commits of your checkout change documents only. The project reports these results for `d3b6fb1`, all with fake data:

- `cargo fmt --check` and Clippy (`-D warnings`) clean. `cargo test --workspace --offline --locked --no-fail-fast`: 698 passed, 0 failed, 2 ignored.
- `npm run build` exit 0. `npm test`: 775 passed in 45 files. `npm run test:ui`: 151 passed in headless Edge, with a mocked native boundary. The Tauri release build exit 0; the setup program was built and never installed.
- KeePassXC 2.8.0 interchange and kdbxweb interchange pass against the CLI build.
- 32 CLI steps on the release build pass through the verification skill's helper (create, add, list, masked view, reveal, audit, scoped export, `run`, reserved names, rotate, history, Recycle Bin, restore, purge, a refused reveal when the journal cannot be written).
- Two focused real-window proofs pass on the desktop build: `-JournalOnly` 27 checks, `-JournalExportOnly` 8 checks. They drive the unlock screen, the key list, Activity and Save .env. No other desktop flow has run on a real window.
- `check_docs.py` passes in the fast and the full mode; the tool tests, the no-network check and `check_dependencies.ps1` exit 0.

Builds (unsigned, SHA-256), in `<build>\release`:

- `akm.exe` `094b76fbdfc8129aa54e5d45e49234a8b111056254de2d9036b666c53c9c71f5`
- `akm-desktop.exe` `926ac145c18b001a704f635cfc13f3fac23d436a6339b627337cd260dce614b9`
- `bundle\nsis\AKM_0.1.0_x64-setup.exe` `4a276c32962d097b8b956b318fe0e9f4138619c4c3c341fee3d5a75b0f4818ac`

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
3. A table of the 9 Nov deductions of that area: holds, partly holds or does not hold, with one line of evidence.

Then:

4. Findings outside your areas, with no points.
5. What you did not cover.
6. Two answers with no points, in plain words.
   - (a) Think of the part of AKM that your areas cover. Would you keep your own API keys in this build today, when each key can be revoked at its provider: yes, yes with named precautions, or no? Name the one failure that you expect a user to meet first.
   - (b) For each required workflow of `docs/PROJECT.md` that your areas touch: complete, partly complete or missing, on the CLI and on the desktop app. For each, say if you ran it, read its code, or took the project's word.
7. Do your points say the same thing as your answer in 6(a)? If not, say what the points miss and what they overstate.

Write plain sentences. Give no total for the whole project.
