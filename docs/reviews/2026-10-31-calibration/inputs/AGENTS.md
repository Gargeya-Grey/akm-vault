# AKM project context

This entry point applies to the whole project. Keep it short. Detailed state belongs in the linked files.

## Read before acting

1. Read [current state and next action](docs/STATE.md).
2. Read [requirements and architectural contracts](docs/PROJECT.md).
3. For phase-two work, read [PHASE2.md](PHASE2.md) and the relevant entries in the [review ledger](docs/REVIEW_LEDGER.md).
4. Use the [verification record](docs/VERIFICATION.md). Load raw reviews only when their evidence matters.

## Safety and scope

- Windows only. Use temporary fake vaults and fake secrets until the owner's independent score-above-90 gate and subsequent testing are satisfied.
- Preserve KDBX compatibility. An established format does not make the community writer or this integration audited.
- Do not clear or overwrite an existing user clipboard. Interactive tests need an empty-clipboard guard. Do not lock or suspend the computer without the owner's participation.
- Do not guess the pre-restart vault preference. Its loss and the retained fake-vault baseline are recorded in `docs/STATE.md`.
- No git initialization, commit, push, publication, installer installation, signing enrollment or real-secret migration without authorization.
- Do not discard unfamiliar changes. A historical passing test applies only to the code and artifact it actually checked.

## Context discipline

- Current user instructions govern scope. Code and reproducible evidence govern behavior. `docs/STATE.md` governs the latest recorded status; old reviews describe their reviewed snapshots, not necessarily current defects.
- Before compaction, interruption, handoff or a completed work unit, update `docs/STATE.md` with changed files, exact checks, artifact identity, unresolved findings and one concrete next action.
- Record new evidence and review dispositions in the ledger. Do not replace a raw failed review with a later approval. Mark supersession explicitly.
- Never add projected points to the historical 40.5/100 score. A correction review is not a full rubric rescore or professional audit.
- After a restart, inspect ownership and fingerprints before resuming. Do not repeat completed suites solely because conversational context was compacted.
- Fresh adversarial reviewers use [the reviewer brief](docs/REVIEWER_BRIEF.md), not the implementer's reasoning or this chat transcript.
- On 2026-10-31 the owner explicitly authorized `openai/gpt-6.1-terra#max` for AKM fresh adversarial reviewers. Honor newer instructions and confirm model availability before dispatch. This is not a general authorization for unrelated model choices.
