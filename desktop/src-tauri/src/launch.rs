use std::{
    cell::Cell,
    ffi::{OsStr, OsString},
    ops::Deref,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use akm_core::{model::reserved_class, EnvDisclosure, Vault};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use zeroize::Zeroize;

use crate::{
    error::CommandError,
    import_review::MAX_REVIEWED_CHANGES,
    session::{Access, Held, Session, Stamped},
    views::clean,
};

/// The variable values of one terminal launch. They are zeroed when the launch ends.
pub struct LaunchSecrets(Vec<(String, String)>);

impl Deref for LaunchSecrets {
    type Target = [(String, String)];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for LaunchSecrets {
    fn drop(&mut self) {
        for (_, value) in &mut self.0 {
            value.zeroize();
        }
    }
}

/// All that a terminal launch can hold. The page supplies none of it except the scope that
/// selected the variables: the folder is the user's pick in a native dialog.
pub struct LaunchRequest {
    pub folder: PathBuf,
    pub secrets: LaunchSecrets,
}

/// What the native confirmation window shows: the scope, the folder and the variable names.
pub struct LaunchReview {
    project: String,
    environment: String,
    folder: PathBuf,
    names: Vec<String>,
}

impl LaunchReview {
    pub fn new(project: String, environment: String, folder: PathBuf, names: Vec<String>) -> Self {
        Self {
            project,
            environment,
            folder,
            names,
        }
    }

    pub fn title(&self) -> &'static str {
        "Open a terminal with these keys"
    }

    /// Names and counts only. A secret value never reaches this text.
    pub fn message(&self) -> String {
        let count = self.names.len();
        let mut text = format!(
            "Open a terminal with the keys of project \"{}\", environment \"{}\"?\n\nFolder: {}\n\nThe terminal will hold {count} variable{}:\n",
            self.project,
            self.environment,
            self.folder.display(),
            if count == 1 { "" } else { "s" },
        );
        for name in &self.names {
            text.push_str(&format!("  {name}\n"));
        }
        text.push_str(
            "\nThese keys stay in that terminal window, and in every program that you start from it, until you close the window. Locking AKM does not remove them.\nSecret values are not shown.",
        );
        text
    }
}

/// The native side of a terminal launch. The page names a scope and nothing else: native code
/// owns the folder picker, the confirmation window and the process.
pub trait TerminalHost: Send + Sync {
    /// Whether the main window is shown and not minimized right now. Native asks it only
    /// before any dialog opens, so that no dialog appears over a window nobody can see.
    fn window_live(&self) -> bool;
    /// The user's choice of folder, or `None` when they cancel.
    fn choose_folder(&self) -> Result<Option<PathBuf>, CommandError>;
    /// The only authority to start the terminal. It answers on the user's own click in a
    /// window that the webview cannot script.
    fn confirm(&self, review: &LaunchReview) -> bool;
    /// Starts the terminal. It runs while the session gate is held for the session that was
    /// reviewed, so a lock cannot slip in between the last check and the start.
    fn open_terminal(&self, held: &Held<'_>, request: &LaunchRequest) -> Result<(), CommandError>;
}

pub struct TerminalState(pub Arc<dyn TerminalHost>);

pub struct NativeTerminal<R: tauri::Runtime>(pub AppHandle<R>);

impl<R: tauri::Runtime> TerminalHost for NativeTerminal<R> {
    fn window_live(&self) -> bool {
        self.0.get_webview_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        })
    }

    fn choose_folder(&self) -> Result<Option<PathBuf>, CommandError> {
        let mut dialog = self
            .0
            .dialog()
            .file()
            .set_title("Choose the folder for the terminal");
        if let Some(window) = self.0.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        dialog
            .blocking_pick_folder()
            .map(|folder| {
                folder
                    .into_path()
                    .map_err(|error| CommandError::new("invalidPath", error.to_string()))
            })
            .transpose()
    }

    fn confirm(&self, review: &LaunchReview) -> bool {
        let mut dialog = self
            .0
            .dialog()
            .message(review.message())
            .title(review.title())
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Open terminal".into(),
                "Cancel".into(),
            ));
        if let Some(window) = self.0.get_webview_window("main") {
            dialog = dialog.parent(&window);
        }
        dialog.blocking_show()
    }

    fn open_terminal(&self, _held: &Held<'_>, request: &LaunchRequest) -> Result<(), CommandError> {
        let shell = system_command_prompt()?;
        let mut command = prepare_command(
            &shell,
            &request.folder,
            std::env::vars_os().map(|(name, _)| name),
            &request.secrets,
        );
        // AKM does not wait for the terminal and does not track it.
        let child = command.spawn().map_err(|error| {
            CommandError::new(
                "launch",
                format!("The terminal could not be started. ({error})"),
            )
        })?;
        drop(command);
        drop(child);
        Ok(())
    }
}

/// The command prompt of the system, found through the Windows system directory and never
/// through PATH or an environment variable.
#[cfg(windows)]
pub fn system_command_prompt() -> Result<PathBuf, CommandError> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = [0u16; 512];
    // SAFETY: the buffer is writable for the length that is passed.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(CommandError::new(
            "launch",
            "Windows did not give the system folder, so no terminal was opened.",
        ));
    }
    Ok(PathBuf::from(OsString::from_wide(&buffer[..length])).join("cmd.exe"))
}

#[cfg(not(windows))]
pub fn system_command_prompt() -> Result<PathBuf, CommandError> {
    Err(CommandError::new(
        "launch",
        "A terminal can be opened on Windows only.",
    ))
}

/// A new console window, for the child to own.
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// The command that opens a terminal in a new console window. It has no argument. The child
/// inherits this process's environment without any `AKM_*` variable (the rule of
/// `crates/akm-cli/src/child.rs`), plus the selected variables.
pub fn prepare_command(
    shell: &Path,
    folder: &Path,
    inherited: impl IntoIterator<Item = OsString>,
    secrets: &[(String, String)],
) -> Command {
    let mut command = Command::new(shell);
    command.current_dir(folder);
    for (name, value) in secrets {
        if !is_akm_name(OsStr::new(name)) {
            command.env(name, value);
        }
    }
    for name in inherited {
        if is_akm_name(&name) {
            command.env_remove(name);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    command
}

fn is_akm_name(name: &OsStr) -> bool {
    name.to_string_lossy()
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("AKM_"))
}

/// The names of a scope, checked before any dialog opens or anything is journaled. The core
/// refuses a reserved name when it plans; this repeats the check of the command line
/// (`child::validate_names`) so that the two cannot drift apart.
fn check_names(names: &[String]) -> Result<(), CommandError> {
    if names.is_empty() {
        return Err(CommandError::new(
            "invalid",
            "No keys match that project and environment.",
        ));
    }
    for name in names {
        if let Some(class) = reserved_class(name) {
            return Err(CommandError::new(
                "invalid",
                format!(
                    "a selected key uses the reserved variable {name:?}: {}",
                    class.reason()
                ),
            ));
        }
    }
    if names.len() > MAX_REVIEWED_CHANGES {
        return Err(CommandError::new(
            "tooLarge",
            format!(
                "This scope has more than {MAX_REVIEWED_CHANGES} keys. A terminal opens for at most {MAX_REVIEWED_CHANGES} keys, so that every name can be reviewed."
            ),
        ));
    }
    Ok(())
}

fn planned_names(
    vault: &Vault,
    project: &str,
    environment: &str,
) -> Result<Vec<String>, CommandError> {
    let plan = vault.plan_env(Some(project), Some(environment), false)?;
    let names: Vec<String> = plan.names().map(str::to_owned).collect();
    check_names(&names)?;
    Ok(names)
}

/// A console cannot start in a network folder: it falls back to the Windows folder, and the
/// review would then show a folder that the terminal is not in.
fn checked_folder(folder: PathBuf) -> Result<PathBuf, CommandError> {
    let text = folder.to_string_lossy();
    let network =
        (text.starts_with(r"\\") && !text.starts_with(r"\\?\")) || text.starts_with(r"\\?\UNC\");
    if network {
        return Err(CommandError::new(
            "invalidPath",
            "A terminal cannot start in a network folder (\\\\server\\share). Choose a folder on a local drive.",
        ));
    }
    if !folder.is_dir() {
        return Err(CommandError::new(
            "invalidPath",
            "That folder does not exist.",
        ));
    }
    Ok(folder)
}

/// The whole launch in one call. The webview names the session and the scope. A native
/// folder picker and a native window decide the rest. `None` means that the user cancelled.
///
/// The order is fixed. The scope is checked, then the folder is picked, then the user
/// reviews the names. The session is checked again under its gate after the review. The
/// launch is journaled before any value is read, and a journal failure ends it there. The
/// terminal starts under the gate, so a lock cannot come between the last check and the start.
pub fn run_launch(
    session: &Session,
    host: &dyn TerminalHost,
    session_id: &str,
    project: Option<String>,
    environment: Option<String>,
) -> Result<Option<usize>, CommandError> {
    let expected = session.snapshot_for(session_id)?;
    let (Some(project), Some(environment)) = (clean(project), clean(environment)) else {
        return Err(CommandError::new(
            "invalid",
            "Choose both a project and an environment before opening a terminal.",
        ));
    };
    let planned = session.access_reviewed(Access::Read, expected, |vault| {
        planned_names(vault, &project, &environment)
    })?;
    let (names, reviewed) = (planned.value, planned.snapshot);

    if !host.window_live() {
        return Err(CommandError::new(
            "viewChanged",
            "The AKM window is not shown, so no window was opened. Nothing was started. Start it again.",
        ));
    }
    let picked = host.choose_folder()?;
    session.ensure(reviewed.ticket)?;
    let Some(folder) = picked else {
        return Ok(None);
    };
    let folder = checked_folder(folder)?;

    let review = LaunchReview::new(project.clone(), environment.clone(), folder.clone(), names);
    if !host.confirm(&review) {
        return Ok(None);
    }

    // A modal dialog cannot be shown under the session gate, so the access below binds the
    // request to the session and the revision again. A lock, another session or any write
    // since the plan was read refuses it, so the plan here is the one the user was shown.
    let started = Cell::new(false);
    let done = session.access_reviewed(Access::Read, reviewed, |vault| {
        let plan = vault.plan_env(Some(&project), Some(&environment), false)?;
        let proof = vault.disclose_env(
            EnvDisclosure::Run,
            &plan,
            Some(&format!("launch requested; {} keys selected", plan.len())),
        )?;
        let request = LaunchRequest {
            folder,
            secrets: LaunchSecrets(vault.env_vars(&plan, &proof)?),
        };
        session.while_current(reviewed.ticket, |held| host.open_terminal(held, &request))??;
        started.set(true);
        Ok(request.secrets.len())
    });
    settle(done, started.get()).map(Some)
}

/// A terminal that started is not undone by a lock that arrives a moment later. The reply
/// then says that it completed, so the page does not offer a second launch.
fn settle(
    result: Result<Stamped<usize>, CommandError>,
    started: bool,
) -> Result<usize, CommandError> {
    match result {
        Ok(done) => Ok(done.value),
        Err(error) if started && error.kind == "locked" => {
            Err(CommandError::completed_while_locked())
        }
        Err(error) => Err(error),
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
    use akm_core::{
        model::{META_ENVIRONMENT, META_ENV_VAR, META_PROJECT},
        Vault,
    };
    use keepass::db::fields;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };

    const PASSWORD: &str = "fake-test-password-123!";

    /// One entry of a fake vault: title, variable name, project, environment, secret.
    type Entry = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    );

    struct Fixture {
        session: Arc<Session>,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn vault_path(&self) -> PathBuf {
            self.directory.path().join("vault.kdbx")
        }

        fn journal_actions(&self) -> Vec<String> {
            crate::test_support::journal_of(&self.vault_path())
                .read(100)
                .map(|entries| entries.into_iter().map(|entry| entry.action).collect())
                .unwrap_or_default()
        }

        fn session_id(&self) -> String {
            let ticket = self.session.snapshot().unwrap().ticket;
            self.session.active_owner(ticket).unwrap()
        }
    }

    /// Entries are written to the file, so a name that `add` would refuse can be tested.
    fn fake_vault(entries: &[Entry]) -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        let mut db = keepass::Database::new();
        db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
        {
            let mut root = db.root_mut();
            let mut group = root.add_group();
            group.name = "API Keys".into();
            for (title, variable, project, environment, secret) in entries {
                let mut entry = group.add_entry();
                entry.set_unprotected(fields::TITLE, *title);
                entry.set_protected(fields::PASSWORD, *secret);
                entry.set_unprotected(META_ENV_VAR, *variable);
                entry.set_unprotected(META_PROJECT, *project);
                entry.set_unprotected(META_ENVIRONMENT, *environment);
            }
        }
        db.save(
            &mut std::fs::File::create(&path).unwrap(),
            keepass::DatabaseKey::new().with_password(PASSWORD),
        )
        .unwrap();
        let session = Arc::new(Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        ));
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || Vault::open(&path, PASSWORD).map_err(Into::into))
            .unwrap();
        Fixture { session, directory }
    }

    const DEV: [Entry; 2] = [
        (
            "OpenAI",
            "OPENAI_API_KEY",
            "app",
            "dev",
            "fake-openai-value",
        ),
        ("Token", "TOKEN", "app", "dev", "fake-token-value"),
    ];

    #[derive(Clone, Debug, PartialEq)]
    struct Launched {
        folder: PathBuf,
        envs: Vec<(String, String)>,
    }

    /// The native side, scripted. It never opens a dialog, a window or a process.
    struct FakeHost {
        live: AtomicBool,
        folder: Mutex<Option<PathBuf>>,
        approve: AtomicBool,
        events: Mutex<Vec<&'static str>>,
        reviews: Mutex<Vec<String>>,
        launched: Mutex<Vec<Launched>>,
        while_choosing: Box<dyn Fn() + Send + Sync>,
        while_confirming: Box<dyn Fn() + Send + Sync>,
        while_launching: Box<dyn Fn() + Send + Sync>,
    }

    impl FakeHost {
        fn choosing(folder: &Path) -> Self {
            Self {
                live: AtomicBool::new(true),
                folder: Mutex::new(Some(folder.to_path_buf())),
                approve: AtomicBool::new(true),
                events: Mutex::new(Vec::new()),
                reviews: Mutex::new(Vec::new()),
                launched: Mutex::new(Vec::new()),
                while_choosing: Box::new(|| {}),
                while_confirming: Box::new(|| {}),
                while_launching: Box::new(|| {}),
            }
        }

        fn events(&self) -> Vec<&'static str> {
            self.events.lock().unwrap().clone()
        }

        fn launched(&self) -> Vec<Launched> {
            self.launched.lock().unwrap().clone()
        }
    }

    impl TerminalHost for FakeHost {
        fn window_live(&self) -> bool {
            self.events.lock().unwrap().push("window");
            self.live.load(Ordering::SeqCst)
        }

        fn choose_folder(&self) -> Result<Option<PathBuf>, CommandError> {
            self.events.lock().unwrap().push("folder");
            (self.while_choosing)();
            Ok(self.folder.lock().unwrap().clone())
        }

        fn confirm(&self, review: &LaunchReview) -> bool {
            self.events.lock().unwrap().push("confirm");
            self.reviews.lock().unwrap().push(review.message());
            (self.while_confirming)();
            self.approve.load(Ordering::SeqCst)
        }

        fn open_terminal(
            &self,
            _held: &Held<'_>,
            request: &LaunchRequest,
        ) -> Result<(), CommandError> {
            self.events.lock().unwrap().push("launch");
            (self.while_launching)();
            self.launched.lock().unwrap().push(Launched {
                folder: request.folder.clone(),
                envs: request.secrets.to_vec(),
            });
            Ok(())
        }
    }

    fn launch(fixture: &Fixture, host: &FakeHost) -> Result<Option<usize>, CommandError> {
        run_launch(
            &fixture.session,
            host,
            &fixture.session_id(),
            Some("app".into()),
            Some("dev".into()),
        )
    }

    fn assert_nothing_happened(fixture: &Fixture, host: &FakeHost, expected_events: &[&str]) {
        assert_eq!(host.events(), expected_events);
        assert!(host.launched().is_empty(), "the launcher was called");
        assert!(
            !fixture
                .journal_actions()
                .iter()
                .any(|action| action == "run-requested"),
            "a launch was recorded"
        );
    }

    #[test]
    fn a_launch_hands_the_selected_variables_and_the_chosen_folder_to_the_launcher() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        assert_eq!(launch(&fixture, &host).unwrap(), Some(2));
        assert_eq!(host.events(), ["window", "folder", "confirm", "launch"]);
        assert_eq!(
            host.launched(),
            vec![Launched {
                folder: fixture.directory.path().to_path_buf(),
                envs: vec![
                    ("OPENAI_API_KEY".into(), "fake-openai-value".into()),
                    ("TOKEN".into(), "fake-token-value".into()),
                ],
            }]
        );
        let journal = crate::test_support::journal_of(&fixture.vault_path())
            .read(100)
            .unwrap();
        let requested: Vec<_> = journal
            .iter()
            .filter(|entry| entry.action == "run-requested")
            .collect();
        assert_eq!(requested.len(), 1);
        let detail = requested[0].detail.clone().unwrap_or_default();
        assert_eq!(detail, "launch requested; 2 keys selected");
        assert!(!detail.contains("fake-"));
    }

    #[test]
    fn a_variable_outside_the_scope_is_not_launched() {
        let fixture = fake_vault(&[
            DEV[0],
            ("Prod", "PROD_KEY", "app", "prod", "fake-prod-value"),
            ("Other", "OTHER_KEY", "other", "dev", "fake-other-value"),
        ]);
        let host = FakeHost::choosing(fixture.directory.path());
        assert_eq!(launch(&fixture, &host).unwrap(), Some(1));
        let launched = host.launched();
        assert_eq!(launched[0].envs.len(), 1);
        assert_eq!(launched[0].envs[0].0, "OPENAI_API_KEY");
    }

    #[test]
    fn case_1_a_journal_failure_before_the_disclosure_starts_nothing_and_reads_no_value() {
        let fixture = fake_vault(&DEV);
        let journal = crate::test_support::journal_of(&fixture.vault_path());
        std::fs::remove_file(journal.path()).ok();
        std::fs::create_dir(journal.path()).unwrap();
        let host = FakeHost::choosing(fixture.directory.path());
        let error = launch(&fixture, &host).expect_err("the launch failed closed");
        assert_eq!(error.kind, "journal");
        assert!(!error.message.contains("fake-"), "{}", error.message);
        assert!(host.launched().is_empty());
        assert_eq!(host.events(), ["window", "folder", "confirm"]);
    }

    #[test]
    fn case_2_a_lock_or_a_change_while_the_window_is_open_refuses_the_launch() {
        for change in ["lock", "write", "other session"] {
            let fixture = fake_vault(&DEV);
            let mut host = FakeHost::choosing(fixture.directory.path());
            let session = fixture.session.clone();
            let path = fixture.vault_path();
            host.while_confirming = match change {
                "lock" => Box::new(move || session.lock().unwrap()),
                "write" => Box::new(move || {
                    session
                        .access(Access::Write, None, |vault| {
                            vault
                                .add(akm_core::ApiKeyInput::new("Added", "fake-added"))
                                .map_err(Into::into)
                        })
                        .unwrap();
                }),
                _ => Box::new(move || {
                    let ticket = session.begin_open().unwrap();
                    session
                        .open(ticket, || Vault::open(&path, PASSWORD).map_err(Into::into))
                        .unwrap();
                }),
            };
            let error = launch(&fixture, &host).expect_err(change);
            assert!(
                ["locked", "stalePreview"].contains(&error.kind.as_str()),
                "{change}: {}",
                error.kind
            );
            assert_eq!(host.events(), ["window", "folder", "confirm"], "{change}");
            assert!(host.launched().is_empty(), "{change}");
            assert!(
                !fixture
                    .journal_actions()
                    .iter()
                    .any(|action| action == "run-requested"),
                "{change}"
            );
        }
    }

    #[test]
    fn a_lock_while_the_folder_picker_is_open_shows_no_review() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let session = fixture.session.clone();
        host.while_choosing = Box::new(move || session.lock().unwrap());
        assert_eq!(launch(&fixture, &host).err().unwrap().kind, "locked");
        assert_nothing_happened(&fixture, &host, &["window", "folder"]);
    }

    #[test]
    fn a_list_of_names_is_checked_for_the_same_faults_as_the_command_line_checks() {
        let owned = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert!(check_names(&owned(&["TOKEN", "OPENAI_API_KEY"])).is_ok());
        assert_eq!(check_names(&[]).unwrap_err().kind, "invalid");
        for reserved in ["PATH", "path", "AKM_ID", "NODE_OPTIONS", "HTTP_PROXY"] {
            let error = check_names(&owned(&["TOKEN", reserved])).unwrap_err();
            assert!(
                error.message.contains("reserved variable"),
                "{reserved}: {}",
                error.message
            );
        }
    }

    #[test]
    fn case_2b_the_start_runs_under_the_session_gate_so_a_lock_waits_for_it() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let finished = Arc::new(AtomicBool::new(false));
        let waiting = Arc::new(Mutex::new(None));
        let (session, flag, slot) = (fixture.session.clone(), finished.clone(), waiting.clone());
        let observed_during_start = Arc::new(AtomicBool::new(true));
        let observed = observed_during_start.clone();
        host.while_launching = Box::new(move || {
            let (session, flag) = (session.clone(), flag.clone());
            *slot.lock().unwrap() = Some(std::thread::spawn(move || {
                session.lock().unwrap();
                flag.store(true, Ordering::SeqCst);
            }));
            std::thread::sleep(std::time::Duration::from_millis(200));
            observed.store(finished.load(Ordering::SeqCst), Ordering::SeqCst);
        });
        let result = launch(&fixture, &host);
        waiting.lock().unwrap().take().unwrap().join().unwrap();
        assert!(
            !observed_during_start.load(Ordering::SeqCst),
            "a lock finished while the terminal was starting"
        );
        assert_eq!(host.launched().len(), 1);
        // The lock came after the start. The reply must not claim that nothing happened.
        match result {
            Ok(Some(2)) => {}
            Err(error) => assert_eq!(error.kind, "completedWhileLocked"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_start_that_a_lock_overtook_is_reported_as_completed_not_as_locked() {
        let overtaken = |launched| {
            settle(Err::<Stamped<usize>, _>(CommandError::locked()), launched)
                .unwrap_err()
                .kind
        };
        assert_eq!(overtaken(true), "completedWhileLocked");
        assert_eq!(overtaken(false), "locked");
    }

    #[test]
    fn case_3_declining_in_the_review_records_nothing_and_starts_nothing() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        host.approve.store(false, Ordering::SeqCst);
        assert_eq!(launch(&fixture, &host).unwrap(), None);
        assert_nothing_happened(&fixture, &host, &["window", "folder", "confirm"]);
    }

    #[test]
    fn case_4_cancelling_the_folder_picker_shows_no_review_and_records_nothing() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        *host.folder.lock().unwrap() = None;
        assert_eq!(launch(&fixture, &host).unwrap(), None);
        assert_nothing_happened(&fixture, &host, &["window", "folder"]);
    }

    #[test]
    fn case_5_a_reserved_variable_name_is_refused_before_any_dialog() {
        for reserved in ["PATH", "PYTHONSTARTUP", "HTTPS_PROXY", "AKM_PASSWORD"] {
            let fixture =
                fake_vault(&[DEV[0], ("Process control", reserved, "app", "dev", "fake")]);
            let host = FakeHost::choosing(fixture.directory.path());
            let error = launch(&fixture, &host).expect_err(reserved);
            assert!(error.message.contains("reserved"), "{}", error.message);
            assert!(!error.message.contains("fake-"), "{}", error.message);
            assert_nothing_happened(&fixture, &host, &[]);
        }
    }

    #[test]
    fn case_6_an_empty_scope_and_a_scope_too_big_to_list_are_refused_before_any_dialog() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        let error = run_launch(
            &fixture.session,
            &host,
            &fixture.session_id(),
            Some("app".into()),
            Some("prod".into()),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, "invalid");
        assert_nothing_happened(&fixture, &host, &[]);

        let names: Vec<String> = (0..=MAX_REVIEWED_CHANGES)
            .map(|n| format!("KEY_{n}"))
            .collect();
        let entries: Vec<Entry> = names
            .iter()
            .map(|name| {
                let name: &'static str = Box::leak(name.clone().into_boxed_str());
                (name, name, "app", "dev", "fake-value")
            })
            .collect();
        let big = fake_vault(&entries);
        let host = FakeHost::choosing(big.directory.path());
        let error = launch(&big, &host).err().unwrap();
        assert_eq!(error.kind, "tooLarge");
        assert!(error.message.contains("25"), "{}", error.message);
        assert_nothing_happened(&big, &host, &[]);

        let exactly: Vec<Entry> = entries[..MAX_REVIEWED_CHANGES].to_vec();
        let limit = fake_vault(&exactly);
        let host = FakeHost::choosing(limit.directory.path());
        assert_eq!(launch(&limit, &host).unwrap(), Some(MAX_REVIEWED_CHANGES));
    }

    #[test]
    fn a_scope_needs_both_a_project_and_an_environment() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        for (project, environment) in [
            (None, Some("dev")),
            (Some("app"), None),
            (Some(" "), Some("dev")),
        ] {
            let error = run_launch(
                &fixture.session,
                &host,
                &fixture.session_id(),
                project.map(Into::into),
                environment.map(Into::into),
            )
            .err()
            .unwrap();
            assert_eq!(error.kind, "invalid");
        }
        assert_nothing_happened(&fixture, &host, &[]);
    }

    #[test]
    fn a_window_that_is_not_shown_gets_no_dialog() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        host.live.store(false, Ordering::SeqCst);
        let error = launch(&fixture, &host).err().unwrap();
        assert_eq!(error.kind, "viewChanged");
        assert_nothing_happened(&fixture, &host, &["window"]);
    }

    #[test]
    fn case_8_a_stale_or_malformed_session_is_refused_without_a_dialog() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        for id in [
            uuid::Uuid::new_v4().to_string(),
            "not-an-id".to_string(),
            String::new(),
        ] {
            let error = run_launch(
                &fixture.session,
                &host,
                &id,
                Some("app".into()),
                Some("dev".into()),
            )
            .err()
            .unwrap();
            assert_eq!(error.kind, "locked", "{id:?}");
        }
        let current = fixture.session_id();
        fixture.session.lock().unwrap();
        let error = run_launch(
            &fixture.session,
            &host,
            &current,
            Some("app".into()),
            Some("dev".into()),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, "locked");
        assert_nothing_happened(&fixture, &host, &[]);
    }

    #[test]
    fn a_folder_that_is_not_a_local_directory_is_refused_after_the_pick_and_before_the_review() {
        let fixture = fake_vault(&DEV);
        for folder in [
            fixture.directory.path().join("missing"),
            PathBuf::from(r"\\server\share\work"),
        ] {
            let host = FakeHost::choosing(&folder);
            let error = launch(&fixture, &host).err().unwrap();
            assert_eq!(error.kind, "invalidPath", "{folder:?}");
            assert_nothing_happened(&fixture, &host, &["window", "folder"]);
        }
    }

    #[test]
    fn case_9_the_review_lists_the_scope_the_folder_and_each_name_but_no_value() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        launch(&fixture, &host).unwrap();
        let shown = host.reviews.lock().unwrap().clone();
        assert_eq!(shown.len(), 1);
        let folder = fixture.directory.path().display().to_string();
        assert_eq!(
            shown[0],
            format!(
                "Open a terminal with the keys of project \"app\", environment \"dev\"?\n\
                 \n\
                 Folder: {folder}\n\
                 \n\
                 The terminal will hold 2 variables:\n  OPENAI_API_KEY\n  TOKEN\n\
                 \n\
                 These keys stay in that terminal window, and in every program that you start from it, until you close the window. Locking AKM does not remove them.\n\
                 Secret values are not shown."
            )
        );
        assert!(!shown[0].contains("fake-"));
    }

    #[test]
    fn the_review_names_one_variable_in_the_singular() {
        let review = LaunchReview::new(
            "app".into(),
            "dev".into(),
            PathBuf::from("C:/work"),
            vec!["ONLY".into()],
        );
        assert!(review.message().contains("hold 1 variable:\n  ONLY\n"));
    }

    fn names<'a>(command: &'a Command, wanted: &'a str) -> Vec<(String, Option<String>)> {
        command
            .get_envs()
            .filter(|(name, _)| {
                name.to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with(wanted)
            })
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn case_7_the_child_environment_holds_the_selected_variables_and_no_akm_variable() {
        let command = prepare_command(
            Path::new("C:/shell/cmd.exe"),
            Path::new("C:/work"),
            ["PATH", "AKM_PASSWORD", "akm_journal_root", "Akm_Other"].map(OsString::from),
            &[
                ("OPENAI_API_KEY".into(), "fake-openai-value".into()),
                ("TOKEN".into(), "fake-token-value".into()),
            ],
        );
        assert_eq!(command.get_program(), "C:/shell/cmd.exe");
        assert_eq!(command.get_current_dir(), Some(Path::new("C:/work")));
        assert_eq!(
            command.get_args().count(),
            0,
            "the page cannot add an argument"
        );
        assert_eq!(
            names(&command, "OPENAI_API_KEY"),
            [(
                "OPENAI_API_KEY".to_string(),
                Some("fake-openai-value".to_string())
            )]
        );
        assert_eq!(
            names(&command, "TOKEN"),
            [("TOKEN".to_string(), Some("fake-token-value".to_string()))]
        );
        let akm = names(&command, "AKM_");
        assert_eq!(akm.len(), 3, "{akm:?}");
        assert!(akm.iter().all(|(_, value)| value.is_none()), "{akm:?}");
        assert!(
            names(&command, "PATH").is_empty(),
            "PATH stays inherited, untouched"
        );
    }

    #[test]
    fn a_selected_name_with_the_akm_prefix_never_reaches_the_child() {
        let command = prepare_command(
            Path::new("C:/shell/cmd.exe"),
            Path::new("C:/work"),
            Vec::<OsString>::new(),
            &[("AKM_PASSWORD".into(), "fake-leak".into())],
        );
        assert!(names(&command, "AKM_")
            .iter()
            .all(|(_, value)| value.is_none()));
    }

    #[test]
    fn the_akm_name_rule_mirrors_the_one_of_the_command_line() {
        for (name, removed) in [
            ("AKM_PASSWORD", true),
            ("akm_password", true),
            ("Akm_", true),
            ("AKM", false),
            ("AKMX", false),
            ("XAKM_Y", false),
            ("PATH", false),
            ("", false),
        ] {
            assert_eq!(is_akm_name(OsStr::new(name)), removed, "{name:?}");
        }
        let child = include_str!("../../../crates/akm-cli/src/child.rs");
        let rule = "name.to_string_lossy()\n        .get(..4)\n        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(\"AKM_\"))";
        assert!(
            child.contains(rule),
            "child.rs changed its AKM_ rule; change launch.rs too"
        );
        assert!(include_str!("launch.rs").contains(rule));
    }

    #[cfg(windows)]
    #[test]
    fn the_shell_is_the_system_command_prompt_and_does_not_come_from_the_environment() {
        let shell = system_command_prompt().unwrap();
        assert!(shell.is_absolute(), "{shell:?}");
        assert_eq!(shell.file_name().unwrap(), "cmd.exe");
        assert!(shell.is_file(), "{shell:?}");
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_default();
        assert!(shell.starts_with(windows), "{shell:?}");
    }

    #[cfg(windows)]
    #[test]
    fn files_that_akm_opens_are_not_inherited_by_a_child() {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("probe");
        std::fs::write(&path, b"x").unwrap();
        let opened = [
            std::fs::File::open(&path).unwrap(),
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap(),
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(directory.path().join("new"))
                .unwrap(),
        ];
        for file in &opened {
            let mut flags = 0u32;
            let ok = unsafe { GetHandleInformation(file.as_raw_handle() as _, &mut flags) };
            assert_ne!(ok, 0);
            assert_eq!(
                flags & HANDLE_FLAG_INHERIT,
                0,
                "an opened file is inheritable"
            );
        }
    }
}
