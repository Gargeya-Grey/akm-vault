//! Real generated IPC handlers on Tauri's mock runtime. No OS window or clipboard.
//! A standalone harness avoids executing the source modules' unit suites twice.
#![allow(dead_code)]
#[path = "../src/clipboard.rs"]
mod clipboard;
#[path = "../src/commands.rs"]
#[allow(unused_imports)]
mod commands;
#[path = "../src/error.rs"]
#[allow(unused_imports)]
mod error;
#[path = "../src/import_review.rs"]
#[allow(unused_imports)]
mod import_review;
#[path = "../src/policy.rs"]
mod policy;
#[path = "../src/private_export.rs"]
mod private_export;
#[path = "../src/session.rs"]
#[allow(unused_imports)]
mod session;
#[path = "../src/settings.rs"]
mod settings;
#[path = "../src/views.rs"]
#[allow(unused_imports)]
mod views;
#[path = "../src/visibility.rs"]
#[allow(unused_imports)]
mod visibility;

use commands::*;
use session::{Access, MonotonicClock, Session, VaultState};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody, InvokeResponse, InvokeResponseBody},
    test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
};

struct FakeClipboard;
impl clipboard::ClipboardBackend for FakeClipboard {
    fn set_private(&mut self, _: &str) -> clipboard::Publication {
        clipboard::Publication::fake_published(1)
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> {
        Ok(true)
    }
}

struct Answer(Arc<AtomicBool>);
impl import_review::ImportConfirm for Answer {
    fn confirm(&self, _: &import_review::ImportReview) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

fn call(
    webview: &tauri::Webview<MockRuntime>,
    command: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    let (response_tx, response_rx) = mpsc::channel();
    let target = webview.clone();
    let command = command.to_string();
    let dispatch = std::thread::spawn(move || {
        target.on_message(
            InvokeRequest {
                cmd: command,
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            },
            Box::new(move |_, _, response, _, _| {
                response_tx.send(response).unwrap();
            }),
        );
    });
    let response = response_rx
        .recv_timeout(Duration::from_secs(60))
        .expect("actual command response");
    dispatch.join().unwrap();
    match response {
        InvokeResponse::Ok(InvokeResponseBody::Json(text)) => {
            Ok(serde_json::from_str(&text).unwrap())
        }
        InvokeResponse::Ok(InvokeResponseBody::Raw(_)) => panic!("raw reply"),
        InvokeResponse::Err(error) => Err(error.0),
    }
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let session = Arc::new(Session::new(
        Arc::new(MonotonicClock::default()),
        Box::new(FakeClipboard),
        Arc::new(|_| {}),
    ));
    session.observe(policy::Signal::Available);
    let answer = Arc::new(AtomicBool::new(true));
    let owner = session.prepare_open().unwrap();
    let ticket = session.claim_open(&owner).unwrap();
    session
        .open(ticket, || {
            akm_core::Vault::create(
                directory.path().join("fake.kdbx"),
                "fake-dispatch-password-123!",
                None,
            )
            .map_err(Into::into)
        })
        .unwrap();
    let key = session
        .access(Access::Write, None, |vault| {
            let mut key =
                akm_core::ApiKeyInput::new("Fake dispatch fixture", "fake-dispatch-value");
            key.project = Some("demo".into());
            key.environment = Some("dev".into());
            key.env_var = Some("TOKEN".into());
            Ok(vault.add(key)?)
        })
        .unwrap()
        .value;
    let id = key.id.clone();
    let second_vault = directory.path().join("second.kdbx");
    akm_core::Vault::create(&second_vault, "fake-dispatch-password-123!", None).unwrap();
    let settings_file = directory.path().join("fake-settings").join("settings.json");
    let app = mock_builder()
        .manage(VaultState(session.clone()))
        .manage(settings::PreferenceState(Arc::new(
            settings::FilePreferences::new(
                Some(settings_file.clone()),
                Some(directory.path().join("fake-default.kdbx")),
            ),
        )))
        .manage(import_review::ConfirmState(Arc::new(Answer(
            answer.clone(),
        ))))
        .invoke_handler(commands::production_handlers!())
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let webview: &tauri::Webview<MockRuntime> = window.as_ref();
    let mut stalled = Vec::new();
    for (command, args) in [
        ("vault_status", serde_json::json!({})),
        ("keys_list", serde_json::json!({})),
        ("key_reveal", serde_json::json!({"id": id})),
        ("key_copy", serde_json::json!({"id": id})),
        ("audit_read", serde_json::json!({"limit": 10})),
        (
            "env_export",
            serde_json::json!({"project":"demo", "environment":"dev", "all":false}),
        ),
        (
            "env_copy",
            serde_json::json!({"project":"demo", "environment":"dev", "all":false}),
        ),
        (
            "env_import",
            serde_json::json!({"sessionId": owner, "operationId": uuid::Uuid::new_v4().to_string(), "text":"TOKEN=fake-next-value", "project":"demo", "environment":"dev"}),
        ),
        ("backup_info", serde_json::json!({})),
    ] {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let holder = session.clone();
        let holding = std::thread::spawn(move || {
            holder.access(Access::Read, None, |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (response_tx, response_rx) = mpsc::channel();
        let (returned_tx, returned_rx) = mpsc::channel();
        let target = webview.clone();
        let dispatch = std::thread::spawn(move || {
            target.on_message(
                InvokeRequest {
                    cmd: command.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "http://tauri.localhost".parse().unwrap(),
                    body: InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.into(),
                },
                Box::new(move |_, _, response, _, _| {
                    response_tx.send(response).unwrap();
                }),
            );
            returned_tx.send(()).unwrap();
        });
        let returned_before_release = returned_rx.recv_timeout(Duration::from_millis(500)).is_ok();
        let early = response_rx.try_recv().ok();
        let completed_while_stalled = early.is_some();
        release_tx.send(()).unwrap(); // Always release/join on a desired-behavior failure.
        holding.join().unwrap().unwrap();
        dispatch.join().unwrap();
        let response = early
            .or_else(|| response_rx.recv_timeout(Duration::from_secs(5)).ok())
            .expect("actual command response");
        assert!(
            matches!(response, InvokeResponse::Ok(_)),
            "{command}: actual handler, not an ACL/fixture error"
        );
        assert!(
            !completed_while_stalled,
            "{command}: actual fake vault read must await its held snapshot"
        );
        if !returned_before_release {
            stalled.push(command);
        }
        println!("{command}: invocationReturnedWhileStalled={returned_before_release}; realResponseAfterRelease=true");
    }
    assert!(
        stalled.is_empty(),
        "vault-backed commands blocked their Tauri invocation thread: {stalled:?}"
    );
    println!("PASS: nine actual generated IPC handlers release invocation dispatch while fake vault access stalls; no real window/clipboard/settings.");

    // The wire shapes that desktop/src/api.ts sends, through the production handler list.
    let import = |operation: &str, session_id: &str, text: &str| {
        call(
            webview,
            "env_import",
            serde_json::json!({"sessionId": session_id, "operationId": operation, "text": text, "project": "demo", "environment": "dev"}),
        )
    };
    let (applied, declined, refused) = (
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
    );
    assert_eq!(
        import(
            &applied,
            &owner,
            "WIRE_ONE=fake-wire-one
"
        )
        .unwrap(),
        serde_json::json!({"added": 1, "updated": 0})
    );
    answer.store(false, Ordering::SeqCst);
    assert_eq!(
        import(
            &declined,
            &owner,
            "WIRE_TWO=fake-wire-two
"
        )
        .unwrap(),
        serde_json::Value::Null
    );
    answer.store(true, Ordering::SeqCst);
    let error = import(
        &refused,
        &uuid::Uuid::new_v4().to_string(),
        "WIRE_THREE=fake
",
    )
    .unwrap_err();
    assert_eq!(error["kind"], "locked");
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    let kinds: Vec<(String, String)> = report["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            (
                record["id"].as_str().unwrap().into(),
                record["kind"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert!(
        kinds.contains(&(applied.clone(), "importApplied".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&(declined.clone(), "importDeclined".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.iter().all(|(id, _)| id != &refused),
        "a refused request leaves no record"
    );
    call(
        webview,
        "vault_dismiss_warnings",
        serde_json::json!({"ids": [applied, declined]}),
    )
    .unwrap();
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    assert!(report["operations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|record| record["id"] != applied.as_str() && record["id"] != declined.as_str()));

    call(
        webview,
        "vault_lock",
        serde_json::json!({"sessionId": owner}),
    )
    .unwrap();
    let reserved = call(webview, "vault_prepare_open", serde_json::json!({})).unwrap();
    let reserved = reserved.as_str().unwrap().to_string();
    let status = call(
        webview,
        "vault_unlock",
        serde_json::json!({"path": second_vault.display().to_string(), "password": "fake-dispatch-password-123!", "sessionId": reserved}),
    )
    .unwrap();
    assert_eq!(status["sessionId"], reserved.as_str());
    assert_eq!(status["unlocked"], true);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings_file).unwrap()).unwrap();
    assert_eq!(saved["vaultPath"], second_vault.display().to_string());
    println!("PASS: env_import, vault_warnings, vault_dismiss_warnings, vault_lock, vault_prepare_open and vault_unlock answer the frontend's argument and reply shapes through the production handler list; the saved path went to the fake settings file.");
}
