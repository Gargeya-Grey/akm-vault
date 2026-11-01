# Captured-outcome follow-up: observations, not a cause verdict

Read-only source inspection after the one failed owner-ready attempt. No application/dependency/proof-mechanism edit or live retry. The raw failure and post-release UIA snapshot remain authoritative for what was observed.

## Supported source facts

- `desktop/src/components/Settings.tsx:76-88` expires local protected results only when its DOM `visibilitychange` listener sees `document.hidden`. This advances local request IDs and closes the import dialog. `submitImport:254-267` checks that request identity before installing the in-context success/error. The captured **post-release** local success is consistent with that invalidation not preceding the completion; it is not telemetry of what the renderer saw while minimized.
- `desktop/src/App.tsx:172-174` polls status every3seconds while unlocked. `desktop/src-tauri/src/commands.rs:25-29` implements synchronous `vault_status` by calling `Session::access`; `session.rs:437-470` takes the stored-vault mutex while invoking the operation. `confirm_import` in `commands.rs:635-643` holds that same access through `vault.import_env`, including its post-commit journal append. A status call can therefore wait behind the proof's held fake journal lock.
- Installed/pinned `tauri-macros2.7.4/src/command/wrapper.rs:398-429` generates a direct call for the synchronous command wrapper, versus an asynchronous responder at350-390. This identifies a potential invocation-thread scheduling seam; this inspection alone does **not** establish which UI/native/renderer event was blocked in the failed run.
- Installed/pinned `wry0.57.1/src/webview2/mod.rs:1333-1359` skips its parent `WM_SIZE` bounds update for `SIZE_MINIMIZED`. Its explicit `set_visible` method calls the WebView2 visibility setter separately. `tauri-runtime-wry2.12.3/src/lib.rs:4273-4294` handles resize by updating webview bounds. These inspected paths do not demonstrate a guarantee that native `IsIconic` becomes DOM `document.hidden`; other Chromium/runtime event paths were not instrumented or ruled out.

## What is still needed

Separate native minimized state, renderer visibility-event delivery, request invalidation and UIA tree freshness in a small deterministic diagnostic design. Check invocation responsiveness while a fake mutation's journal append is stalled. Do not merely extend the300ms predicate, inject synthetic visibility into the real proof, or assume that one of the source seams above caused this observation.

Any correction/new native visibility notification needs explicit ownership and stale-event semantics consistent with the new native session boundary, plus actual caller regressions. Preserve the application-owned dispatched acknowledgment separately from local protected continuation lifetime. No production delay/debug hook or automatic lock of a replacement owner.

Obtain owner agreement on that bounded diagnosis/correction and separately renew readiness before another live visibility attempt. Do not consume the single fresh reviewer while treating the failed proof as a pass. No dropped receipt, real-secret exposure or numerical-score change is claimed.
