//! Real generated IPC handlers on Tauri's mock runtime. No OS window or clipboard.
//! A standalone harness avoids executing the source modules' unit suites twice.
#![allow(dead_code, unused_imports)]
#[path = "../src/clipboard.rs"]
mod clipboard;
#[path = "../src/clipboard_fake.rs"]
mod clipboard_fake;
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
#[path = "../src/publication.rs"]
#[allow(unused_imports)]
mod publication;
#[path = "../src/reminders.rs"]
mod reminders;
#[path = "../src/session.rs"]
#[allow(unused_imports)]
mod session;
#[path = "../src/settings.rs"]
mod settings;
#[path = "../src/test_support.rs"]
mod test_support;
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
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody, InvokeResponse, InvokeResponseBody},
    test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
};

type Clipboard = Arc<Mutex<clipboard_fake::FakeState>>;

fn clipboard_text(state: &Clipboard) -> String {
    state.lock().unwrap().text()
}

#[derive(Default)]
struct Toasts(Mutex<Vec<(String, String)>>);
impl reminders::Notifier for Toasts {
    fn permission(&self) -> reminders::Permission {
        reminders::Permission::Granted
    }
    fn show(&self, title: &str, body: &str) -> Result<(), String> {
        self.0.lock().unwrap().push((title.into(), body.into()));
        Ok(())
    }
}
#[derive(Default)]
struct Flag(Mutex<bool>);
impl reminders::ReminderStore for Flag {
    fn enabled(&self) -> bool {
        *self.0.lock().unwrap()
    }
    fn set_enabled(&self, enabled: bool) -> Result<(), error::CommandError> {
        *self.0.lock().unwrap() = enabled;
        Ok(())
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
    let clipboard: Clipboard = Arc::default();
    let session = Arc::new(Session::new(
        Arc::new(MonotonicClock::default()),
        Box::new(clipboard_fake::TransactionBackend(clipboard_fake::FakeApi(
            clipboard.clone(),
        ))),
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
    let toasts = Arc::new(Toasts::default());
    let app = mock_builder()
        .manage(VaultState(session.clone()))
        .manage(reminders::ReminderState(Arc::new(
            reminders::Reminders::new(Arc::new(Flag::default()), toasts.clone()),
        )))
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
        ("recycled_list", serde_json::json!({})),
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
        let sequence_before = clipboard.lock().unwrap().sequence;
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
        assert_eq!(
            clipboard.lock().unwrap().sequence,
            sequence_before,
            "{command}: a delayed command must not touch the clipboard before its snapshot is read"
        );
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
        match command {
            "key_copy" => assert_eq!(clipboard_text(&clipboard), "fake-dispatch-value"),
            "env_copy" => assert_eq!(
                clipboard_text(&clipboard),
                "TOKEN=fake-dispatch-value
"
            ),
            _ => {}
        }
        println!("{command}: invocationReturnedWhileStalled={returned_before_release}; realResponseAfterRelease=true");
    }
    assert!(
        stalled.is_empty(),
        "vault-backed commands blocked their Tauri invocation thread: {stalled:?}"
    );
    println!("PASS: ten actual generated IPC handlers release invocation dispatch while fake vault access stalls; no real window/clipboard/settings.");

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

    // Rotation, the Recycle Bin, restore and purge with the shapes that api.ts sends.
    let listed = call(webview, "keys_list", serde_json::json!({})).unwrap();
    let history_before = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["id"] == id.as_str())
        .unwrap()["historyCount"]
        .as_u64()
        .unwrap();
    let rotated = call(
        webview,
        "key_rotate",
        serde_json::json!({"id": id, "secret": "fake-rotated-wire-value"}),
    )
    .unwrap();
    assert_eq!(rotated["key"]["id"], id.as_str());
    assert_eq!(rotated["key"]["historyCount"], history_before + 1);
    assert_eq!(rotated["key"]["secretChange"], "known");
    assert!(rotated["changedAt"].is_string());
    assert!(!rotated.to_string().contains("fake-rotated-wire-value"));
    assert!(!rotated.to_string().contains("fake-dispatch-value"));
    let repeated = call(
        webview,
        "key_rotate",
        serde_json::json!({"id": id, "secret": "fake-rotated-wire-value"}),
    )
    .unwrap_err();
    assert_eq!(repeated["kind"], "invalid");
    call(webview, "key_remove", serde_json::json!({"id": id})).unwrap();
    let listed = call(webview, "keys_list", serde_json::json!({})).unwrap();
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .all(|key| key["id"] != id.as_str()));
    let recycled = call(webview, "recycled_list", serde_json::json!({})).unwrap();
    assert_eq!(recycled.as_array().unwrap().len(), 1);
    assert_eq!(recycled[0]["key"]["id"], id.as_str());
    assert!(recycled[0]["deletedAt"].is_string());
    assert!(!recycled.to_string().contains("fake-rotated-wire-value"));
    let restored = call(webview, "key_restore", serde_json::json!({"id": id})).unwrap();
    assert_eq!(restored["id"], id.as_str());
    assert_eq!(restored["historyCount"], history_before + 1);
    let live_purge = call(webview, "key_purge", serde_json::json!({"id": id})).unwrap_err();
    assert_eq!(
        live_purge["kind"], "notFound",
        "a live key is never purged from here"
    );
    call(webview, "key_remove", serde_json::json!({"id": id})).unwrap();
    call(webview, "key_purge", serde_json::json!({"id": id})).unwrap();
    let recycled = call(webview, "recycled_list", serde_json::json!({})).unwrap();
    assert!(recycled.as_array().unwrap().is_empty());
    println!("PASS: key_rotate, recycled_list, key_restore and key_purge answer the frontend's argument and reply shapes; no reply carries a secret.");

    // The opt-in notification, with the shapes that api.ts sends.
    let off = serde_json::json!({"enabled": false, "permission": "granted", "delivery": "none", "blocked": false});
    assert_eq!(
        call(webview, "reminders_status", serde_json::json!({})).unwrap(),
        off
    );
    let ignored = call(
        webview,
        "reminders_notify",
        serde_json::json!({"count": 3, "sessionId": owner}),
    )
    .unwrap();
    assert_eq!(ignored, off, "a disabled setting shows nothing");
    assert!(toasts.0.lock().unwrap().is_empty());
    let on = call(
        webview,
        "reminders_set",
        serde_json::json!({"enabled": true}),
    )
    .unwrap();
    assert_eq!(on["enabled"], true);
    let sent = call(
        webview,
        "reminders_notify",
        serde_json::json!({"count": 2, "sessionId": owner}),
    )
    .unwrap();
    assert_eq!(sent["delivery"], "sent");
    assert_eq!(
        *toasts.0.lock().unwrap(),
        vec![("AKM".to_string(), "2 API keys need attention".to_string())]
    );
    let extra = call(
        webview,
        "reminders_notify",
        serde_json::json!({"count": 2, "sessionId": owner, "title": "Secret title", "body": "Project X"}),
    )
    .unwrap();
    assert_eq!(extra["delivery"], "sent");
    assert!(
        toasts
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|(title, body)| title == "AKM" && body == "2 API keys need attention"),
        "a page cannot pass text into the notification"
    );
    assert_eq!(
        call(
            webview,
            "reminders_notify",
            serde_json::json!({"count": 0, "sessionId": owner})
        )
        .unwrap_err()["kind"],
        "invalid"
    );
    let stranger = uuid::Uuid::new_v4().to_string();
    let shown_so_far = toasts.0.lock().unwrap().len();
    assert_eq!(
        call(
            webview,
            "reminders_notify",
            serde_json::json!({"count": 2, "sessionId": stranger})
        )
        .unwrap_err()["kind"],
        "locked",
        "a request made for another session shows nothing"
    );
    assert!(
        call(webview, "reminders_notify", serde_json::json!({"count": 2})).is_err(),
        "a request that names no session is refused"
    );
    assert_eq!(toasts.0.lock().unwrap().len(), shown_so_far);

    call(
        webview,
        "vault_lock",
        serde_json::json!({"sessionId": owner}),
    )
    .unwrap();
    let shown_before_lock = toasts.0.lock().unwrap().len();
    assert_eq!(
        call(
            webview,
            "reminders_notify",
            serde_json::json!({"count": 2, "sessionId": owner})
        )
        .unwrap_err()["kind"],
        "locked"
    );
    assert_eq!(toasts.0.lock().unwrap().len(), shown_before_lock);
    println!("PASS: reminders_status, reminders_set and reminders_notify answer the frontend shapes; a locked session shows nothing.");
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
    clipboard_through_the_handlers(&session, webview, &clipboard, &reserved, &second_vault);
    println!("PASS: key_copy through the generated handler publishes with the real publisher over the fake Win32 API, a lock after it cleans up once, and a copy delayed past a lock leaves the clipboard untouched.");
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings_file).unwrap()).unwrap();
    assert_eq!(saved["vaultPath"], second_vault.display().to_string());
    println!("PASS: env_import, vault_warnings, vault_dismiss_warnings, vault_lock, vault_prepare_open and vault_unlock answer the frontend's argument and reply shapes through the production handler list; the saved path went to the fake settings file.");
}

fn send(
    webview: &tauri::Webview<MockRuntime>,
    command: &str,
    args: serde_json::Value,
) -> mpsc::Receiver<InvokeResponse> {
    let (response_tx, response_rx) = mpsc::channel();
    let target = webview.clone();
    let command = command.to_string();
    std::thread::spawn(move || {
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
    response_rx
}

/// A copy command and a session change, through the generated handlers and the real publisher
/// over the fake Win32 API (1H-08). A lock after the copy cleans it up once. A lock that ends
/// the session while a delayed copy waits for the vault leaves the clipboard untouched.
fn clipboard_through_the_handlers(
    session: &Arc<Session>,
    webview: &tauri::Webview<MockRuntime>,
    clipboard: &Clipboard,
    owner: &str,
    vault: &std::path::Path,
) {
    let added = call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake handler copy", "secret": "fake-handler-secret"}}),
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_string();

    assert_eq!(
        call(webview, "key_copy", serde_json::json!({"id": id})).unwrap(),
        serde_json::json!(30)
    );
    assert_eq!(clipboard_text(clipboard), "fake-handler-secret");
    let (after_copy, clears_before) = {
        let state = clipboard.lock().unwrap();
        (state.sequence, state.clears)
    };
    call(
        webview,
        "vault_lock",
        serde_json::json!({"sessionId": owner}),
    )
    .unwrap();
    {
        let state = clipboard.lock().unwrap();
        assert_eq!(state.text(), "", "a lock cleans up the copy it owns");
        assert_eq!(state.clears, clears_before + 1);
        assert!(state.sequence > after_copy);
        assert_eq!(state.opens, state.closes);
    }

    let reserved = call(webview, "vault_prepare_open", serde_json::json!({})).unwrap();
    let reserved = reserved.as_str().unwrap().to_string();
    call(
        webview,
        "vault_unlock",
        serde_json::json!({"path": vault.display().to_string(), "password": "fake-dispatch-password-123!", "sessionId": reserved}),
    )
    .unwrap();

    let (sequence, clears_after_lock) = {
        let state = clipboard.lock().unwrap();
        (state.sequence, state.clears)
    };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holder = session.clone();
    let holding = std::thread::spawn(move || {
        let _ = holder.access(Access::Read, None, |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        });
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let response = send(webview, "key_copy", serde_json::json!({"id": id}));
    call(
        webview,
        "vault_lock",
        serde_json::json!({"sessionId": reserved}),
    )
    .unwrap();
    release_tx.send(()).unwrap();
    holding.join().unwrap();
    let reply = response
        .recv_timeout(Duration::from_secs(60))
        .expect("the delayed copy answers");
    let InvokeResponse::Err(error) = reply else {
        panic!("a copy delayed past a lock must fail");
    };
    assert_eq!(error.0["kind"], "locked");
    let state = clipboard.lock().unwrap();
    assert_eq!(state.text(), "");
    assert_eq!(
        state.sequence, sequence,
        "the delayed copy never reached the clipboard"
    );
    assert_eq!(state.clears, clears_after_lock);
}
