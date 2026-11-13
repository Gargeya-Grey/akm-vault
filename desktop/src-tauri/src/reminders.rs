//! The opt-in Windows notification (unit 4, M13).
//!
//! The text is built here from a count. The page cannot pass a title, a project or any other
//! text, so a notification can never hold more than "N API keys need attention". The preference
//! is off until the user turns it on. A notification is shown only while the app runs and the
//! session is unlocked: AKM cannot read the vault while it is locked, so there is no count to
//! show, and it runs no service, task or startup entry.
//!
//! The official `tauri-plugin-notification` is called from Rust only. The app grants the
//! webview no `notification:*` permission, so the page cannot send a notification itself.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::{
    error::CommandError,
    session::{Binding, Effect, Held, Session},
};

pub const TITLE: &str = "AKM";

/// The whole body of a notification. A count and fixed words.
pub fn notification_body(count: u32) -> String {
    if count == 1 {
        "1 API key needs attention".to_string()
    } else {
        format!("{count} API keys need attention")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Granted,
    Denied,
    Prompt,
}

/// What happened to the last notification of this run. Windows decides whether it shows a
/// toast (Focus assist, the per-app switch), and AKM cannot see that decision, so `Sent` means
/// "handed to Windows", not "seen".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Delivery {
    None,
    Sent,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderStatus {
    pub enabled: bool,
    pub permission: Permission,
    pub delivery: Delivery,
    /// AKM stopped trying for this run, after a refused permission or a failed notification.
    /// Turning the setting off and on again clears it.
    pub blocked: bool,
}

pub trait Notifier: Send + Sync {
    fn permission(&self) -> Permission;
    /// The error text is not shown or stored.
    fn show(&self, held: &Held<'_>, title: &str, body: &str) -> Result<(), String>;
}

pub struct NativeNotifier<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Notifier for NativeNotifier<R> {
    fn permission(&self) -> Permission {
        use tauri::plugin::PermissionState;
        match self.0.notification().permission_state() {
            Ok(PermissionState::Granted) => Permission::Granted,
            Ok(PermissionState::Denied) => Permission::Denied,
            Ok(PermissionState::Prompt) | Ok(PermissionState::PromptWithRationale) => {
                Permission::Prompt
            }
            Err(_) => Permission::Denied,
        }
    }

    fn show(&self, _held: &Held<'_>, title: &str, body: &str) -> Result<(), String> {
        self.0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|error| error.to_string())
    }
}

pub trait ReminderStore: Send + Sync {
    /// Off when nothing is stored, and off when the stored value cannot be read.
    fn enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> Result<(), CommandError>;
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    enabled: bool,
}

/// `reminders.json` beside the settings file. It holds one flag and never touches `settings.json`.
pub struct FileReminders(Option<PathBuf>);

impl FileReminders {
    pub fn new(file: Option<PathBuf>) -> Self {
        Self(file)
    }

    pub fn for_app<R: Runtime>(app: &AppHandle<R>) -> Self {
        Self::new(
            crate::settings::settings_file(app)
                .ok()
                .and_then(|settings| settings.parent().map(|dir| dir.join("reminders.json"))),
        )
    }
}

impl ReminderStore for FileReminders {
    fn enabled(&self) -> bool {
        let Some(file) = self.0.as_ref() else {
            return false;
        };
        fs::read_to_string(file)
            .ok()
            .and_then(|raw| serde_json::from_str::<Stored>(&raw).ok())
            .is_some_and(|stored| stored.enabled)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), CommandError> {
        let file = self.0.as_ref().ok_or_else(|| {
            CommandError::new("config", "The settings location is not available.")
        })?;
        write_atomic(file, &Stored { enabled })
    }
}

fn write_atomic(path: &Path, stored: &Stored) -> Result<(), CommandError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| CommandError::new("io", error.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(stored).map_err(CommandError::internal)?;
    fs::write(&tmp, body).map_err(|error| CommandError::new("io", error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| CommandError::new("io", error.to_string()))?;
    Ok(())
}

struct Gate {
    delivery: Delivery,
    blocked: bool,
}

pub struct Reminders {
    store: Arc<dyn ReminderStore>,
    notifier: Arc<dyn Notifier>,
    gate: Mutex<Gate>,
}

pub struct ReminderState(pub Arc<Reminders>);

impl Reminders {
    pub fn new(store: Arc<dyn ReminderStore>, notifier: Arc<dyn Notifier>) -> Self {
        Self {
            store,
            notifier,
            gate: Mutex::new(Gate {
                delivery: Delivery::None,
                blocked: false,
            }),
        }
    }

    fn gate(&self) -> Result<std::sync::MutexGuard<'_, Gate>, CommandError> {
        self.gate
            .lock()
            .map_err(|_| CommandError::internal("The reminder state failed. Restart AKM."))
    }

    fn status_of(&self, gate: &Gate) -> ReminderStatus {
        ReminderStatus {
            enabled: self.store.enabled(),
            permission: self.notifier.permission(),
            delivery: gate.delivery,
            blocked: gate.blocked,
        }
    }

    pub fn status(&self) -> Result<ReminderStatus, CommandError> {
        let gate = self.gate()?;
        Ok(self.status_of(&gate))
    }

    /// Saves the choice. A save that fails is an error the user sees, and the old choice stays.
    /// Turning the setting off or on clears an earlier block, so it is the way to try again.
    pub fn set_enabled(&self, enabled: bool) -> Result<ReminderStatus, CommandError> {
        let mut gate = self.gate()?;
        self.store.set_enabled(enabled)?;
        gate.delivery = Delivery::None;
        gate.blocked = false;
        Ok(self.status_of(&gate))
    }

    /// Shows at most one notification for `count` keys, counted in the session `session_id`.
    /// Nothing is shown when that session is not the current one, when the setting is off,
    /// when `count` is zero, or after a block. A refused permission or a failed notification
    /// blocks further tries for this run: no retry loop.
    ///
    /// The preference and the permission are read before the commit, outside the session
    /// gate. The hand-over is the act of `Session::commit`: lock and a newer opening wait for
    /// that one call, so neither can land between the check and the hand-over. A toast that
    /// the notifier already passed to Windows cannot be withdrawn.
    pub fn notify(
        &self,
        session: &Session,
        session_id: &str,
        count: u32,
    ) -> Result<ReminderStatus, CommandError> {
        if count == 0 {
            return Err(CommandError::new(
                "invalid",
                "There is nothing to report: no key needs attention.",
            ));
        }
        let mut gate = self.gate()?;
        // A request of a session that is not the current one is refused, whatever the setting.
        session.snapshot_for(session_id)?;
        if !self.store.enabled() || gate.blocked {
            return Ok(self.status_of(&gate));
        }
        if self.notifier.permission() != Permission::Granted {
            gate.blocked = true;
            return Ok(self.status_of(&gate));
        }
        let mut toast = Toast {
            notifier: self.notifier.as_ref(),
            body: notification_body(count),
        };
        match session.commit(Binding::Named(session_id), &mut toast)? {
            Ok(()) => gate.delivery = Delivery::Sent,
            Err(_) => {
                gate.delivery = Delivery::Failed;
                gate.blocked = true;
            }
        }
        Ok(self.status_of(&gate))
    }
}

/// One notification, ready to hand to Windows. A failed hand-over is the act's answer, not a
/// refusal: the caller records it as a failed delivery.
struct Toast<'a> {
    notifier: &'a dyn Notifier,
    body: String,
}

impl Effect for Toast<'_> {
    type Output = Result<(), String>;

    fn act(&mut self, held: &Held<'_>) -> Result<Self::Output, CommandError> {
        Ok(self.notifier.show(held, TITLE, &self.body))
    }

    fn revoke(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clipboard::tests::{FakeClipboard, FakeData},
        policy::Signal,
        session::MonotonicClock,
    };

    #[derive(Default)]
    struct FakeNotifier {
        shown: Mutex<Vec<(String, String)>>,
        permission: Mutex<Option<Permission>>,
        refuse: Mutex<bool>,
        /// The first `show` reports it entered, then waits to be released.
        hold: Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>,
    }

    impl FakeNotifier {
        fn shown(&self) -> Vec<(String, String)> {
            self.shown.lock().unwrap().clone()
        }
        fn set_permission(&self, permission: Permission) {
            *self.permission.lock().unwrap() = Some(permission);
        }
        fn refuse(&self, refuse: bool) {
            *self.refuse.lock().unwrap() = refuse;
        }
    }

    impl Notifier for FakeNotifier {
        fn permission(&self) -> Permission {
            self.permission
                .lock()
                .unwrap()
                .unwrap_or(Permission::Granted)
        }
        fn show(&self, _held: &Held<'_>, title: &str, body: &str) -> Result<(), String> {
            let held = self.hold.lock().unwrap().take();
            if let Some((entered, release)) = held {
                entered.send(()).unwrap();
                release.recv_timeout(crate::test_support::PATIENCE).unwrap();
            }
            if *self.refuse.lock().unwrap() {
                return Err("fake refusal".into());
            }
            self.shown.lock().unwrap().push((title.into(), body.into()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryStore(Mutex<bool>);
    impl ReminderStore for MemoryStore {
        fn enabled(&self) -> bool {
            *self.0.lock().unwrap()
        }
        fn set_enabled(&self, enabled: bool) -> Result<(), CommandError> {
            *self.0.lock().unwrap() = enabled;
            Ok(())
        }
    }

    fn unlocked() -> (Arc<Session>, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        open_vault(&session, directory.path());
        (session, directory)
    }

    fn open_vault(session: &Session, directory: &Path) {
        let path = directory.join("vault.kdbx");
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                if path.exists() {
                    akm_core::Vault::open(&path, "fake-test-password-123!")
                } else {
                    akm_core::Vault::create(&path, "fake-test-password-123!", None)
                }
                .map_err(Into::into)
            })
            .unwrap();
    }

    fn owner(session: &Session) -> String {
        session
            .active_owner(session.snapshot().unwrap().ticket)
            .unwrap()
    }

    fn owner_of_nothing() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn setup(enabled: bool) -> (Reminders, Arc<FakeNotifier>) {
        let notifier = Arc::new(FakeNotifier::default());
        let store = Arc::new(MemoryStore::default());
        store.set_enabled(enabled).unwrap();
        (Reminders::new(store, notifier.clone()), notifier)
    }

    #[test]
    fn the_text_is_a_count_and_fixed_words() {
        assert_eq!(notification_body(1), "1 API key needs attention");
        assert_eq!(notification_body(2), "2 API keys need attention");
        assert_eq!(notification_body(10), "10 API keys need attention");
        assert_eq!(TITLE, "AKM");
    }

    #[test]
    fn the_setting_is_off_by_default_and_a_disabled_setting_shows_nothing() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(false);
        let status = reminders.status().unwrap();
        assert!(!status.enabled);
        assert_eq!(status.delivery, Delivery::None);
        let status = reminders.notify(&session, &owner(&session), 3).unwrap();
        assert!(!status.enabled);
        assert!(notifier.shown().is_empty());
        let store = FileReminders::new(Some(tempfile::tempdir().unwrap().path().join("none.json")));
        assert!(!store.enabled(), "a missing file means off");
    }

    #[test]
    fn an_enabled_unlocked_session_shows_one_count_only_notification() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(true);
        let status = reminders.notify(&session, &owner(&session), 2).unwrap();
        assert_eq!(
            notifier.shown(),
            vec![("AKM".to_string(), "2 API keys need attention".to_string())]
        );
        assert_eq!(status.delivery, Delivery::Sent);
        assert!(!status.blocked);
    }

    #[test]
    fn a_locked_session_shows_nothing() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(true);
        let id = owner(&session);
        session.lock().unwrap();
        let error = reminders.notify(&session, &id, 2).unwrap_err();
        assert_eq!(error.kind, "locked");
        assert!(notifier.shown().is_empty());
        let never_opened = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        );
        assert_eq!(
            reminders
                .notify(&never_opened, &owner_of_nothing(), 2)
                .unwrap_err()
                .kind,
            "locked"
        );
        assert!(notifier.shown().is_empty());
    }

    #[test]
    fn review1_7_a_request_made_for_an_older_session_shows_nothing() {
        let (session, directory) = unlocked();
        let (reminders, notifier) = setup(true);
        let old = owner(&session);
        session.lock().unwrap();
        open_vault(&session, directory.path());
        let current = owner(&session);
        assert_ne!(old, current);
        assert_eq!(
            reminders.notify(&session, &old, 2).unwrap_err().kind,
            "locked"
        );
        assert_eq!(
            reminders
                .notify(&session, "not-a-session", 2)
                .unwrap_err()
                .kind,
            "locked"
        );
        assert!(notifier.shown().is_empty());
        reminders.notify(&session, &current, 2).unwrap();
        assert_eq!(notifier.shown().len(), 1);
    }

    #[test]
    fn review1_7_lock_and_a_new_opening_wait_for_a_notification_that_passed_validation() {
        use std::sync::mpsc;
        for newer in ["lock", "open"] {
            let (session, _directory) = unlocked();
            let (reminders, notifier) = setup(true);
            let reminders = Arc::new(reminders);
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            *notifier.hold.lock().unwrap() = Some((entered_tx, release_rx));
            let id = owner(&session);
            let sender = {
                let (session, reminders, id) = (session.clone(), reminders.clone(), id.clone());
                std::thread::spawn(move || reminders.notify(&session, &id, 2))
            };
            entered_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
            let (done_tx, done_rx) = mpsc::channel();
            let retire = {
                let session = session.clone();
                std::thread::spawn(move || {
                    match newer {
                        "lock" => session.lock().unwrap(),
                        _ => drop(session.prepare_open().unwrap()),
                    }
                    done_tx.send(()).unwrap();
                })
            };
            assert!(
                done_rx
                    .recv_timeout(std::time::Duration::from_millis(300))
                    .is_err(),
                "{newer}: the session was retired while the notifier was being called"
            );
            release_tx.send(()).unwrap();
            sender.join().unwrap().unwrap();
            retire.join().unwrap();
            assert_eq!(notifier.shown().len(), 1, "{newer}");
            assert_eq!(
                reminders.notify(&session, &id, 3).unwrap_err().kind,
                "locked",
                "{newer}: a request after the retirement shows nothing"
            );
            assert_eq!(notifier.shown().len(), 1, "{newer}");
        }
    }

    #[test]
    fn a_count_of_zero_shows_nothing() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(true);
        assert_eq!(
            reminders
                .notify(&session, &owner(&session), 0)
                .unwrap_err()
                .kind,
            "invalid"
        );
        assert!(notifier.shown().is_empty());
    }

    #[test]
    fn a_denied_permission_is_a_state_and_akm_does_not_try_again() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(true);
        notifier.set_permission(Permission::Denied);
        let first = reminders.notify(&session, &owner(&session), 2).unwrap();
        assert_eq!(first.permission, Permission::Denied);
        assert!(first.blocked);
        assert_eq!(first.delivery, Delivery::None);
        for _ in 0..5 {
            reminders.notify(&session, &owner(&session), 3).unwrap();
        }
        assert!(notifier.shown().is_empty());
        notifier.set_permission(Permission::Granted);
        assert!(
            reminders
                .notify(&session, &owner(&session), 2)
                .unwrap()
                .blocked,
            "a block holds until the setting is turned off and on"
        );
        assert!(notifier.shown().is_empty());
        reminders.set_enabled(false).unwrap();
        reminders.set_enabled(true).unwrap();
        reminders.notify(&session, &owner(&session), 2).unwrap();
        assert_eq!(notifier.shown().len(), 1);
    }

    #[test]
    fn a_failed_notification_is_a_state_and_akm_does_not_try_again() {
        let (session, _directory) = unlocked();
        let (reminders, notifier) = setup(true);
        notifier.refuse(true);
        let failed = reminders.notify(&session, &owner(&session), 2).unwrap();
        assert_eq!(failed.delivery, Delivery::Failed);
        assert!(failed.blocked);
        notifier.refuse(false);
        for _ in 0..3 {
            assert_eq!(
                reminders
                    .notify(&session, &owner(&session), 2)
                    .unwrap()
                    .delivery,
                Delivery::Failed
            );
        }
        assert!(notifier.shown().is_empty());
        reminders.set_enabled(false).unwrap();
        let again = reminders.set_enabled(true).unwrap();
        assert_eq!(again.delivery, Delivery::None);
        assert!(!again.blocked);
        reminders.notify(&session, &owner(&session), 2).unwrap();
        assert_eq!(notifier.shown().len(), 1);
    }

    #[test]
    fn the_choice_is_saved_in_its_own_file_and_survives_a_restart() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("config").join("reminders.json");
        let store = FileReminders::new(Some(file.clone()));
        assert!(!store.enabled());
        store.set_enabled(true).unwrap();
        assert!(FileReminders::new(Some(file.clone())).enabled());
        assert_eq!(
            fs::read_to_string(&file)
                .unwrap()
                .split_whitespace()
                .collect::<String>(),
            "{\"enabled\":true}"
        );
        store.set_enabled(false).unwrap();
        assert!(!FileReminders::new(Some(file.clone())).enabled());
        fs::write(&file, "not json").unwrap();
        assert!(
            !FileReminders::new(Some(file)).enabled(),
            "an unreadable file means off"
        );
    }

    #[test]
    fn a_choice_that_cannot_be_saved_is_an_error_and_the_old_choice_stays() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-folder");
        fs::write(&blocker, b"fake").unwrap();
        let store = Arc::new(FileReminders::new(Some(blocker.join("reminders.json"))));
        let notifier = Arc::new(FakeNotifier::default());
        let reminders = Reminders::new(store.clone(), notifier);
        let error = reminders.set_enabled(true).unwrap_err();
        assert_eq!(error.kind, "io");
        assert!(!reminders.status().unwrap().enabled);
        let none = Reminders::new(
            Arc::new(FileReminders::new(None)),
            Arc::new(FakeNotifier::default()),
        );
        assert_eq!(none.set_enabled(true).unwrap_err().kind, "config");
    }

    #[test]
    fn the_webview_has_no_notification_permission_and_no_background_entry_exists() {
        let capabilities = include_str!("../capabilities/default.json");
        assert!(
            !capabilities.contains("notification"),
            "only Rust sends notifications; the page gets no notification permission"
        );
        let config = include_str!("../tauri.conf.json");
        for startup in ["autostart", "startup", "scheduledTask", "service"] {
            assert!(
                !config.contains(startup),
                "{startup} must not appear in tauri.conf.json"
            );
        }
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("autostart"));
    }
}
