use std::{
    ffi::{OsStr, OsString},
    ops::Deref,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use akm_core::{model::reserved_class, EnvDisclosure, Vault};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

use crate::{
    error::CommandError,
    import_review::MAX_REVIEWED_CHANGES,
    review_window::{
        review_name, review_names, review_path, NativeReviewDialog, ReviewAnswer, ReviewDialog,
    },
    session::{Access, Held, Session},
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

    /// Scope, folder and warnings, with the count of names. A secret value never reaches this
    /// text. The names come after it, so a long list cannot push the warnings off the screen.
    pub fn body(&self) -> String {
        let count = self.names.len();
        format!(
            "Open a terminal with the keys of project \"{}\", environment \"{}\"?\n\nFolder: {}\n\nThese keys stay in that terminal window, and in every program that you start from it, until you close the window. Locking AKM does not remove them.\nThe terminal also gets the environment that AKM was started with, without AKM's own settings.\nA program in that folder can run when you type its name, so choose a folder that you trust.\nSecret values are not shown.\n\nThe terminal will hold {count} variable{}:",
            review_name(&self.project),
            review_name(&self.environment),
            review_path(&self.folder),
            if count == 1 { "" } else { "s" },
        )
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Shows the review and waits for the click. No gate is held. The caller checks the session
    /// again before anything that the answer allows.
    pub fn ask(&self, dialog: &dyn ReviewDialog) -> ReviewAnswer {
        review_names(
            dialog,
            self.title(),
            &self.body(),
            &self.names,
            "Open terminal",
            true,
        )
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
    /// The window in which the user approves the launch: a click that the webview cannot
    /// script.
    fn review_dialog(&self) -> Box<dyn ReviewDialog + '_>;
    /// Runs at each step of the state table in `docs/design/launch-with-keys.md`. Production
    /// does nothing here. A test places a barrier, so that a lock lands exactly at that step.
    fn reached(&self, _step: LaunchStep) {}
    /// Starts the terminal. It runs while the session gate is held for the session that was
    /// reviewed, so a lock cannot slip in between the last check and the start.
    fn open_terminal(&self, held: &Held<'_>, request: &LaunchRequest) -> Result<(), CommandError>;
}

/// The steps of a launch, in order: the rows of the state table. Each one is a point at which a
/// lock can land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchStep {
    /// The names are planned. Next: the window check and the picker.
    Planned,
    /// The picked folder passed its check. Next: the last check before the review.
    FolderChecked,
    /// The last check before the review passed. Next: the window.
    LastCheckPassed,
    /// The user approved. Next: the access that re-checks the session and the revision.
    Approved,
    /// The check before the journal record passed. Next: the record.
    RecordNext,
    /// The record is written. Next: the session check and the copy of the values.
    Recorded,
    /// The values are copied. Next: the end of the vault access.
    ValuesCopied,
    /// The vault is released. Next: the session check and the start, under the gate.
    BeforeStart,
    /// The terminal has started.
    Started,
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

    fn review_dialog(&self) -> Box<dyn ReviewDialog + '_> {
        Box::new(NativeReviewDialog(self.0.clone()))
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

/// The command that opens a terminal in a new console window. `/d` turns off the AutoRun
/// commands of the registry, which would otherwise run with the keys before the user types
/// anything. The child inherits this process's environment without any `AKM_*` variable (the
/// rule of `crates/akm-cli/src/child.rs`), plus the selected variables.
pub fn prepare_command(
    shell: &Path,
    folder: &Path,
    inherited: impl IntoIterator<Item = OsString>,
    secrets: &[(String, String)],
) -> Command {
    let mut command = Command::new(shell);
    command.arg("/d").current_dir(folder);
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

/// Variables that the command prompt itself reads. It prints `PROMPT` on every line, and it
/// prints the value of the other two in its error messages.
const READ_BY_THE_COMMAND_PROMPT: [&str; 3] = ["PROMPT", "DIRCMD", "COPYCMD"];

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
        if READ_BY_THE_COMMAND_PROMPT
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(name))
        {
            return Err(CommandError::new(
                "invalid",
                format!(
                    "a selected key uses the variable {name:?}. The command prompt can print that variable on screen, so no terminal was opened. Give the key another variable name."
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
/// The steps and what a lock does at each one are the state table in
/// `docs/design/launch-with-keys.md`, section 1.1. The gate is taken only to check the session
/// and copy what is needed. Blocking work runs outside it. The gate is taken again, and the
/// session is checked, before any effect that discloses or starts.
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
    let ticket = reviewed.ticket;

    host.reached(LaunchStep::Planned);
    if !host.window_live() {
        return Err(CommandError::new(
            "viewChanged",
            "The AKM window is not shown, so no window was opened. Nothing was started. Start it again.",
        ));
    }
    session.ensure(ticket)?;
    let picked = host.choose_folder()?;
    session.ensure(ticket)?;
    let Some(folder) = picked else {
        return Ok(None);
    };
    let folder = checked_folder(folder)?;
    host.reached(LaunchStep::FolderChecked);

    let review = LaunchReview::new(project.clone(), environment.clone(), folder.clone(), names);
    let dialog = host.review_dialog();
    // The last check before the window. It is not atomic with the window, and it cannot be: a
    // modal window cannot wait under the gate. A names-only review can appear just after a lock,
    // and it starts nothing, because the access below refuses (owner's option (a)).
    session.ensure(ticket)?;
    host.reached(LaunchStep::LastCheckPassed);
    match review.ask(dialog.as_ref()) {
        ReviewAnswer::Approved => {}
        ReviewAnswer::Declined => return Ok(None),
        ReviewAnswer::Failed(failure) => return Err(failure.into()),
    }
    host.reached(LaunchStep::Approved);

    // The access binds the request to the session and the revision again. A lock, another
    // session or any write since the plan was read refuses it, so the plan here is the one the
    // user was shown.
    let secrets = session
        .access_reviewed(Access::Read, reviewed, |vault| {
            disclose_for_launch(session, host, ticket, vault, &project, &environment)
        })?
        .value;
    let count = secrets.len();
    let request = LaunchRequest { folder, secrets };
    host.reached(LaunchStep::BeforeStart);
    session.while_current(ticket, |held| host.open_terminal(held, &request))??;
    host.reached(LaunchStep::Started);
    Ok(Some(count))
}

/// The record and the values, in one step under the session gate. A lock that came after the
/// access began stops both. The vault lock is already held, and vault then gate is the order
/// that every access takes.
fn disclose_for_launch(
    session: &Session,
    host: &dyn TerminalHost,
    ticket: crate::policy::Ticket,
    vault: &mut Vault,
    project: &str,
    environment: &str,
) -> Result<LaunchSecrets, CommandError> {
    let values = session.while_current(ticket, |_held| {
        host.reached(LaunchStep::RecordNext);
        let plan = vault.plan_env(Some(project), Some(environment), false)?;
        let proof = vault.disclose_env(
            EnvDisclosure::Run,
            &plan,
            Some(&format!("launch requested; {} keys selected", plan.len())),
        )?;
        host.reached(LaunchStep::Recorded);
        let values: Result<_, CommandError> = vault.env_vars(&plan, &proof).map_err(Into::into);
        values
    })??;
    host.reached(LaunchStep::ValuesCopied);
    Ok(LaunchSecrets(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clipboard::tests::{FakeClipboard, FakeData},
        policy::Signal,
        review_window::{review_text, ReviewFailure, ReviewText, CANCEL_BUTTON, CONFIRM_BUTTON},
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
        while_preparing: Box<dyn Fn() + Send + Sync>,
        before_window: Box<dyn Fn() + Send + Sync>,
        while_confirming: Box<dyn Fn() + Send + Sync>,
        window_result: Mutex<Option<ReviewFailure>>,
        at_step: Box<dyn Fn(LaunchStep) + Send + Sync>,
        steps: Mutex<Vec<LaunchStep>>,
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
                while_preparing: Box::new(|| {}),
                before_window: Box::new(|| {}),
                while_confirming: Box::new(|| {}),
                window_result: Mutex::new(None),
                at_step: Box::new(|_| {}),
                steps: Mutex::new(Vec::new()),
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

        fn review_dialog(&self) -> Box<dyn ReviewDialog + '_> {
            (self.while_preparing)();
            Box::new(self)
        }

        fn reached(&self, step: LaunchStep) {
            self.steps.lock().unwrap().push(step);
            (self.at_step)(step);
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

    impl ReviewDialog for &FakeHost {
        fn show(&self, review: &ReviewText) -> Result<i32, ReviewFailure> {
            (self.before_window)();
            self.events.lock().unwrap().push("confirm");
            self.reviews.lock().unwrap().push(review.text.clone());
            (self.while_confirming)();
            if let Some(failure) = self.window_result.lock().unwrap().take() {
                return Err(failure);
            }
            Ok(if self.approve.load(Ordering::SeqCst) {
                CONFIRM_BUTTON
            } else {
                CANCEL_BUTTON
            })
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
    fn case_1_a_journal_failure_before_the_disclosure_starts_nothing_and_hands_out_no_value() {
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
    fn a_lock_after_the_folder_check_shows_no_review_of_the_old_session() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let session = fixture.session.clone();
        host.while_preparing = Box::new(move || session.lock().unwrap());
        assert_eq!(launch(&fixture, &host).err().unwrap().kind, "locked");
        assert_nothing_happened(&fixture, &host, &["window", "folder"]);
        assert!(host.reviews.lock().unwrap().is_empty());
    }

    #[test]
    fn a_lock_after_the_access_began_stops_the_record_and_the_values() {
        let fixture = fake_vault(&DEV);
        let session = fixture.session.clone();
        let reviewed = session.snapshot().unwrap();
        let result = session.access_reviewed(Access::Read, reviewed, |vault| {
            session.lock().unwrap();
            let host = FakeHost::choosing(Path::new("C:/unused"));
            disclose_for_launch(&session, &host, reviewed.ticket, vault, "app", "dev")
        });
        assert_eq!(result.err().unwrap().kind, "locked");
        assert!(
            !fixture
                .journal_actions()
                .iter()
                .any(|action| action == "run-requested"),
            "a launch was recorded after the lock"
        );
    }

    #[test]
    fn the_vault_is_released_before_the_terminal_starts() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let session = fixture.session.clone();
        let released = Arc::new(AtomicBool::new(false));
        let seen = released.clone();
        host.while_launching =
            Box::new(move || seen.store(session.vault_is_released(), Ordering::SeqCst));
        launch(&fixture, &host).unwrap();
        assert!(released.load(Ordering::SeqCst));
    }

    #[test]
    fn a_variable_that_the_command_prompt_prints_is_refused_before_any_dialog() {
        for name in ["PROMPT", "prompt", "DIRCMD", "CopyCmd"] {
            let fixture = fake_vault(&[("Shown", name, "app", "dev", "fake-prompt-value")]);
            let host = FakeHost::choosing(fixture.directory.path());
            let error = launch(&fixture, &host).err().unwrap();
            assert_eq!(error.kind, "invalid", "{name}");
            assert!(!error.message.contains("fake-"), "{}", error.message);
            assert_nothing_happened(&fixture, &host, &[]);
        }
    }

    #[test]
    fn hostile_scope_text_cannot_pass_for_a_line_of_the_review() {
        let review = LaunchReview::new(
            "demo\"\nFolder: C:\\trusted".into(),
            "dev\u{202e}".into(),
            PathBuf::from("C:/real"),
            vec!["ONLY".into()],
        );
        let message = review_text(&review.body(), review.names());
        assert_eq!(message.matches("\nFolder: ").count(), 1, "{message}");
        assert!(message.contains("\nFolder: C:/real\n"), "{message}");
        assert!(!message.contains('\u{202e}'), "{message}");
        assert!(message.contains("\\u{202e}"), "{message}");
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
                 These keys stay in that terminal window, and in every program that you start from it, until you close the window. Locking AKM does not remove them.\n\
                 The terminal also gets the environment that AKM was started with, without AKM's own settings.\n\
                 A program in that folder can run when you type its name, so choose a folder that you trust.\n\
                 Secret values are not shown.\n\
                 \n\
                 The terminal will hold 2 variables:\n  OPENAI_API_KEY\n  TOKEN"
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
        assert!(review_text(&review.body(), review.names()).contains("hold 1 variable:\n  ONLY"));
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
            command.get_args().collect::<Vec<_>>(),
            ["/d"],
            "AutoRun stays off, and the page cannot add an argument"
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

    /// A real child process, with no window. The command is the one a launch prepares, plus
    /// the arguments that make it print what it received.
    #[cfg(windows)]
    #[test]
    fn a_real_child_gets_the_folder_and_the_selected_variables_and_no_akm_variable() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        assert!(
            std::env::var_os("AKM_JOURNAL_ROOT").is_some(),
            "the test needs one real AKM_ variable to inherit (.cargo/config.toml)"
        );
        let folder = tempfile::tempdir().unwrap();
        let shell = system_command_prompt().unwrap();
        assert_eq!(shell.file_name().unwrap(), "cmd.exe");
        let mut command = prepare_command(
            &shell,
            folder.path(),
            std::env::vars_os().map(|(name, _)| name),
            &[("OPENAI_API_KEY".into(), "fake-openai-value".into())],
        );
        command.args(["/c", "cd&&set"]);
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command.output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap().to_lowercase(),
            folder.path().display().to_string().to_lowercase()
        );
        let variables: Vec<&str> = lines.collect();
        assert!(variables.contains(&"OPENAI_API_KEY=fake-openai-value"));
        assert!(
            variables
                .iter()
                .any(|line| line.to_ascii_uppercase().starts_with("PATH=")),
            "the terminal keeps the normal environment, as the review says"
        );
        let akm: Vec<&&str> = variables
            .iter()
            .filter(|line| line.to_ascii_uppercase().starts_with("AKM_"))
            .collect();
        assert!(akm.is_empty(), "{akm:?}");
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
