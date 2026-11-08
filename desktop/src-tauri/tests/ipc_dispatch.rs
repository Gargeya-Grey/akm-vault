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
#[path = "../src/launch.rs"]
#[allow(unused_imports)]
mod launch;
#[path = "../src/policy.rs"]
mod policy;
#[path = "../src/private_export.rs"]
mod private_export;
#[path = "../src/publication.rs"]
#[allow(unused_imports)]
mod publication;
#[path = "../src/reminders.rs"]
mod reminders;
#[path = "../src/review_window.rs"]
#[allow(unused_imports)]
mod review_window;
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
use session::{Access, Held, MonotonicClock, Session, VaultState};
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
    fn show(&self, _held: &Held<'_>, title: &str, body: &str) -> Result<(), String> {
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

/// The native confirmation, scripted. `arm` makes the next dialog wait until the test answers it.
struct Dialog {
    approve: AtomicBool,
    window_live: AtomicBool,
    hold: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}
impl Dialog {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            approve: AtomicBool::new(true),
            window_live: AtomicBool::new(true),
            hold: Mutex::new(None),
        })
    }

    /// `entered` fires when the dialog opens. Send on `release` to answer it.
    fn arm(&self) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        *self.hold.lock().unwrap() = Some((entered_tx, release_rx));
        (entered_rx, release_tx)
    }
}
impl import_review::ImportConfirm for Dialog {
    fn prepare_review<'a>(
        &'a self,
        _: &import_review::ImportReview,
    ) -> review_window::ReviewWindow<'a> {
        Box::new(move |_held| {
            Box::new(move || {
                let held = self.hold.lock().unwrap().take();
                if let Some((entered, release)) = held {
                    entered.send(()).unwrap();
                    release
                        .recv_timeout(Duration::from_secs(60))
                        .expect("the test answers the dialog");
                }
                self.approve.load(Ordering::SeqCst)
            })
        })
    }
    fn window_live(&self) -> bool {
        self.window_live.load(Ordering::SeqCst)
    }
}

/// The native terminal, scripted. It opens no window and starts no process: it records what
/// native code would have started.
type Started = Vec<(std::path::PathBuf, Vec<(String, String)>)>;
struct Terminal {
    folder: std::path::PathBuf,
    approve: AtomicBool,
    started: Mutex<Started>,
}
impl launch::TerminalHost for Terminal {
    fn window_live(&self) -> bool {
        true
    }
    fn choose_folder(&self) -> Result<Option<std::path::PathBuf>, error::CommandError> {
        Ok(Some(self.folder.clone()))
    }
    fn prepare_review<'a>(&'a self, _: &launch::LaunchReview) -> review_window::ReviewWindow<'a> {
        Box::new(move |_held| Box::new(move || self.approve.load(Ordering::SeqCst)))
    }
    fn open_terminal(
        &self,
        _: &Held<'_>,
        request: &launch::LaunchRequest,
    ) -> Result<(), error::CommandError> {
        self.started
            .lock()
            .unwrap()
            .push((request.folder.clone(), request.secrets.to_vec()));
        Ok(())
    }
}

/// The commands that act on the open vault and name the session in which the page made the
/// request (the frontend's `sessionScoped`). The harness plays the page: it adds the session
/// that it opened last, unless a test names another one.
const SESSION_SCOPED: &[&str] = &[
    "vault_activity",
    "vault_upgrade",
    "vault_reload",
    "keys_list",
    "key_reveal",
    "key_copy",
    "key_add",
    "key_update",
    "key_remove",
    "key_rotate",
    "recycled_list",
    "key_restore",
    "key_purge",
    "audit_read",
    "env_export",
    "env_copy",
    "env_save",
    "env_launch_terminal",
    "key_history",
    "backup_info",
];
static PAGE_SESSION: Mutex<Option<String>> = Mutex::new(None);

fn scoped(command: &str, mut args: serde_json::Value) -> serde_json::Value {
    if SESSION_SCOPED.contains(&command) && args.get("sessionId").is_none() {
        if let Some(id) = PAGE_SESSION.lock().unwrap().clone() {
            args["sessionId"] = serde_json::json!(id);
        }
    }
    args
}

fn call(
    webview: &tauri::Webview<MockRuntime>,
    command: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    let (response_tx, response_rx) = mpsc::channel();
    let target = webview.clone();
    let command = command.to_string();
    let opened = matches!(command.as_str(), "vault_unlock" | "vault_create")
        .then(|| args["sessionId"].as_str().map(str::to_string))
        .flatten();
    let args = scoped(&command, args);
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
            if let Some(id) = opened {
                *PAGE_SESSION.lock().unwrap() = Some(id);
            }
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
    let dialog = Dialog::new();
    let terminal = Arc::new(Terminal {
        folder: directory.path().to_path_buf(),
        approve: AtomicBool::new(true),
        started: Mutex::default(),
    });
    let owner = session.prepare_open().unwrap();
    *PAGE_SESSION.lock().unwrap() = Some(owner.clone());
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
        .manage(import_review::ConfirmState(dialog.clone()))
        .manage(launch::TerminalState(terminal.clone()))
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
        let args = scoped(command, args);
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
            .or_else(|| response_rx.recv_timeout(Duration::from_secs(60)).ok())
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
    dialog.approve.store(false, Ordering::SeqCst);
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
    dialog.approve.store(true, Ordering::SeqCst);
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

    // The page reports a hidden document; native expires the presentation as a minimize does.
    let before_hide = session.snapshot().unwrap();
    assert_eq!(
        call(
            webview,
            "view_discard",
            serde_json::json!({"sessionId": owner})
        )
        .unwrap(),
        serde_json::Value::Null
    );
    let after_hide = session.snapshot().unwrap();
    assert_eq!(after_hide.ticket, before_hide.ticket);
    assert_ne!(after_hide.presentation, before_hide.presentation);

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
    the_dialog_is_the_review(&app, webview, &session, &dialog, directory.path());
    a_discard_names_its_session(&app, webview, &session, &dialog, directory.path());
    the_launch_command_takes_a_scope_and_no_program(webview, &terminal, directory.path());
    the_history_command_answers_times_and_field_names(webview, directory.path());
    a_stale_request_never_acts_on_the_session_that_replaced_its_own(
        webview,
        &session,
        &clipboard,
        directory.path(),
    );
}

/// Class B (P2R2-03). Every command that acts on the open vault names the session of the page
/// that made the request. A request that names session A, however late, does nothing to B.
fn a_stale_request_never_acts_on_the_session_that_replaced_its_own(
    webview: &tauri::Webview<MockRuntime>,
    session: &Arc<Session>,
    clipboard: &Clipboard,
    directory: &std::path::Path,
) {
    let (a, _) = open_new(webview, directory, "stale-a.kdbx");
    let (b, _) = open_new(webview, directory, "stale-b.kdbx");
    let added = call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake B key", "secret": "fake-b-secret", "project": "demo", "environment": "dev", "envVar": "B_TOKEN"}}),
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_string();
    let before = session.snapshot().unwrap();
    let sequence = clipboard.lock().unwrap().sequence;
    let scope = serde_json::json!({"project": "demo", "environment": "dev", "all": false});
    let requests = [
        ("vault_activity", serde_json::json!({})),
        ("vault_upgrade", serde_json::json!({})),
        ("vault_reload", serde_json::json!({})),
        ("keys_list", serde_json::json!({})),
        ("key_reveal", serde_json::json!({"id": id})),
        ("key_copy", serde_json::json!({"id": id})),
        (
            "key_add",
            serde_json::json!({"draft": {"title": "Fake stale", "secret": "fake-stale"}}),
        ),
        (
            "key_update",
            serde_json::json!({"id": id, "draft": {"title": "Renamed by A"}}),
        ),
        ("key_remove", serde_json::json!({"id": id})),
        (
            "key_rotate",
            serde_json::json!({"id": id, "secret": "fake-rotated-by-a"}),
        ),
        ("recycled_list", serde_json::json!({})),
        ("key_restore", serde_json::json!({"id": id})),
        ("key_purge", serde_json::json!({"id": id})),
        ("audit_read", serde_json::json!({"limit": 5})),
        ("env_export", scope.clone()),
        ("env_copy", scope.clone()),
        ("env_save", scope.clone()),
        ("env_launch_terminal", scope),
        ("key_history", serde_json::json!({"id": id})),
        ("backup_info", serde_json::json!({})),
    ];
    assert_eq!(
        requests.len(),
        SESSION_SCOPED.len(),
        "every session-scoped command is exercised"
    );
    for (command, mut args) in requests {
        assert!(SESSION_SCOPED.contains(&command), "{command}");
        args["sessionId"] = serde_json::json!(a);
        let error = call(webview, command, args).unwrap_err();
        assert_eq!(error["kind"], "locked", "{command}: a request for A");
    }
    assert_eq!(
        session.snapshot().unwrap(),
        before,
        "B's ticket, revision and presentation are intact"
    );
    assert_eq!(clipboard.lock().unwrap().sequence, sequence);

    for command in SESSION_SCOPED {
        assert!(
            call(
                webview,
                command,
                serde_json::json!({"sessionId": null, "id": id})
            )
            .is_err(),
            "{command}: a request with no session is refused"
        );
    }
    assert_eq!(session.snapshot().unwrap(), before);

    let listed = call(webview, "keys_list", serde_json::json!({"sessionId": b})).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["title"], "Fake B key");
    assert_eq!(
        call(
            webview,
            "key_reveal",
            serde_json::json!({"id": id, "sessionId": b})
        )
        .unwrap(),
        "fake-b-secret"
    );
    println!("PASS: all 20 session-scoped handlers refuse a request that names a retired session, leave B and the clipboard untouched, and still serve B's own requests.");
}

const PASSWORD: &str = "fake-dispatch-password-123!";

/// The terminal command through the production handler list. Whatever else the page sends,
/// native code starts only what its own host chose: the folder comes from the host and the
/// variables from the vault.
fn the_launch_command_takes_a_scope_and_no_program(
    webview: &tauri::Webview<MockRuntime>,
    terminal: &Arc<Terminal>,
    directory: &std::path::Path,
) {
    let (owner, _) = open_new(webview, directory, "launch.kdbx");
    call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake launch", "secret": "fake-launch-secret", "project": "demo", "environment": "dev", "envVar": "LAUNCH_TOKEN"}}),
    )
    .unwrap();
    let hostile = serde_json::json!({
        "project": "demo", "environment": "dev",
        "program": "calc.exe", "command": "calc.exe", "path": "C:/evil", "cwd": "C:/evil",
        "folder": "C:/evil", "workingDirectory": "C:/evil", "args": ["/c", "calc"],
        "env": {"AKM_PASSWORD": "fake-leak"}, "shell": "powershell.exe",
    });
    assert_eq!(
        call(webview, "env_launch_terminal", hostile.clone()).unwrap(),
        serde_json::json!(1)
    );
    let started = terminal.started.lock().unwrap().clone();
    assert_eq!(
        started,
        vec![(
            directory.to_path_buf(),
            vec![("LAUNCH_TOKEN".to_string(), "fake-launch-secret".to_string())]
        )],
        "only the host's folder and the vault's variable reached the launcher"
    );

    terminal.approve.store(false, Ordering::SeqCst);
    assert_eq!(
        call(webview, "env_launch_terminal", hostile.clone()).unwrap(),
        serde_json::Value::Null,
        "a decline answers null"
    );
    terminal.approve.store(true, Ordering::SeqCst);
    assert_eq!(terminal.started.lock().unwrap().len(), 1);

    let mut stale = hostile.clone();
    stale["sessionId"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        call(webview, "env_launch_terminal", stale).unwrap_err()["kind"],
        "locked"
    );
    let mut unscoped = serde_json::json!({"project": "demo"});
    unscoped["sessionId"] = serde_json::json!(owner);
    assert_eq!(
        call(webview, "env_launch_terminal", unscoped).unwrap_err()["kind"],
        "invalid",
        "a scope needs both parts"
    );
    assert_eq!(terminal.started.lock().unwrap().len(), 1);
    println!("PASS: env_launch_terminal is registered, answers a count or null, ignores every program, path and argument that a page adds, and refuses a stale session.");
}

/// The history command answers times and field names, with the camelCase shape of api.ts.
fn the_history_command_answers_times_and_field_names(
    webview: &tauri::Webview<MockRuntime>,
    directory: &std::path::Path,
) {
    open_new(webview, directory, "history.kdbx");
    let added = call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake history", "secret": "fake-history-one"}}),
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_string();
    assert_eq!(
        call(webview, "key_history", serde_json::json!({"id": id})).unwrap(),
        serde_json::json!([])
    );
    call(
        webview,
        "key_rotate",
        serde_json::json!({"id": id, "secret": "fake-history-two"}),
    )
    .unwrap();
    let history = call(webview, "key_history", serde_json::json!({"id": id})).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["index"], 0);
    assert!(history[0]["modifiedAt"].is_string());
    assert_eq!(history[0]["secretChanged"], true);
    assert!(history[0]["changed"].is_array());
    assert!(!history.to_string().contains("fake-history"));
    assert_eq!(
        call(webview, "key_history", serde_json::json!({"id": "missing"})).unwrap_err()["kind"],
        "notFound"
    );
    println!(
        "PASS: key_history answers index, modifiedAt, changed and secretChanged, and no value."
    );
}

fn reply(rx: &mpsc::Receiver<InvokeResponse>) -> Result<serde_json::Value, serde_json::Value> {
    match rx
        .recv_timeout(Duration::from_secs(60))
        .expect("the command answers")
    {
        InvokeResponse::Ok(InvokeResponseBody::Json(text)) => {
            Ok(serde_json::from_str(&text).unwrap())
        }
        InvokeResponse::Ok(InvokeResponseBody::Raw(_)) => panic!("raw reply"),
        InvokeResponse::Err(error) => Err(error.0),
    }
}

/// A new fake vault, opened as the current session through the production handlers. The
/// older session is retired by the reservation.
fn open_new(
    webview: &tauri::Webview<MockRuntime>,
    directory: &std::path::Path,
    name: &str,
) -> (String, std::path::PathBuf) {
    let path = directory.join(name);
    akm_core::Vault::create(&path, PASSWORD, None).unwrap();
    let reserved = call(webview, "vault_prepare_open", serde_json::json!({})).unwrap();
    let reserved = reserved.as_str().unwrap().to_string();
    call(
        webview,
        "vault_unlock",
        serde_json::json!({"path": path.display().to_string(), "password": PASSWORD, "sessionId": reserved}),
    )
    .unwrap();
    (reserved, path)
}

fn demo_dev_is_empty(path: &std::path::Path) -> bool {
    akm_core::Vault::open(path, PASSWORD)
        .unwrap()
        .plan_env(Some("demo"), Some("dev"), false)
        .unwrap()
        .is_empty()
}

fn import_args(owner: &str, operation: &str, text: &str) -> serde_json::Value {
    serde_json::json!({"sessionId": owner, "operationId": operation, "text": text, "project": "demo", "environment": "dev"})
}

fn record_kinds(webview: &tauri::Webview<MockRuntime>, operation: &str) -> Vec<String> {
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    report["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["id"] == operation)
        .map(|record| record["kind"].as_str().unwrap().to_string())
        .collect()
}

fn export_text(webview: &tauri::Webview<MockRuntime>) -> String {
    call(
        webview,
        "env_export",
        serde_json::json!({"project": "demo", "environment": "dev", "all": false}),
    )
    .unwrap()
    .as_str()
    .unwrap()
    .to_string()
}

fn expiry_notices(app: &tauri::App<MockRuntime>) -> Arc<Mutex<Vec<String>>> {
    use tauri::Listener;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    app.listen("vault-view-expired", move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    seen
}

/// P2R2-02. The native dialog is the review. An Import click stands after a hide, a page load,
/// a discard that came before it or after it, or a hidden window. Only a lock, another session
/// and another write to the vault refuse it, and then nothing is written.
fn the_dialog_is_the_review(
    app: &tauri::App<MockRuntime>,
    webview: &tauri::Webview<MockRuntime>,
    session: &Arc<Session>,
    dialog: &Arc<Dialog>,
    directory: &std::path::Path,
) {
    let delivered = expiry_notices(app);
    let applied = serde_json::json!({"added": 1, "updated": 0});
    let dismiss = |operation: &str| {
        call(
            webview,
            "vault_dismiss_warnings",
            serde_json::json!({"ids": [operation]}),
        )
        .unwrap();
    };

    for (index, approval) in [
        "a discard that reached native while the dialog was open",
        "a window that was hidden when the dialog returned",
        "a page load while the dialog was open",
        "a discard that was held until after the approval",
        "a discard that never arrived",
    ]
    .into_iter()
    .enumerate()
    {
        let (owner, path) = open_new(webview, directory, &format!("rule-{index}.kdbx"));
        let operation = uuid::Uuid::new_v4().to_string();
        let (entered, release) = dialog.arm();
        let pending = send(
            webview,
            "env_import",
            import_args(&owner, &operation, "TOKEN=fake-approved\n"),
        );
        entered.recv_timeout(Duration::from_secs(20)).unwrap();
        let reviewed = session.snapshot().unwrap();
        match approval {
            "a discard that reached native while the dialog was open" => {
                assert_eq!(
                    call(
                        webview,
                        "view_discard",
                        serde_json::json!({"sessionId": owner})
                    )
                    .unwrap(),
                    serde_json::Value::Null
                );
                assert_ne!(
                    session.snapshot().unwrap().presentation,
                    reviewed.presentation,
                    "the hide reached native"
                );
            }
            "a window that was hidden when the dialog returned" => {
                dialog.window_live.store(false, Ordering::SeqCst);
            }
            "a page load while the dialog was open" => session.document_started(),
            _ => {}
        }
        release.send(()).unwrap();
        assert_eq!(reply(&pending).unwrap(), applied, "{approval}");
        dialog.window_live.store(true, Ordering::SeqCst);
        if approval == "a discard that was held until after the approval" {
            assert_eq!(
                call(
                    webview,
                    "view_discard",
                    serde_json::json!({"sessionId": owner})
                )
                .unwrap(),
                serde_json::Value::Null
            );
        }
        assert_eq!(export_text(webview), "TOKEN=fake-approved\n", "{approval}");
        assert!(session.snapshot().is_ok(), "{approval}: still unlocked");
        assert_eq!(
            record_kinds(webview, &operation),
            ["importApplied"],
            "{approval}"
        );
        assert!(!demo_dev_is_empty(&path), "{approval}: written once");
        dismiss(&operation);
    }

    for change in ["lock", "session", "write", "reload"] {
        let (owner, path) = open_new(webview, directory, &format!("refuse-{change}.kdbx"));
        let operation = uuid::Uuid::new_v4().to_string();
        let (entered, release) = dialog.arm();
        let pending = send(
            webview,
            "env_import",
            import_args(&owner, &operation, "TOKEN=fake-refused\n"),
        );
        entered.recv_timeout(Duration::from_secs(20)).unwrap();
        let mut other = None;
        match change {
            "lock" => {
                call(
                    webview,
                    "vault_lock",
                    serde_json::json!({"sessionId": owner}),
                )
                .unwrap();
            }
            "session" => other = Some(open_new(webview, directory, "refuse-other.kdbx").1),
            "write" => {
                call(
                    webview,
                    "key_add",
                    serde_json::json!({"draft": {"title": "Fake other", "secret": "fake-other-secret"}}),
                )
                .unwrap();
            }
            _ => {
                call(webview, "vault_reload", serde_json::json!({})).unwrap();
            }
        }
        release.send(()).unwrap();
        let error = reply(&pending).unwrap_err();
        assert_eq!(
            error["kind"],
            if change == "lock" {
                "locked"
            } else {
                "stalePreview"
            },
            "{change}"
        );
        assert_eq!(
            record_kinds(webview, &operation),
            ["importFailed"],
            "{change}"
        );
        assert!(demo_dev_is_empty(&path), "{change}: nothing was written");
        if let Some(other) = other {
            assert!(demo_dev_is_empty(&other), "{change}: B got nothing");
        }
        dismiss(&operation);
    }
    assert!(
        delivered.lock().unwrap().len() >= 2,
        "the notices of the discards were delivered"
    );
    println!("PASS: through the production handlers an Import click stands after a hide, a page load, a hidden window and a late or missing discard; a lock, another session and another write refuse it with no write.");
}

/// P2R2-03. A discard names the session that the page read when it handled the hide. A
/// delayed discard from session A does nothing to session B and its answer is still success.
fn a_discard_names_its_session(
    app: &tauri::App<MockRuntime>,
    webview: &tauri::Webview<MockRuntime>,
    session: &Arc<Session>,
    dialog: &Arc<Dialog>,
    directory: &std::path::Path,
) {
    let delivered = expiry_notices(app);
    let (a, _) = open_new(webview, directory, "ident-a.kdbx");
    let a_view = session
        .access(Access::Read, None, |_| Ok("fake-a-view"))
        .unwrap();
    let (b, b_path) = open_new(webview, directory, "ident-b.kdbx");
    let operation = uuid::Uuid::new_v4().to_string();
    let (entered, release) = dialog.arm();
    let pending = send(
        webview,
        "env_import",
        import_args(&b, &operation, "TOKEN=fake-b-import\n"),
    );
    entered.recv_timeout(Duration::from_secs(20)).unwrap();
    let b_before = session.snapshot().unwrap();
    let notices_before = delivered.lock().unwrap().len();

    for stranger in [
        a.clone(),
        uuid::Uuid::new_v4().to_string(),
        "not-an-id".into(),
    ] {
        assert_eq!(
            call(
                webview,
                "view_discard",
                serde_json::json!({"sessionId": stranger})
            )
            .unwrap(),
            serde_json::Value::Null,
            "a discard for a session that is not current is a success"
        );
        assert_eq!(
            session.snapshot().unwrap(),
            b_before,
            "{stranger}: B's ticket, revision and presentation are intact"
        );
        assert_eq!(
            delivered.lock().unwrap().len(),
            notices_before,
            "no notice names B"
        );
    }
    assert!(
        call(webview, "view_discard", serde_json::json!({})).is_err(),
        "a discard with no session is refused"
    );
    let status = call(webview, "vault_status", serde_json::json!({})).unwrap();
    assert_eq!(status["unlocked"], true);
    assert_eq!(status["sessionId"], b.as_str());
    assert_eq!(
        session.finish(Access::Read, a_view).unwrap_err().kind,
        "locked",
        "A's view stays revoked"
    );

    release.send(()).unwrap();
    assert_eq!(
        reply(&pending).unwrap(),
        serde_json::json!({"added": 1, "updated": 0}),
        "B's pending import was not touched"
    );
    assert!(!demo_dev_is_empty(&b_path));

    call(webview, "view_discard", serde_json::json!({"sessionId": b})).unwrap();
    assert_ne!(
        session.snapshot().unwrap().presentation,
        b_before.presentation,
        "B's own discard reaches B"
    );
    let notices = delivered.lock().unwrap();
    assert_eq!(notices.len(), notices_before + 1);
    assert!(notices[notices_before].contains(&b));
    println!("PASS: view_discard names its session; a discard for A, an unknown id or a malformed id leaves B intact and succeeds, and B's own discard still reaches B.");
}

fn send(
    webview: &tauri::Webview<MockRuntime>,
    command: &str,
    args: serde_json::Value,
) -> mpsc::Receiver<InvokeResponse> {
    let (response_tx, response_rx) = mpsc::channel();
    let target = webview.clone();
    let command = command.to_string();
    let args = scoped(&command, args);
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
