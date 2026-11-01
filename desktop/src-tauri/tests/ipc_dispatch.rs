//! Real generated IPC handlers on Tauri's mock runtime. No OS window or clipboard.
//! A standalone harness avoids executing the source modules' unit suites twice.
#![allow(dead_code)]
#[path = "../src/clipboard.rs"]
mod clipboard;
#[path = "../src/commands.rs"]
mod commands;
#[path = "../src/error.rs"]
#[allow(unused_imports)]
mod error;
#[path = "../src/pending.rs"]
mod pending;
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
    sync::{mpsc, Arc},
    time::Duration,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody, InvokeResponse},
    test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
};

struct FakeClipboard;
impl clipboard::ClipboardBackend for FakeClipboard {
    fn set_private(&mut self, _: &str) -> Result<clipboard::CopyAttempt, error::CommandError> {
        Ok(clipboard::CopyAttempt {
            sequence: 1,
            failure: None,
        })
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> {
        Ok(true)
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
    let app = mock_builder()
        .manage(VaultState(session.clone()))
        .invoke_handler(tauri::generate_handler![
            vault_status,
            keys_list,
            key_reveal,
            key_copy,
            audit_read,
            env_export,
            env_copy,
            env_import_preview,
            backup_info
        ])
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
            "env_import_preview",
            serde_json::json!({"text":"TOKEN=fake-next-value", "project":"demo", "environment":"dev"}),
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
}
