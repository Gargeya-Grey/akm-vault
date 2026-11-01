# Unit 1a round-two targeted probe record

All paths in this directory are reviewer-owned. The probes used fake data and did not use the GUI, native automation, clipboard, settings, or a real vault. The CLI probes removed their temporary fake vault or journal on exit. No runtime data archive was retained.

## CLI whitespace boundary

`python probe_whitespace_boundary.py` used `W:\API Keys Manager\target\release\akm.exe`, SHA-256 `382422d789ded2216aa8f3d344b8be72c27bd76a88f77fff88b76c9d2a5983e7`. It created a fake vault, added a fake key, and placed one controlled trailing byte after a valid newline-terminated journal entry. `whitespace-boundary-result.json` records the output. A final form-feed byte led `audit --verify` to exit 0 before `get`; `get --field secret` exited 0 and disclosed the fake secret; a subsequent `audit --verify` exited 2. A final space remained a valid JSON whitespace prefix, and a final vertical tab was refused without disclosure. The form-feed test demonstrates the distinct invalid-JSON-whitespace boundary.

## React/API handoff and action names

`node 'W:\API Keys Manager\desktop\node_modules\vitest\vitest.mjs' run --config vitest.config.mjs --reporter=verbose` ran in this directory. The first setup attempt failed because the scratch Vite config lacked a `react/jsx-dev-runtime` alias. After adding that alias, one test file and two tests passed. The tests used the actual `App`, `api`, and notice modules with fake native IPC. They demonstrated loss of a drained application warning and loss of a `completedWhileLocked` receipt delivered between roots. They also observed replay of the operation notice and an active B region.

The file was then extended with an independent Activity test. `node 'W:\API Keys Manager\desktop\node_modules\vitest\vitest.mjs' run --config vitest.config.mjs -t 'valid unrecognized' --reporter=verbose` passed one test and skipped the previous two. With a fake native report containing `__proto__`, an explicit test error boundary caught a React render error. The real `App` has no such boundary.

`python probe_legacy_action.py` created an isolated, fake, correctly hashed one-record JSONL journal containing `__proto__`. The release CLI accepted it as consistent and displayed the action, with exit 0. The separate React probe injected the equivalent report through mocked native IPC; it was not a real Tauri test. Results are in `legacy-action-result.json`.

## Sequence precision

`node probe_sequence_precision.cjs` parsed `{"brokenAt":9007199254740993}` and printed `displayed` as `9007199254740992` with `exact` false. This is a JavaScript number-boundary probe combined with inspection of the Rust DTO and Activity component, not a real Tauri run.
