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
#[path = "../src/disclosure.rs"]
#[allow(unused_imports)]
mod disclosure;
#[path = "../src/error.rs"]
#[allow(unused_imports)]
mod error;
#[path = "../src/import_review.rs"]
#[allow(unused_imports)]
mod import_review;
#[path = "../src/launch.rs"]
#[allow(unused_imports)]
mod launch;
#[path = "../src/outcome.rs"]
mod outcome;
#[path = "../src/policy.rs"]
mod policy;
#[path = "../src/private_export.rs"]
mod private_export;
#[cfg(windows)]
#[path = "../src/process.rs"]
mod process;
#[path = "../src/publication.rs"]
#[allow(unused_imports)]
mod publication;
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
#[cfg(windows)]
#[path = "../src/windows_session.rs"]
mod windows_session;

use commands::*;
use session::{Access, Held, MonotonicClock, Session, VaultState};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody, InvokeResponse, InvokeResponseBody},
    test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
};

/// The bound of a wait for an event that must happen: the same value as
/// `test_support::PATIENCE` in the library (class Q).
const PATIENCE: Duration = Duration::from_secs(120);

type Clipboard = Arc<Mutex<clipboard_fake::FakeState>>;

fn clipboard_text(state: &Clipboard) -> String {
    state.lock().unwrap().text()
}

/// The native confirmation, scripted. `arm` makes the next dialog wait until the test answers it.
struct Dialog {
    approve: AtomicBool,
    window_live: AtomicBool,
    hold: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    /// How many windows opened, and the text of the last one.
    opened: AtomicUsize,
    last: Mutex<String>,
    /// The next import panics right after it replaced the vault file (U1-R1-04).
    fault_after_import: AtomicBool,
}
impl Dialog {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            approve: AtomicBool::new(true),
            window_live: AtomicBool::new(true),
            hold: Mutex::new(None),
            opened: AtomicUsize::new(0),
            last: Mutex::new(String::new()),
            fault_after_import: AtomicBool::new(false),
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
    fn review_dialog(&self) -> Box<dyn review_window::ReviewDialog + '_> {
        Box::new(self)
    }
    fn window_live(&self) -> bool {
        self.window_live.load(Ordering::SeqCst)
    }
    fn imported(&self) {
        if self.fault_after_import.swap(false, Ordering::SeqCst) {
            panic!("a fake fault right after the import replaced the vault file");
        }
    }
}
impl review_window::ReviewDialog for &Dialog {
    fn show(
        &self,
        review: &review_window::ReviewText,
    ) -> Result<i32, review_window::ReviewFailure> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        *self.last.lock().unwrap() = review.text.clone();
        let held = self.hold.lock().unwrap().take();
        if let Some((entered, release)) = held {
            entered.send(()).unwrap();
            release
                .recv_timeout(PATIENCE)
                .expect("the test answers the dialog");
        }
        Ok(if self.approve.load(Ordering::SeqCst) {
            review_window::CONFIRM_BUTTON
        } else {
            review_window::CANCEL_BUTTON
        })
    }
}

/// The native terminal, scripted. It opens no window and starts no process: it records what
/// native code would have started. `arm` makes the next review wait until the test answers it,
/// and `at` runs a test step (a lock, a page load, a fault) when the launch reaches a step.
type Started = Vec<(std::path::PathBuf, Vec<(String, String)>)>;
type Step = (launch::LaunchStep, Box<dyn FnOnce() + Send>);
struct Terminal {
    /// The folder that the picker answers. `None` is a cancel.
    folder: Mutex<Option<std::path::PathBuf>>,
    approve: AtomicBool,
    live: AtomicBool,
    fail_prepare: AtomicBool,
    /// The next start panics right after the resume, under the gate (U1-R1-04).
    fault_after_resume: AtomicBool,
    reviews: AtomicUsize,
    hold: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    step: Mutex<Option<Step>>,
    started: Mutex<Started>,
}
impl Terminal {
    fn new(folder: &std::path::Path) -> Arc<Self> {
        Arc::new(Self {
            folder: Mutex::new(Some(folder.to_path_buf())),
            approve: AtomicBool::new(true),
            live: AtomicBool::new(true),
            fail_prepare: AtomicBool::new(false),
            fault_after_resume: AtomicBool::new(false),
            reviews: AtomicUsize::new(0),
            hold: Mutex::default(),
            step: Mutex::default(),
            started: Mutex::default(),
        })
    }

    /// `entered` fires when the review opens. Send on `release` to answer it.
    fn arm(&self) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        *self.hold.lock().unwrap() = Some((entered_tx, release_rx));
        (entered_rx, release_tx)
    }

    fn at(&self, step: launch::LaunchStep, run: impl FnOnce() + Send + 'static) {
        *self.step.lock().unwrap() = Some((step, Box::new(run)));
    }
}
impl launch::TerminalHost for Terminal {
    fn window_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }
    fn choose_folder(&self) -> Result<Option<std::path::PathBuf>, error::CommandError> {
        Ok(self.folder.lock().unwrap().clone())
    }
    fn review_dialog(&self) -> Box<dyn review_window::ReviewDialog + '_> {
        Box::new(self)
    }
    fn reached(&self, step: launch::LaunchStep) {
        let mut slot = self.step.lock().unwrap();
        if slot.as_ref().is_some_and(|(at, _)| *at == step) {
            let (_, run) = slot.take().unwrap();
            drop(slot);
            run();
        }
    }
    fn prepare_terminal(
        &self,
        request: &launch::LaunchRequest,
    ) -> Result<Box<dyn session::Effect<Output = ()> + '_>, error::CommandError> {
        if self.fail_prepare.load(Ordering::SeqCst) {
            return Err(error::CommandError::new(
                "terminal",
                "The fake terminal could not be made.",
            ));
        }
        Ok(Box::new(FakeStart {
            started: &self.started,
            request: Some((request.folder.clone(), request.secrets.to_vec())),
            fault: self.fault_after_resume.swap(false, Ordering::SeqCst),
        }))
    }
}

/// A prepared start: the commit records it, a revoke drops it. With `fault`, the act panics
/// right after the start, and the revoke ends that start, as the real revoke terminates the
/// process.
struct FakeStart<'a> {
    started: &'a Mutex<Started>,
    request: Option<(std::path::PathBuf, Vec<(String, String)>)>,
    fault: bool,
}
impl session::Effect for FakeStart<'_> {
    type Output = ();
    fn act(&mut self, _: &Held<'_>) -> Result<(), error::CommandError> {
        self.started.lock().unwrap().extend(self.request.take());
        if self.fault {
            panic!("a fake fault right after the resume");
        }
        Ok(())
    }
    fn revoke(&mut self) -> Result<(), String> {
        if self.fault {
            self.started.lock().unwrap().pop();
        }
        self.request = None;
        Ok(())
    }
}

impl review_window::ReviewDialog for &Terminal {
    fn show(&self, _: &review_window::ReviewText) -> Result<i32, review_window::ReviewFailure> {
        self.reviews.fetch_add(1, Ordering::SeqCst);
        let held = self.hold.lock().unwrap().take();
        if let Some((entered, release)) = held {
            entered.send(()).unwrap();
            release
                .recv_timeout(PATIENCE)
                .expect("the test answers the review");
        }
        Ok(if self.approve.load(Ordering::SeqCst) {
            review_window::CONFIRM_BUTTON
        } else {
            review_window::CANCEL_BUTTON
        })
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
    "audit_recover",
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
        .recv_timeout(PATIENCE)
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
    let session = Session::new(
        Arc::new(MonotonicClock::default()),
        Box::new(clipboard_fake::TransactionBackend(clipboard_fake::FakeApi(
            clipboard.clone(),
        ))),
        Arc::new(|_| {}),
    );
    session.observe(policy::Signal::Available);
    let dialog = Dialog::new();
    let terminal = Terminal::new(directory.path());
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
    let app = mock_builder()
        .manage(VaultState(session.clone()))
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
        entered_rx.recv_timeout(PATIENCE).unwrap();
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
            .or_else(|| response_rx.recv_timeout(PATIENCE).ok())
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
    every_dispatched_launch_leaves_one_record(webview, &session, &terminal, directory.path());
    a_core_warning_reaches_the_page_with_its_own_kind(webview, directory.path());
    a_hidden_window_at_the_import_check_leaves_a_failed_record(webview, &dialog, directory.path());
    a_fault_after_the_import_replacement_leaves_a_true_record(webview, &dialog, directory.path());
    the_history_command_answers_times_and_field_names(webview, directory.path());
    a_foreign_entry_with_history_shows_only_known_field_names(webview, directory.path());
    a_page_that_holds_the_session_id_cannot_disclose_without_the_user(
        webview,
        &dialog,
        &clipboard,
        directory.path(),
    );
    a_damaged_journal_is_replaced_only_after_the_user_approves(webview, &dialog, directory.path());
    a_stale_request_never_acts_on_the_session_that_replaced_its_own(
        webview,
        &session,
        &clipboard,
        directory.path(),
    );
}

/// FR-03. A compromised page reads its session ID and calls the disclosing commands directly,
/// with no click. Through the production handlers: no value leaves and nothing reaches the
/// clipboard unless the user approved it in the native window. A decline is a typed `null`.
fn a_page_that_holds_the_session_id_cannot_disclose_without_the_user(
    webview: &tauri::Webview<MockRuntime>,
    dialog: &Arc<Dialog>,
    clipboard: &Clipboard,
    directory: &std::path::Path,
) {
    let (_session_id, _) = open_new(webview, directory, "compromised.kdbx");
    let mut ids = Vec::new();
    for (title, secret, project, variable) in [
        ("Fake page one", "fake-page-secret-one", "demo", "PAGE_ONE"),
        ("Fake page two", "fake-page-secret-two", "demo", "PAGE_TWO"),
        (
            "Fake page three",
            "fake-page-secret-three",
            "other",
            "PAGE_THREE",
        ),
    ] {
        let added = call(
            webview,
            "key_add",
            serde_json::json!({"draft": {"title": title, "secret": secret, "project": project, "environment": "dev", "envVar": variable}}),
        )
        .unwrap();
        ids.push(added["id"].as_str().unwrap().to_string());
    }
    let secrets = [
        "fake-page-secret-one",
        "fake-page-secret-two",
        "fake-page-secret-three",
    ];
    let leaked = |text: &str| secrets.iter().any(|secret| text.contains(secret));
    let sequence = clipboard.lock().unwrap().sequence;
    dialog.approve.store(false, Ordering::SeqCst);
    let opened = dialog.opened.load(Ordering::SeqCst);

    // Bulk: all keys and one scope, as a preview and as a copy.
    for scope in [
        serde_json::json!({"project": null, "environment": null, "all": true}),
        serde_json::json!({"project": "demo", "environment": "dev", "all": false}),
    ] {
        for command in ["env_export", "env_copy"] {
            let answer = call(webview, command, scope.clone()).unwrap();
            assert_eq!(answer, serde_json::Value::Null, "{command}: declined");
        }
    }
    assert_eq!(
        dialog.opened.load(Ordering::SeqCst) - opened,
        4,
        "each bulk request opened the native window"
    );
    let window = dialog.last.lock().unwrap().clone();
    assert!(
        window.contains("PAGE_ONE") && window.contains("PAGE_TWO"),
        "{window}"
    );
    assert!(!leaked(&window), "the window shows no value: {window}");
    assert_eq!(
        clipboard.lock().unwrap().sequence,
        sequence,
        "no clipboard write"
    );
    assert!(!leaked(&clipboard_text(clipboard)));

    // Single keys: a loop of reveals and copies. Only the free budget leaves the vault.
    let opened = dialog.opened.load(Ordering::SeqCst);
    let (mut shown, mut copied, mut asked) = (0, 0, 0);
    for round in 0..30 {
        let reveal = call(
            webview,
            "key_reveal",
            serde_json::json!({"id": ids[round % 3]}),
        )
        .unwrap();
        if let Some(value) = reveal.as_str() {
            assert!(secrets.contains(&value));
            shown += 1;
        } else {
            assert_eq!(reveal, serde_json::Value::Null);
            asked += 1;
        }
        let before_copy = clipboard.lock().unwrap().sequence;
        let copy = call(
            webview,
            "key_copy",
            serde_json::json!({"id": ids[round % 3]}),
        )
        .unwrap();
        if copy.is_null() {
            asked += 1;
            assert_eq!(
                clipboard.lock().unwrap().sequence,
                before_copy,
                "a declined copy writes nothing"
            );
        } else {
            copied += 1;
            assert_ne!(clipboard.lock().unwrap().sequence, before_copy);
        }
    }
    assert_eq!(
        shown + copied,
        session::FREE_SINGLE_DISCLOSURES,
        "only the free budget left the vault"
    );
    assert_eq!(
        dialog.opened.load(Ordering::SeqCst) - opened,
        asked,
        "every other request opened the native window"
    );

    // The user approves: the same call now returns the value.
    dialog.approve.store(true, Ordering::SeqCst);
    let preview = call(
        webview,
        "env_export",
        serde_json::json!({"project": "demo", "environment": "dev", "all": false}),
    )
    .unwrap();
    let preview = preview.as_str().expect("an approved preview");
    assert!(
        preview.contains("PAGE_ONE=fake-page-secret-one"),
        "{preview}"
    );
    let copied_after = call(
        webview,
        "env_copy",
        serde_json::json!({"project": "demo", "environment": "dev", "all": false}),
    )
    .unwrap();
    assert_eq!(copied_after, serde_json::json!(30));
    assert!(clipboard_text(clipboard).contains("PAGE_TWO=fake-page-secret-two"));
    println!("PASS: env_export, env_copy, key_reveal and key_copy called by a page with only the session ID disclose nothing and write no clipboard without the native approval; a decline answers null; the single-key budget stops a loop at the free count.");
}

/// FR-14, the reviewer's probe through the production handlers: damage the journal, a reveal is
/// refused, the user declines and nothing changes, the user approves, a reveal works again, and
/// the damaged bytes are kept as they were. The journal folder is the Cargo test root.
fn a_damaged_journal_is_replaced_only_after_the_user_approves(
    webview: &tauri::Webview<MockRuntime>,
    dialog: &Arc<Dialog>,
    directory: &std::path::Path,
) {
    use sha2::{Digest, Sha256};
    let (_session_id, vault) = open_new(webview, directory, "recovery.kdbx");
    dialog.approve.store(true, Ordering::SeqCst);
    let added = call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake probe key", "secret": "fake-probe-secret", "project": "demo", "environment": "dev", "envVar": "PROBE_KEY"}}),
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_string();
    let reveal = |expect_ok: bool| {
        let answer = call(webview, "key_reveal", serde_json::json!({"id": id}));
        assert_eq!(answer.is_ok(), expect_ok, "{answer:?}");
        answer
    };
    assert_eq!(reveal(true).unwrap(), "fake-probe-secret");

    // Not damaged: a recovery is refused and no window opens.
    let opened = dialog.opened.load(Ordering::SeqCst);
    let refused = call(webview, "audit_recover", serde_json::json!({})).unwrap_err();
    assert_eq!(refused["kind"], "journal");
    assert_eq!(
        dialog.opened.load(Ordering::SeqCst),
        opened,
        "no window for an intact journal"
    );

    // Damage: a line that is no record, appended to the journal of this vault.
    let journal = akm_core::audit::AuditLog::for_vault(&vault)
        .journal_path()
        .unwrap()
        .expect("the add made a journal");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap();
        file.write_all(b"{this is not a record}\n").unwrap();
    }
    let damaged = std::fs::read(&journal).unwrap();
    let report = call(webview, "audit_read", serde_json::json!({"limit": 10})).unwrap();
    assert_eq!(report["state"], "corrupt");
    for command in ["key_reveal", "key_copy"] {
        let error = call(webview, command, serde_json::json!({"id": id})).unwrap_err();
        assert_eq!(error["kind"], "journal", "{command}: disclosure is blocked");
    }
    assert_eq!(
        std::fs::read(&journal).unwrap(),
        damaged,
        "a refusal changes nothing"
    );

    // The user declines: null, the bytes and the block stay.
    dialog.approve.store(false, Ordering::SeqCst);
    let opened = dialog.opened.load(Ordering::SeqCst);
    assert_eq!(
        call(webview, "audit_recover", serde_json::json!({})).unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(dialog.opened.load(Ordering::SeqCst), opened + 1);
    let window = dialog.last.lock().unwrap().clone();
    assert!(
        window.contains(journal.file_name().unwrap().to_str().unwrap()),
        "{window}"
    );
    assert!(window.contains("cannot be proved complete"), "{window}");
    assert!(!window.contains("fake-probe-secret"));
    assert_eq!(
        std::fs::read(&journal).unwrap(),
        damaged,
        "a decline changes nothing"
    );
    // The single-key budget counts across sessions, so an earlier check may have spent it.
    // A budget window approves here, and the damaged journal itself must refuse the reveal.
    dialog.approve.store(true, Ordering::SeqCst);
    assert_eq!(reveal(false).unwrap_err()["kind"], "journal");

    // The user approves: the damaged file is kept byte for byte and disclosure works again.
    let kept = call(webview, "audit_recover", serde_json::json!({})).unwrap();
    let kept_name = kept["keptName"].as_str().unwrap();
    assert!(kept_name.contains(".damaged-"), "{kept_name}");
    let kept_path = journal.with_file_name(kept_name);
    assert_eq!(
        std::fs::read(&kept_path).unwrap(),
        damaged,
        "the old bytes are kept"
    );
    let digest: String = Sha256::digest(&damaged)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(kept["sha256"], digest.as_str());
    assert_eq!(kept["bytes"], damaged.len());
    assert_eq!(reveal(true).unwrap(), "fake-probe-secret");
    let report = call(webview, "audit_read", serde_json::json!({"limit": 10})).unwrap();
    assert_eq!(report["state"], "consistent");
    let entries = report["entries"].as_array().unwrap();
    assert_eq!(entries[0]["action"], "journal-recovered");
    assert!(entries[0]["detail"].as_str().unwrap().contains(&digest));
    assert_eq!(
        std::fs::read(&kept_path).unwrap(),
        damaged,
        "the disclosure left the kept file alone"
    );

    // Recovered: a second recovery is refused, with no window.
    let opened = dialog.opened.load(Ordering::SeqCst);
    assert_eq!(
        call(webview, "audit_recover", serde_json::json!({})).unwrap_err()["kind"],
        "journal"
    );
    assert_eq!(dialog.opened.load(Ordering::SeqCst), opened);
    println!("PASS: audit_recover refuses an intact journal, a decline changes nothing, an approval keeps the damaged bytes and their SHA-256 and lets a reveal work again, and the first record of the new journal states the gap.");
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
        ("audit_recover", serde_json::json!({})),
        ("env_export", scope.clone()),
        ("env_copy", scope.clone()),
        ("env_save", scope.clone()),
        (
            "env_launch_terminal",
            serde_json::json!({"project": "demo", "environment": "dev", "operationId": uuid::Uuid::new_v4().to_string()}),
        ),
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
    println!("PASS: all 21 session-scoped handlers refuse a request that names a retired session, leave B and the clipboard untouched, and still serve B's own requests.");
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
        "operationId": uuid::Uuid::new_v4().to_string(),
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
    let mut hostile = hostile;
    hostile["operationId"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
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
    let mut unscoped =
        serde_json::json!({"project": "demo", "operationId": uuid::Uuid::new_v4().to_string()});
    unscoped["sessionId"] = serde_json::json!(owner);
    assert_eq!(
        call(webview, "env_launch_terminal", unscoped).unwrap_err()["kind"],
        "invalid",
        "a scope needs both parts"
    );
    assert_eq!(terminal.started.lock().unwrap().len(), 1);
    println!("PASS: env_launch_terminal is registered, answers a count or null, ignores every program, path and argument that a page adds, and refuses a stale session.");
}

/// Class N, through the production handlers. Every launch that passes its session check leaves
/// exactly one outcome record, on every way out, and the record does not depend on the reply:
/// the test reads each reply as a page that lost it would, and then reads the record. A request
/// with a malformed operation ID, or of a session that is not the current one, is refused with
/// no record. A record of session A changes nothing in session B.
fn every_dispatched_launch_leaves_one_record(
    webview: &tauri::Webview<MockRuntime>,
    session: &Arc<Session>,
    terminal: &Arc<Terminal>,
    directory: &std::path::Path,
) {
    use launch::LaunchStep;
    let scope = |operation: &str| serde_json::json!({"operationId": operation, "project": "demo", "environment": "dev"});
    let fresh = |name: &str| {
        let (owner, _) = open_new(webview, directory, name);
        call(
            webview,
            "key_add",
            serde_json::json!({"draft": {"title": "Fake outcome", "secret": "fake-outcome-secret", "project": "demo", "environment": "dev", "envVar": "OUTCOME_TOKEN"}}),
        )
        .unwrap();
        owner
    };
    let started = || terminal.started.lock().unwrap().len();
    let only_record = |operation: &str, kind: &str, case: &str| {
        assert_eq!(record_kinds(webview, operation), [kind], "{case}");
        call(
            webview,
            "vault_dismiss_warnings",
            serde_json::json!({"ids": [operation]}),
        )
        .unwrap();
        assert!(
            record_kinds(webview, operation).is_empty(),
            "{case}: acknowledged"
        );
    };
    let lock = {
        let session = session.clone();
        move || {
            let _ = session.lock();
        }
    };
    let mut owner = fresh("outcome-0.kdbx");

    let malformed = call(webview, "env_launch_terminal", scope("not-a-uuid")).unwrap_err();
    assert_eq!(malformed["kind"], "invalid");
    let stale_operation = uuid::Uuid::new_v4().to_string();
    let mut stale = scope(&stale_operation);
    stale["sessionId"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        call(webview, "env_launch_terminal", stale).unwrap_err()["kind"],
        "locked"
    );
    assert!(record_kinds(webview, &stale_operation).is_empty());
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    assert!(!report.to_string().contains("not-a-uuid"));

    // The ways out before the start. None of them starts a terminal.
    let before = started();
    type Case<'a> = (
        &'a str,
        &'a str,
        &'a str,
        Box<dyn Fn() + 'a>,
        Box<dyn Fn() + 'a>,
    );
    let cases: Vec<Case> = vec![
        (
            "a request without an environment",
            "invalid",
            "terminalFailed",
            Box::new(|| {}),
            Box::new(|| {}),
        ),
        (
            "a hidden window at the window check",
            "viewChanged",
            "terminalFailed",
            Box::new(|| terminal.live.store(false, Ordering::SeqCst)),
            Box::new(|| terminal.live.store(true, Ordering::SeqCst)),
        ),
        (
            "a cancelled folder picker",
            "null",
            "terminalDeclined",
            Box::new(|| *terminal.folder.lock().unwrap() = None),
            Box::new(|| *terminal.folder.lock().unwrap() = Some(directory.to_path_buf())),
        ),
        (
            "a folder that does not exist",
            "invalidPath",
            "terminalFailed",
            Box::new(|| *terminal.folder.lock().unwrap() = Some(directory.join("missing-folder"))),
            Box::new(|| *terminal.folder.lock().unwrap() = Some(directory.to_path_buf())),
        ),
        (
            "a declined review",
            "null",
            "terminalDeclined",
            Box::new(|| terminal.approve.store(false, Ordering::SeqCst)),
            Box::new(|| terminal.approve.store(true, Ordering::SeqCst)),
        ),
        (
            "a terminal that could not be made",
            "terminal",
            "terminalFailed",
            Box::new(|| terminal.fail_prepare.store(true, Ordering::SeqCst)),
            Box::new(|| terminal.fail_prepare.store(false, Ordering::SeqCst)),
        ),
        (
            "a fault after the approval",
            "internal",
            "terminalFailed",
            Box::new(|| {
                terminal.at(LaunchStep::Approved, || {
                    panic!("a fake fault in the launch")
                })
            }),
            Box::new(|| {}),
        ),
    ];
    for (case, reply, kind, arrange, restore) in &cases {
        let operation = uuid::Uuid::new_v4().to_string();
        arrange();
        let mut args = scope(&operation);
        if *case == "a request without an environment" {
            args["environment"] = serde_json::Value::Null;
        }
        let lost = call(webview, "env_launch_terminal", args);
        restore();
        match lost {
            Ok(value) => assert_eq!(value.to_string(), *reply, "{case}"),
            Err(error) => assert_eq!(error["kind"], *reply, "{case}"),
        }
        only_record(&operation, kind, case);
        assert_eq!(started(), before, "{case}: nothing started");
    }

    // A start, and a fault after it: the record says that the terminal started. After the
    // fault, no reply invites a retry: the session ends, and the reply says that the launch
    // completed (U1-R1-04).
    for case in ["a start", "a fault after the start"] {
        let operation = uuid::Uuid::new_v4().to_string();
        let fault = case == "a fault after the start";
        if fault {
            terminal.at(LaunchStep::Started, || {
                panic!("a fake fault after the start")
            });
        }
        let before = started();
        let lost = call(webview, "env_launch_terminal", scope(&operation));
        match lost {
            Ok(count) => assert!(!fault && count == serde_json::json!(1), "{case}"),
            Err(error) => assert!(fault && error["kind"] == "completedWhileLocked", "{case}"),
        }
        let kind = if fault {
            "terminalSessionEnded"
        } else {
            "terminalStarted"
        };
        only_record(&operation, kind, case);
        assert_eq!(started(), before + 1, "{case}");
    }
    assert_eq!(
        call(webview, "vault_status", serde_json::json!({})).unwrap()["unlocked"],
        false,
        "the fault after the start ended the session"
    );

    // A fault right after the resume, under the gate: the gate is not poisoned, the start is
    // revoked, the record says failed, the reply does not invite a retry, and the session ends.
    fresh("outcome-resume.kdbx");
    let operation = uuid::Uuid::new_v4().to_string();
    terminal.fault_after_resume.store(true, Ordering::SeqCst);
    let before = started();
    let lost = call(webview, "env_launch_terminal", scope(&operation));
    assert!(
        !terminal.fault_after_resume.load(Ordering::SeqCst),
        "the start was prepared"
    );
    let error = lost.unwrap_err();
    assert_eq!(error["kind"], "internal");
    assert!(!error["message"].as_str().unwrap().contains("Try again"));
    only_record(
        &operation,
        "terminalFailed",
        "a fault right after the resume",
    );
    assert_eq!(started(), before, "the revoke ended the start");
    assert_eq!(
        call(webview, "vault_status", serde_json::json!({})).unwrap()["unlocked"],
        false,
        "the fault under the gate ended the session"
    );
    fresh("outcome-after-resume.kdbx");

    // A page load while the review is open, then the approval: the page that replaced the one
    // that sent the request finds the start in the record.
    let operation = uuid::Uuid::new_v4().to_string();
    let (entered, release) = terminal.arm();
    let pending = send(webview, "env_launch_terminal", scope(&operation));
    entered.recv_timeout(PATIENCE).unwrap();
    session.document_started();
    release.send(()).unwrap();
    let _lost = reply(&pending);
    only_record(
        &operation,
        "terminalStarted",
        "a page load during the review",
    );

    // A lock at each step around the start.
    for (case, step, reply_kind, kind, starts) in [
        (
            "a lock during the review",
            None,
            "locked",
            "terminalFailed",
            false,
        ),
        (
            "a lock at the prepared step",
            Some(LaunchStep::Prepared),
            "locked",
            "terminalFailed",
            false,
        ),
        (
            "a lock after the start",
            Some(LaunchStep::Started),
            "1",
            "terminalSessionEnded",
            true,
        ),
    ] {
        let operation = uuid::Uuid::new_v4().to_string();
        let before = started();
        let lost = match step {
            None => {
                let (entered, release) = terminal.arm();
                let pending = send(webview, "env_launch_terminal", scope(&operation));
                entered.recv_timeout(PATIENCE).unwrap();
                lock();
                release.send(()).unwrap();
                reply(&pending)
            }
            Some(step) => {
                terminal.at(step, lock.clone());
                call(webview, "env_launch_terminal", scope(&operation))
            }
        };
        match lost {
            Ok(value) => assert_eq!(value.to_string(), reply_kind, "{case}"),
            Err(error) => assert_eq!(error["kind"], reply_kind, "{case}"),
        }
        only_record(&operation, kind, case);
        assert_eq!(started(), before + usize::from(starts), "{case}");
        owner = fresh(&format!("outcome-{}.kdbx", case.len()));
    }

    // Session A asks; session B opens while A's review is open; A is refused. A's record names
    // A, and B's own launch, record and acknowledgement are B's alone.
    let a_operation = uuid::Uuid::new_v4().to_string();
    let (entered, release) = terminal.arm();
    let pending = send(webview, "env_launch_terminal", scope(&a_operation));
    entered.recv_timeout(PATIENCE).unwrap();
    let b_owner = fresh("outcome-b.kdbx");
    assert_ne!(b_owner, owner);
    let b_before = session.snapshot().unwrap();
    release.send(()).unwrap();
    // The access after the review finds another session current.
    assert_eq!(reply(&pending).unwrap_err()["kind"], "stalePreview");
    assert_eq!(record_kinds(webview, &a_operation), ["terminalFailed"]);
    let b_after = session.snapshot().unwrap();
    assert_eq!(
        (b_after.ticket, b_after.revision, b_after.presentation),
        (b_before.ticket, b_before.revision, b_before.presentation),
        "A's record changed nothing in B"
    );
    let b_operation = uuid::Uuid::new_v4().to_string();
    let before = started();
    assert_eq!(
        call(webview, "env_launch_terminal", scope(&b_operation)).unwrap(),
        serde_json::json!(1)
    );
    assert_eq!(started(), before + 1);
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    let origin = |operation: &str| {
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["id"] == operation)
            .unwrap()["originSession"]
            .clone()
    };
    assert_ne!(origin(&a_operation), origin(&b_operation));
    only_record(&a_operation, "terminalFailed", "session A");
    only_record(&b_operation, "terminalStarted", "session B");
    println!("PASS: every env_launch_terminal that passed its session check left exactly one outcome record (started, session ended, failed or declined) on each way out, including a hidden window, a fault before and after the start and a lost reply; a malformed ID or a stale session left none; A's record changed nothing in B.");
}

/// T-R3-05, through the production handlers. A hidden window at the window check refuses the
/// import before any dialog, and the request still leaves its failed record.
fn a_hidden_window_at_the_import_check_leaves_a_failed_record(
    webview: &tauri::Webview<MockRuntime>,
    dialog: &Arc<Dialog>,
    directory: &std::path::Path,
) {
    let (owner, path) = open_new(webview, directory, "hidden-import.kdbx");
    let operation = uuid::Uuid::new_v4().to_string();
    let opened = dialog.opened.load(Ordering::SeqCst);
    dialog.window_live.store(false, Ordering::SeqCst);
    let lost = call(
        webview,
        "env_import",
        import_args(&owner, &operation, "TOKEN=fake-hidden\n"),
    );
    dialog.window_live.store(true, Ordering::SeqCst);
    assert_eq!(lost.unwrap_err()["kind"], "viewChanged");
    assert_eq!(dialog.opened.load(Ordering::SeqCst), opened, "no dialog");
    assert_eq!(record_kinds(webview, &operation), ["importFailed"]);
    assert!(demo_dev_is_empty(&path), "nothing was written");
    call(
        webview,
        "vault_dismiss_warnings",
        serde_json::json!({"ids": [operation]}),
    )
    .unwrap();
    println!("PASS: env_import with a hidden window opened no dialog, wrote nothing and left one importFailed record.");
}

/// U1-R1-04, through the production handlers. A fault right after the import replaced the vault
/// file leaves one importSessionEnded record, and the reply says that the import completed. The
/// session ends, the file holds the import, and the next opening works.
fn a_fault_after_the_import_replacement_leaves_a_true_record(
    webview: &tauri::Webview<MockRuntime>,
    dialog: &Arc<Dialog>,
    directory: &std::path::Path,
) {
    let (owner, path) = open_new(webview, directory, "fault-import.kdbx");
    let operation = uuid::Uuid::new_v4().to_string();
    dialog.fault_after_import.store(true, Ordering::SeqCst);
    let lost = call(
        webview,
        "env_import",
        import_args(&owner, &operation, "TOKEN=fake-imported\n"),
    );
    assert!(
        !dialog.fault_after_import.load(Ordering::SeqCst),
        "the fault was reached"
    );
    assert_eq!(lost.unwrap_err()["kind"], "completedWhileLocked");
    assert_eq!(record_kinds(webview, &operation), ["importSessionEnded"]);
    assert_eq!(
        call(webview, "vault_status", serde_json::json!({})).unwrap()["unlocked"],
        false,
        "the session ended"
    );
    assert!(!demo_dev_is_empty(&path), "the file holds the import");
    call(
        webview,
        "vault_dismiss_warnings",
        serde_json::json!({"ids": [operation]}),
    )
    .unwrap();
    open_new(webview, directory, "after-fault-import.kdbx");
    println!("PASS: env_import with a fault right after the vault replacement left one importSessionEnded record, answered completedWhileLocked, ended the session, kept the import in the file, and the next opening worked.");
}

/// FR-12 and FR-05, through the production handlers. A vault file that an unseen clone replaced
/// continues its journal, and the page gets a notice of its own kind, not the generic journal
/// notice. The file-permission warning needs a file system that ignores private permissions,
/// which only the core's own test seam can fake, so its kind is checked on the wire as the
/// reply serializes it.
fn a_core_warning_reaches_the_page_with_its_own_kind(
    webview: &tauri::Webview<MockRuntime>,
    directory: &std::path::Path,
) {
    let before = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    let ids: Vec<serde_json::Value> = before["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["id"].clone())
        .collect();
    call(
        webview,
        "vault_dismiss_warnings",
        serde_json::json!({ "ids": ids }),
    )
    .unwrap();

    let a = directory.join("lineage-a.kdbx");
    let b = directory.join("lineage-b.kdbx");
    let original = akm_core::Vault::create(&a, PASSWORD, None).unwrap();
    original.log("fake-lineage-marker", None, None).unwrap();
    drop(original);
    std::fs::copy(&a, &b).unwrap();
    let mut clone = akm_core::Vault::open_in(
        &b,
        PASSWORD,
        akm_core::journal::JournalRoot::at(directory.join("other-journal")),
    )
    .unwrap();
    clone
        .add(akm_core::ApiKeyInput::new("Fake clone", "fake-clone-value"))
        .unwrap();
    drop(clone);
    std::fs::copy(&b, &a).unwrap();

    let reserved = call(webview, "vault_prepare_open", serde_json::json!({})).unwrap();
    let reserved = reserved.as_str().unwrap().to_string();
    call(
        webview,
        "vault_unlock",
        serde_json::json!({"path": a.display().to_string(), "password": PASSWORD, "sessionId": reserved}),
    )
    .unwrap();
    call(
        webview,
        "key_add",
        serde_json::json!({"draft": {"title": "Fake lineage", "secret": "fake-lineage-secret"}}),
    )
    .unwrap();
    let report = call(webview, "vault_warnings", serde_json::json!({})).unwrap();
    let kinds: Vec<&str> = report["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["journalLineageUnconfirmed"]);
    assert!(
        !report.to_string().contains("lineage-a"),
        "no path on the wire"
    );
    assert_eq!(
        serde_json::to_value(session::OperationWarningKind::FilePermissions).unwrap(),
        "filePermissions"
    );
    println!("PASS: a vault replaced by an unseen clone gives the page one journalLineageUnconfirmed notice and no path; filePermissions has its own name on the wire.");
}

/// T-R1-08, through the production handlers. A vault that another KeePass program wrote holds an
/// entry with native history and custom field names. `key_history` answers the known names and
/// "Other" for the rest: no custom name and no value reaches the page.
fn a_foreign_entry_with_history_shows_only_known_field_names(
    webview: &tauri::Webview<MockRuntime>,
    directory: &std::path::Path,
) {
    use keepass::db::fields;
    let path = directory.join("foreign-history.kdbx");
    let mut db = keepass::Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "Fake foreign");
        entry.set_protected(fields::PASSWORD, "fake-foreign-one");
        entry.set_unprotected("fake-secret-in-a-field-name", "fake-field-value-one");
        let before = (*entry).clone();
        entry
            .history
            .get_or_insert_with(Default::default)
            .add_entry(before);
        entry.set_protected(fields::PASSWORD, "fake-foreign-two");
        entry.set_unprotected("fake-secret-in-a-field-name", "fake-field-value-two");
        entry.set_unprotected("another-fake-name", "fake-added");
        entry.set_unprotected(fields::TITLE, "Fake foreign renamed");
    }
    db.save(
        &mut std::fs::File::create(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();

    let reserved = call(webview, "vault_prepare_open", serde_json::json!({})).unwrap();
    let reserved = reserved.as_str().unwrap().to_string();
    call(
        webview,
        "vault_unlock",
        serde_json::json!({"path": path.display().to_string(), "password": PASSWORD, "sessionId": reserved}),
    )
    .unwrap();
    let listed = call(webview, "keys_list", serde_json::json!({})).unwrap();
    let id = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["title"] == "Fake foreign renamed")
        .expect("the foreign entry is listed")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let history = call(webview, "key_history", serde_json::json!({"id": id})).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 1, "{history}");
    assert_eq!(history[0]["secretChanged"], true);
    assert_eq!(
        history[0]["changed"],
        serde_json::json!(["Password", "Title", "Other"]),
        "{history}"
    );
    let wire = history.to_string();
    assert!(
        !wire.contains("fake"),
        "no custom name and no value: {wire}"
    );
    println!("PASS: key_history of a foreign entry with history answers Password, Title and Other, and no custom field name or value.");
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
    match rx.recv_timeout(PATIENCE).expect("the command answers") {
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
        entered.recv_timeout(PATIENCE).unwrap();
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
        entered.recv_timeout(PATIENCE).unwrap();
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
    entered.recv_timeout(PATIENCE).unwrap();
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
    entered_rx.recv_timeout(PATIENCE).unwrap();
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
        .recv_timeout(PATIENCE)
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
