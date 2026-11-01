# Review update pending final clipboard reread

Historical intermediate update. The later clipboard implementation and follow-up probe results are assessed in `followup-review-result.md`.

The earlier clipboard source-only assessment is superseded by the user's new real-runtime evidence. The opted-in test passed its empty-clipboard precondition and observed sequence 2353 after return/owner destruction instead of captured sequence 2350. The reviewed lease requires sequence equality to clear, so this is a supported ownership/cleanup defect, not merely missing runtime coverage. The reviewer has not accessed the real clipboard. Final assessment awaits the planner's changed clipboard source and new evidence.

## Additional P2 finding

`desktop/src/App.tsx:43-46`; `desktop/src/api.ts:24-26`; `desktop/src-tauri/src/session.rs:232-243`.

A late completed receipt from an old session can false-lock a newer session permanently until a real lock/restart. For example, an authorized session-A export may finish after locking and successfully opening session B. The original export's completedWhileLocked error calls reportCompletion. App then calls showLocked, which hides B and advances the frontend generation but never locks native B. On the next unlock, Session::begin_open sees native B active and emits vault-locked. That event invalidates the new unlock response. The backend is active again but the UI remains locked, and subsequent attempts repeat the cycle.

`late-receipt.test.tsx` imports the actual App and API wrapper and models the native begin_open notification rule from source. The test fails at its recovery assertion with `native remains active=true, unlocks=2, native locks=0`. Results are in `late-receipt-results.json`. No real GUI or clipboard was used.

`src/bin/late_receipt_native.rs` independently exercises the actual Session and private-export modules. It pauses an authorized export, locks A, opens B, then completes A's export. The old operation returns completedWhileLocked while native B remains active. Starting another open over B emits a lock notification and installs another active session. The probe exited 0; `late-receipt-native-results.txt` records these confirmations. It uses a fake clipboard backend and temporary fake vaults only.

Carry operation/session identity with completed receipts so an obsolete receipt cannot invalidate a newer session. If receipt handling intentionally hides the current session, perform and coordinate an actual native lock rather than creating a frontend-only locked state. Preserve a visible, secret-free receipt for the completed old operation.

## Current clipboard reread

The next source reread found a new close-synthesis fake and regression, but production publish/clear still captured before close and required equality with that sequence. This is an in-progress red-test revision, not a final repaired implementation. The reviewer has not approved it. PHASE1 now records the real failure and further corrections under way, with the native save-dialog harness still being repaired. A final clipboard source/test verdict remains pending a stable repaired revision.
