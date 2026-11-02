# AKM agent rules

AKM is a Windows API-key manager. It has a Rust core and CLI, and a Tauri + React desktop app, over a KeePass (KDBX) vault. The project will become open source later. Write every tracked file as if it were public.

## Start here

1. [docs/STATE.md](docs/STATE.md) gives the current status and the next action. Read it at the start of every session.
2. [docs/PROJECT.md](docs/PROJECT.md) gives the goals, the non-goals and the contracts that every change must keep. Read it before you change behaviour.
3. [docs/FINDINGS.md](docs/FINDINGS.md) lists every open review finding. Read it before you plan a fix or a review.
4. [docs/ROADMAP.md](docs/ROADMAP.md) lists the remaining work units in order.
5. [docs/TESTING.md](docs/TESTING.md) gives the commands that define done, and the tests that need the owner.
6. [docs/REVIEWING.md](docs/REVIEWING.md) gives the review brief and the models for reviews and side jobs.

## Safety

- Use fake data only: temporary vaults and fake keys. Real secrets wait for the release gate. That gate is an independent full review that scores above 90/100, followed by the owner's own testing.
- Keep the owner's clipboard intact. A test that writes the clipboard needs its empty-clipboard guard and the owner present.
- Lock, suspend or sign out of Windows, or minimize or hide a real window, only after the owner confirms in chat that they are ready to watch. Each confirmation allows one attempt. A failed attempt stops, and the result goes back to the owner.
- The desktop app's saved settings point at a fake vault from an older proof run (see STATE.md). Keep that setting until the owner gives the value to restore.
- Keep KDBX compatibility. The format is established. This integration and the community `keepass` writer are not audited.

## Owner gates

These need the owner's yes in chat each time:

- making the repository public, or publishing a release or installer;
- installing a build, or code signing;
- using real secrets;
- turning on GitHub Actions;
- merging a pull request;
- the visible-window and OS tests in Safety.

You can do these without asking: branches, commits, pushes and pull requests on the private `Gargeya-Grey/akm-vault` repository.

## How work runs

- Git is the record. Each commit or pull request says what changed and which checks ran, with their results. Put disposable scripts, probes and logs in `.scratch/`, which Git ignores. Commit only what becomes a real test or tool.
- Before each commit, read `git status` and the exact staged diff. Before each push, list every outgoing branch and tag, and read each new commit's changes (`git log -p <remote>..<ref>`), including any parentless commit. A file that a later commit removes is still in the pushed history. Look for secrets, vaults, screenshots and private evidence. Ignore rules are a guard, not a secrets scanner. Keep unfamiliar changes that you did not make. Ask the owner about them.
- A code defect closes only when two things are true: a permanent regression test fails on the affected version and passes after the fix, and the work unit passes its review. A design, coverage or premise finding closes when its decision or evidence is verified and reviewed. Write down any limit that remains.
- Each work unit gets one fresh review (REVIEWING.md). A review passes when it establishes no P0, P1 or P2 finding and its P3 findings have moved to a named later unit ([pass rule](docs/REVIEWING.md#pass-rule)). If one work unit fails review twice, stop patching. Write the design question in STATE.md and ask the owner.
- The only measured score is 40.5/100, from the baseline review of 30 Oct 2026. Never add projected points to it. A new score needs a new full review.
- Before you stop, update STATE.md: what changed, the checks you ran, the open findings, and one next action.
