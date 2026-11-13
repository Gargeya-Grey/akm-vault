use std::{
    ffi::{OsStr, OsString},
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use akm_core::{model::reserved_class, EnvDisclosure, Vault};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

use crate::{
    error::CommandError,
    import_review::MAX_REVIEWED_CHANGES,
    outcome::{self, Operation},
    review_window::{
        check_shown_in_full, review_name, review_names, review_path, show_exactly,
        NativeReviewDialog, ReviewAnswer, ReviewDialog,
    },
    session::{Access, Act, Binding, Effect, Held, Session, Snapshot},
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

    #[cfg(test)]
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
    /// Makes the terminal, suspended, outside the session gate. The launch commits it: under
    /// the gate, a check of the session and the revision that were reviewed, then the resume.
    /// A refusal revokes it, which terminates the process before it ran.
    fn prepare_terminal(
        &self,
        request: &LaunchRequest,
    ) -> Result<Box<dyn Effect<Output = ()> + '_>, CommandError>;
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
    /// The vault is released. Next: the suspended terminal is made, outside the gate.
    BeforeStart,
    /// The suspended terminal exists, and AKM's copy of the values is zeroed. Next: the check
    /// of the session and the revision, and the resume, under the gate.
    Prepared,
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

    fn prepare_terminal(
        &self,
        request: &LaunchRequest,
    ) -> Result<Box<dyn Effect<Output = ()> + '_>, CommandError> {
        #[cfg(windows)]
        {
            let shell = system_command_prompt()?;
            Ok(Box::new(Terminal(suspended_shell(
                &shell,
                TERMINAL_ARGUMENTS,
                &request.folder,
                &request.secrets,
                crate::process::Console::New,
            )?)))
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(CommandError::new(
                "launch",
                "A terminal can be opened on Windows only.",
            ))
        }
    }
}

/// A suspended terminal. AKM does not wait for it once it runs, and does not track it.
#[cfg(windows)]
struct Terminal(crate::process::Suspended);

#[cfg(windows)]
impl Effect for Terminal {
    type Output = ();

    fn act(&mut self, held: &Held<'_>) -> Result<(), CommandError> {
        self.0.resume(held)
    }

    fn revoke(&mut self) -> Result<(), String> {
        self.0.terminate()
    }
}

/// The whole command line of a terminal after the program. `/d` turns off the AutoRun
/// commands of the registry, which would otherwise run with the keys before the user types
/// anything. The page cannot add an argument.
const TERMINAL_ARGUMENTS: &str = "/d";

/// The environment of a terminal: the inherited one without any `AKM_*` variable (the rule of
/// `crates/akm-cli/src/child.rs`), plus the selected variables.
#[cfg(windows)]
fn launch_environment<'a>(
    inherited: impl IntoIterator<Item = (&'a OsStr, &'a OsStr)>,
    secrets: &'a [(String, String)],
) -> crate::process::EnvironmentBlock {
    crate::process::EnvironmentBlock::new(inherited, secrets, is_akm_name)
}

/// The command prompt, suspended, in `folder`, with the launch environment. It inherits no
/// handle.
#[cfg(windows)]
pub fn suspended_shell(
    shell: &Path,
    arguments: &str,
    folder: &Path,
    secrets: &[(String, String)],
    console: crate::process::Console,
) -> Result<crate::process::Suspended, CommandError> {
    let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let environment = launch_environment(
        inherited
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str())),
        secrets,
    );
    crate::process::Suspended::create(&crate::process::Spec {
        program: shell,
        arguments,
        folder,
        environment: &environment,
        console,
    })
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

fn is_akm_name(name: &OsStr) -> bool {
    name.to_string_lossy()
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("AKM_"))
}

/// The names of a scope, checked before any dialog opens or anything is journaled. The core
/// refuses a reserved name when it plans; this repeats the check of the command line
/// (`child::validate_names`), with the same `reserved_class` rule. That rule also holds
/// the variables that the command prompt reads and can print (`PROMPT`, `DIRCMD`,
/// `COPYCMD`).
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
    check_shown_in_full(
        names
            .iter()
            .map(String::as_str)
            .chain([project, environment]),
    )?;
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
    if !show_exactly(&folder) {
        return Err(CommandError::new(
            "invalidPath",
            "The path of that folder cannot be shown exactly in the confirmation window. Choose another folder.",
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
/// and copy what is needed. Blocking work runs outside it. An effect that discloses or starts
/// is a commit (`docs/design/commit-point.md`): the gate is taken again, and the session and
/// the revision are checked, in the same hold as the act.
///
/// Every launch that passes its session check leaves one outcome record (`outcome.rs`).
pub fn run_launch(
    session: &Session,
    host: &dyn TerminalHost,
    session_id: &str,
    operation_id: &str,
    project: Option<String>,
    environment: Option<String>,
) -> Result<Option<usize>, CommandError> {
    outcome::dispatched(
        session,
        Operation::Terminal,
        operation_id,
        session_id,
        |outcome, expected| {
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

            let review =
                LaunchReview::new(project.clone(), environment.clone(), folder.clone(), names);
            let dialog = host.review_dialog();
            // The last check before the window. It is not atomic with the window, and it cannot
            // be: a modal window cannot wait under the gate. A names-only review can appear just
            // after a lock, and it starts nothing, because the access below refuses (owner's
            // option (a)).
            session.ensure(ticket)?;
            host.reached(LaunchStep::LastCheckPassed);
            match review.ask(dialog.as_ref()) {
                ReviewAnswer::Approved => {}
                ReviewAnswer::Declined => return Ok(None),
                ReviewAnswer::Failed(failure) => return Err(failure.into()),
            }
            host.reached(LaunchStep::Approved);

            // The access binds the request to the session and the revision again. A lock,
            // another session or any write since the plan was read refuses it, so the plan here
            // is the one the user was shown.
            let secrets = session
                .access_reviewed(Access::Read, reviewed, |vault| {
                    disclose_for_launch(session, host, reviewed, vault, &project, &environment)
                })?
                .value;
            let count = secrets.len();
            host.reached(LaunchStep::BeforeStart);
            // No terminal is made for a session that already ended.
            session.ensure(ticket)?;
            let mut terminal = {
                let request = LaunchRequest { folder, secrets };
                host.prepare_terminal(&request)?
                // The request drops here: AKM's copy of the values is zeroed before the commit.
            };
            host.reached(LaunchStep::Prepared);
            // A write since the review moved the revision at its own commit, so it refuses this
            // one.
            session.commit(Binding::Revision(reviewed), terminal.as_mut())?;
            // The terminal runs from here on, so the record says so, whatever comes after.
            outcome.done();
            host.reached(LaunchStep::Started);
            Ok(Some(count))
        },
    )
}

/// The record, then the values. The session is checked before each one: a lock that came first
/// stops it. The copy of the values is a commit, and runs only after the record is durable.
fn disclose_for_launch(
    session: &Session,
    host: &dyn TerminalHost,
    reviewed: Snapshot,
    vault: &mut Vault,
    project: &str,
    environment: &str,
) -> Result<LaunchSecrets, CommandError> {
    let plan = vault.plan_env(Some(project), Some(environment), false)?;
    session.ensure(reviewed.ticket)?;
    host.reached(LaunchStep::RecordNext);
    // The journal takes a file lock and can stall, so it runs outside the gate. A lock during
    // the write is prompt. It leaves a record for a launch that never starts (owner's option (a)).
    let proof = vault.disclose_env(
        EnvDisclosure::Run,
        &plan,
        Some(&format!("launch requested; {} keys selected", plan.len())),
    )?;
    host.reached(LaunchStep::Recorded);
    let values = session.commit(
        Binding::Revision(reviewed),
        &mut Act::new(|_held: &Held<'_>| vault.env_vars(&plan, &proof).map_err(CommandError::from)),
    )?;
    host.reached(LaunchStep::ValuesCopied);
    Ok(LaunchSecrets(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation() -> String {
        uuid::Uuid::new_v4().to_string()
    }
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
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc, Mutex,
        },
        time::Duration,
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
        fake_vault_on(entries, Arc::new(MonotonicClock::default()))
    }

    fn fake_vault_on(entries: &[Entry], clock: Arc<dyn crate::session::Clock>) -> Fixture {
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
        let session = Session::new(
            clock,
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        );
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
        /// Runs inside the making of the suspended terminal: a barrier inside process creation.
        while_creating: Box<dyn Fn() + Send + Sync>,
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
                while_creating: Box::new(|| {}),
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

        fn prepare_terminal(
            &self,
            request: &LaunchRequest,
        ) -> Result<Box<dyn Effect<Output = ()> + '_>, CommandError> {
            self.events.lock().unwrap().push("prepare");
            (self.while_creating)();
            Ok(Box::new(FakeTerminal {
                host: self,
                launched: Some(Launched {
                    folder: request.folder.clone(),
                    envs: request.secrets.to_vec(),
                }),
            }))
        }
    }

    /// A suspended terminal, scripted. The commit records the start; a revoke drops it.
    struct FakeTerminal<'a> {
        host: &'a FakeHost,
        launched: Option<Launched>,
    }

    impl Effect for FakeTerminal<'_> {
        type Output = ();

        fn act(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
            self.host.events.lock().unwrap().push("launch");
            (self.host.while_launching)();
            self.host
                .launched
                .lock()
                .unwrap()
                .extend(self.launched.take());
            Ok(())
        }

        fn revoke(&mut self) -> Result<(), String> {
            self.host.events.lock().unwrap().push("revoked");
            self.launched = None;
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
            &operation(),
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

    /// A clock that a test moves, to reach the idle deadline.
    #[derive(Default)]
    struct SteppedClock(AtomicU64);
    impl crate::session::Clock for SteppedClock {
        fn now(&self) -> Duration {
            Duration::from_secs(self.0.load(Ordering::SeqCst))
        }
    }

    fn held_journal_lock(fixture: &Fixture) -> std::fs::File {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(crate::test_support::journal_of(&fixture.vault_path()).lock_path())
            .unwrap();
        file.lock().unwrap();
        file
    }

    #[test]
    fn a_held_journal_lock_does_not_delay_a_lock_and_the_launch_then_starts_nothing() {
        for how in ["explicit lock", "idle expiry", "Windows lock", "suspend"] {
            let clock = Arc::new(SteppedClock::default());
            let fixture = fake_vault_on(&DEV, clock.clone());
            let held = held_journal_lock(&fixture);
            let (at_record, reached_record) = mpsc::channel();
            let mut host = FakeHost::choosing(fixture.directory.path());
            host.at_step = Box::new(move |step| {
                if step == LaunchStep::RecordNext {
                    at_record.send(()).unwrap();
                }
            });
            let host = Arc::new(host);
            let (worker, worker_host, id) =
                (fixture.session.clone(), host.clone(), fixture.session_id());
            let launch = std::thread::spawn(move || {
                run_launch(
                    &worker,
                    worker_host.as_ref(),
                    &id,
                    &operation(),
                    Some("app".into()),
                    Some("dev".into()),
                )
            });
            reached_record
                .recv_timeout(crate::test_support::PATIENCE)
                .expect("the launch reached the journal");
            std::thread::sleep(Duration::from_millis(200));

            let (done, finished) = mpsc::channel();
            let session = fixture.session.clone();
            let locker = std::thread::spawn(move || {
                match how {
                    "explicit lock" => session.lock().unwrap(),
                    "idle expiry" => {
                        clock.0.store(100_000, Ordering::SeqCst);
                        session.tick();
                    }
                    "Windows lock" => session.observe(Signal::Locked),
                    _ => session.observe(Signal::Suspend),
                }
                done.send(()).unwrap();
            });
            finished
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|_| panic!("{how}: the lock waited for the journal"));
            locker.join().unwrap();
            assert!(fixture.session.snapshot().is_err(), "{how}");

            drop(held);
            let outcome = launch.join().unwrap();
            assert_eq!(outcome.err().unwrap().kind, "locked", "{how}");
            assert!(host.launched().is_empty(), "{how}: a terminal started");
            assert_eq!(host.events(), ["window", "folder", "confirm"], "{how}");
            assert!(
                !host
                    .steps
                    .lock()
                    .unwrap()
                    .contains(&LaunchStep::ValuesCopied),
                "{how}: values left the vault"
            );
            assert_eq!(
                fixture
                    .journal_actions()
                    .iter()
                    .filter(|action| *action == "run-requested")
                    .count(),
                1,
                "{how}: the accepted limit is one record for a launch that never started"
            );
        }
    }

    #[test]
    fn a_launch_hands_the_selected_variables_and_the_chosen_folder_to_the_launcher() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        assert_eq!(launch(&fixture, &host).unwrap(), Some(2));
        assert_eq!(
            host.events(),
            ["window", "folder", "confirm", "prepare", "launch"]
        );
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

    /// Replaces case 2b, which asserted that a lock waits for process creation. The terminal
    /// is now made suspended, outside the gate: a lock during creation finishes at once, and
    /// the commit then refuses and terminates the terminal.
    #[test]
    fn a_lock_while_the_terminal_is_created_is_prompt_and_the_terminal_never_starts() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let session = fixture.session.clone();
        let prompt = Arc::new(AtomicBool::new(false));
        let seen = prompt.clone();
        host.while_creating = Box::new(move || {
            let (done, finished) = mpsc::channel();
            let session = session.clone();
            std::thread::spawn(move || {
                session.lock().unwrap();
                done.send(()).unwrap();
            });
            let finished = finished.recv_timeout(Duration::from_secs(10)).is_ok();
            seen.store(finished, Ordering::SeqCst);
        });
        let error = launch(&fixture, &host).unwrap_err();
        assert!(
            prompt.load(Ordering::SeqCst),
            "the lock waited for process creation"
        );
        assert_eq!(error.kind, "locked");
        assert_eq!(
            host.events(),
            ["window", "folder", "confirm", "prepare", "revoked"]
        );
        assert!(host.launched().is_empty(), "a terminal started");
    }

    /// The values were read at the reviewed revision. A rotate or a remove of the same session
    /// after that moves the revision at its own commit, so the start refuses.
    #[test]
    fn a_same_session_rotate_or_remove_at_the_last_step_before_the_start_refuses_it() {
        for change in ["rotate", "remove"] {
            let fixture = fake_vault(&DEV);
            let mut host = FakeHost::choosing(fixture.directory.path());
            let session = fixture.session.clone();
            host.at_step = Box::new(move |step| {
                if step != LaunchStep::Prepared {
                    return;
                }
                session
                    .access(Access::Write, None, |vault| {
                        let id = vault
                            .list()
                            .into_iter()
                            .find(|key| key.title == "Token")
                            .unwrap()
                            .id;
                        if change == "rotate" {
                            vault.rotate(&id, "fake-rotated-value".into()).map(|_| ())
                        } else {
                            vault.remove(&id).map(|_| ())
                        }
                        .map_err(Into::into)
                    })
                    .unwrap();
            });
            let error = launch(&fixture, &host).unwrap_err();
            assert_eq!(error.kind, "stalePreview", "{change}");
            assert_eq!(
                host.events(),
                ["window", "folder", "confirm", "prepare", "revoked"],
                "{change}"
            );
            assert!(host.launched().is_empty(), "{change}: a terminal started");
        }
    }

    /// A write that passed its checks and waits at its own commit point when the launch commits.
    /// The gate orders the two: the terminal starts with the reviewed values, then the write
    /// replaces the file. Both answers are true.
    #[test]
    fn a_write_in_progress_at_the_start_is_ordered_by_the_gate_and_both_answers_are_true() {
        let fixture = fake_vault(&DEV);
        let before = std::fs::read(fixture.vault_path()).unwrap();
        let (arrived, at_commit) = mpsc::channel::<()>();
        let (release, go) = mpsc::channel::<()>();
        let (arrived, go) = (Mutex::new(arrived), Mutex::new(go));
        fixture.session.at_commit(move || {
            arrived.lock().unwrap().send(()).unwrap();
            go.lock().unwrap().recv().unwrap();
        });
        let writer = Arc::new(Mutex::new(None));
        let (session, slot) = (fixture.session.clone(), writer.clone());
        let (at_commit, release) = (Mutex::new(at_commit), Mutex::new(release));
        let mut host = FakeHost::choosing(fixture.directory.path());
        host.at_step = Box::new(move |step| match step {
            LaunchStep::Prepared => {
                let session = session.clone();
                *slot.lock().unwrap() = Some(std::thread::spawn(move || {
                    session
                        .access(Access::Write, None, |vault| {
                            let id = vault
                                .list()
                                .into_iter()
                                .find(|key| key.title == "Token")
                                .unwrap()
                                .id;
                            vault
                                .rotate(&id, "fake-rotated-value".into())
                                .map_err(Into::into)
                        })
                        .map(|done| done.snapshot.revision)
                }));
                at_commit
                    .lock()
                    .unwrap()
                    .recv_timeout(crate::test_support::PATIENCE)
                    .expect("the write reached its commit point");
            }
            LaunchStep::Started => release.lock().unwrap().send(()).unwrap(),
            _ => {}
        });
        assert_eq!(launch(&fixture, &host).unwrap(), Some(2));
        let revision = writer.lock().unwrap().take().unwrap().join().unwrap();
        assert_eq!(revision.unwrap(), 1, "the write committed after the start");
        assert_eq!(
            host.launched()[0].envs,
            [
                (
                    "OPENAI_API_KEY".to_string(),
                    "fake-openai-value".to_string()
                ),
                ("TOKEN".to_string(), "fake-token-value".to_string()),
            ],
            "the terminal holds the reviewed values"
        );
        assert_ne!(std::fs::read(fixture.vault_path()).unwrap(), before);
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
            disclose_for_launch(&session, &host, reviewed, vault, "app", "dev")
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

    /// The text of both reviews for one name, as the windows would show it.
    fn both_reviews(name: &str) -> [String; 2] {
        let launch = LaunchReview::new(
            "app".into(),
            "dev".into(),
            PathBuf::from("C:/work"),
            vec![name.to_string()],
        );
        let import = crate::import_review::ImportReview::new(
            "app".into(),
            "dev".into(),
            akm_core::ImportPreview {
                added: 1,
                updated: 0,
                unchanged: 0,
                changes: vec![akm_core::model::ImportChange {
                    name: name.into(),
                    title: name.into(),
                    action: "add".into(),
                }],
            },
        )
        .unwrap();
        [
            review_text(&launch.body(), launch.names()),
            import.message(),
        ]
    }

    #[test]
    fn names_that_differ_only_after_80_characters_read_differently_in_both_reviews() {
        let prefix = "A".repeat(80);
        let (first, second) = (format!("{prefix}_ONE"), format!("{prefix}_TWO"));
        let (one, two) = (both_reviews(&first), both_reviews(&second));
        for review in 0..2 {
            assert_ne!(one[review], two[review], "review {review}");
            assert!(one[review].contains(&first), "review {review}: not in full");
        }
    }

    #[test]
    fn a_real_control_character_and_its_literal_text_read_differently_in_both_reviews() {
        for (real, literal) in [
            ("dev\u{202e}", "dev\\u{202e}"),
            ("a\u{200b}b", "a\\u{200b}b"),
            ("\u{feff}x", "\\u{feff}x"),
        ] {
            let (real, literal) = (both_reviews(real), both_reviews(literal));
            for review in 0..2 {
                assert_ne!(real[review], literal[review], "{}", real[review]);
                for hidden in ['\u{202e}', '\u{200b}', '\u{feff}'] {
                    assert!(!real[review].contains(hidden), "{}", real[review]);
                }
            }
        }
    }

    #[test]
    fn a_name_too_long_to_show_in_full_is_refused_before_any_dialog() {
        let long: &'static str = Box::leak("x".repeat(400).into_boxed_str());
        let fixture = fake_vault(&[("Long", "TOKEN", long, "dev", "fake-long")]);
        let host = FakeHost::choosing(fixture.directory.path());
        let error = run_launch(
            &fixture.session,
            &host,
            &fixture.session_id(),
            &operation(),
            Some(long.into()),
            Some("dev".into()),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, "tooLarge");
        assert!(!error.message.contains(long), "{}", error.message);
        assert_nothing_happened(&fixture, &host, &[]);
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_that_a_review_could_not_show_exactly_is_refused_before_the_review() {
        use std::os::windows::ffi::OsStringExt;
        let fixture = fake_vault(&DEV);
        let literal = fixture.directory.path().join("u{202e}");
        std::fs::create_dir(&literal).unwrap();
        assert!(literal.is_dir());
        let host = FakeHost::choosing(&literal);
        let error = launch(&fixture, &host).err().unwrap();
        assert_eq!(error.kind, "invalidPath");
        assert!(
            error.message.contains("cannot be shown exactly"),
            "{}",
            error.message
        );
        assert_nothing_happened(&fixture, &host, &["window", "folder"]);

        let surrogate = PathBuf::from(OsString::from_wide(&[0x43, 0x3a, 0x5c, 0xd800]));
        let host = FakeHost::choosing(&surrogate);
        let error = launch(&fixture, &host).err().unwrap();
        assert_eq!(error.kind, "invalidPath");
        assert!(
            error.message.contains("cannot be shown exactly"),
            "{}",
            error.message
        );
        assert_nothing_happened(&fixture, &host, &["window", "folder"]);
    }

    #[test]
    fn a_lock_just_before_the_window_appears_starts_nothing_even_if_the_user_approves() {
        let fixture = fake_vault(&DEV);
        let mut host = FakeHost::choosing(fixture.directory.path());
        let session = fixture.session.clone();
        host.before_window = Box::new(move || session.lock().unwrap());
        let error = launch(&fixture, &host).err().unwrap();
        assert_eq!(error.kind, "locked");
        assert_eq!(host.events(), ["window", "folder", "confirm"]);
        assert!(host.launched().is_empty(), "a terminal started");
        assert!(
            !fixture
                .journal_actions()
                .iter()
                .any(|action| action == "run-requested"),
            "a launch was recorded after the lock"
        );
        let steps = host.steps.lock().unwrap().clone();
        assert!(steps.contains(&LaunchStep::LastCheckPassed), "{steps:?}");
        assert!(!steps.contains(&LaunchStep::RecordNext), "{steps:?}");
        assert!(!steps.contains(&LaunchStep::ValuesCopied), "{steps:?}");
        assert!(!steps.contains(&LaunchStep::BeforeStart), "{steps:?}");
    }

    #[test]
    fn a_lock_at_each_step_of_the_launch_has_the_outcome_of_the_state_table() {
        use LaunchStep::*;
        // The step, whether a review appears, whether a record is written, whether the values
        // leave the vault, whether a suspended terminal is made, and whether it starts. A made
        // terminal that does not start is revoked.
        let table = [
            (Planned, false, false, false, false, false),
            (FolderChecked, false, false, false, false, false),
            (LastCheckPassed, true, false, false, false, false),
            (Approved, true, false, false, false, false),
            (RecordNext, true, true, false, false, false),
            (Recorded, true, true, false, false, false),
            (ValuesCopied, true, true, true, false, false),
            (BeforeStart, true, true, true, false, false),
            (Prepared, true, true, true, true, false),
            (Started, true, true, true, true, true),
        ];
        for event in ["lock", "new session"] {
            for (step, review, record, values, prepared, started) in table {
                let fixture = fake_vault(&DEV);
                let mut host = FakeHost::choosing(fixture.directory.path());
                let session = fixture.session.clone();
                let opening = Arc::new(Mutex::new(None));
                let slot = opening.clone();
                host.at_step = Box::new(move |reached| {
                    if reached == step {
                        session.lock().unwrap();
                        if event == "new session" {
                            *slot.lock().unwrap() = Some(session.begin_open().unwrap());
                        }
                    }
                });
                let result = launch(&fixture, &host);
                let row = format!("{event} at {step:?}");
                if started {
                    assert_eq!(result.unwrap(), Some(2), "{row}");
                } else {
                    assert_eq!(result.err().unwrap().kind, "locked", "{row}");
                }
                assert_eq!(host.reviews.lock().unwrap().len() == 1, review, "{row}");
                assert_eq!(
                    fixture
                        .journal_actions()
                        .iter()
                        .any(|action| action == "run-requested"),
                    record,
                    "{row}"
                );
                let steps = host.steps.lock().unwrap().clone();
                let events = host.events();
                assert_eq!(steps.contains(&ValuesCopied), values, "{row}: {steps:?}");
                assert_eq!(events.contains(&"prepare"), prepared, "{row}: {events:?}");
                assert_eq!(
                    events.contains(&"revoked"),
                    prepared && !started,
                    "{row}: {events:?}"
                );
                assert_eq!(!host.launched().is_empty(), started, "{row}");
                if !started {
                    assert!(!steps.contains(&Started), "{row}");
                }
                let opened = opening.lock().unwrap().take();
                if let Some(ticket) = opened {
                    let path = fixture.vault_path();
                    fixture
                        .session
                        .open(ticket, || Vault::open(&path, PASSWORD).map_err(Into::into))
                        .unwrap();
                    let fresh = fixture.session.snapshot().unwrap();
                    assert_eq!((fresh.ticket, fresh.revision), (ticket, 0), "{row}");
                }
            }
        }
    }

    #[test]
    fn a_failed_review_window_is_an_error_and_starts_nothing() {
        let fixture = fake_vault(&DEV);
        let host = FakeHost::choosing(fixture.directory.path());
        *host.window_result.lock().unwrap() = Some(ReviewFailure::new("TaskDialogIndirect failed"));
        let error = launch(&fixture, &host).err().unwrap();
        assert_eq!(error.kind, "reviewFailed");
        assert!(host.launched().is_empty());
        assert!(!fixture
            .journal_actions()
            .iter()
            .any(|action| action == "run-requested"));
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

    /// The names of each reserved class that this table checks. The match has no wildcard, so
    /// a new class in the core does not compile until it has a row here.
    fn reserved_names(class: akm_core::model::ReservedClass) -> &'static [&'static str] {
        use akm_core::model::ReservedClass::*;
        match class {
            AkmInternal => &["AKM_PASSWORD", "akm_id"],
            ProcessControl => &[
                "PATH",
                "PATHEXT",
                "ComSpec",
                "SYSTEMROOT",
                "WINDIR",
                "PSMODULEPATH",
                "LD_PRELOAD",
                "RUSTUP_TOOLCHAIN",
            ],
            StartupHook => &[
                "NODE_OPTIONS",
                "PYTHONSTARTUP",
                "JAVA_TOOL_OPTIONS",
                "DOTNET_STARTUP_HOOKS",
                "COR_PROFILER",
                "CORECLR_ENABLE_PROFILING",
            ],
            HelperProgram => &[
                "GIT_EDITOR",
                "git_editor",
                "GIT_SEQUENCE_EDITOR",
                "GIT_SSH_COMMAND",
                "EDITOR",
                "PAGER",
                "RUSTC_WRAPPER",
                "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER",
                "GIT_CONFIG_COUNT",
            ],
            // The shells: cmd, PowerShell and the POSIX shells that tools start.
            ShellStartup => &[
                "PROMPT",
                "DIRCMD",
                "COPYCMD",
                "PSEXECUTIONPOLICYPREFERENCE",
                "__PSLOCKDOWNPOLICY",
                "BASH_ENV",
                "ENV",
                "PROMPT_COMMAND",
            ],
            TrafficRedirect => &[
                "HTTPS_PROXY",
                "ALL_PROXY",
                "NODE_EXTRA_CA_CERTS",
                "SSLKEYLOGFILE",
                "PIP_INDEX_URL",
                "NPM_CONFIG_REGISTRY",
            ],
            UserLocation => &["TEMP", "TMP", "USERPROFILE", "APPDATA", "CARGO_HOME"],
            OutputCapture => &[
                "GIT_TRACE",
                "git_trace2_event",
                "GIT_REDIRECT_STDERR",
                "NODE_V8_COVERAGE",
                "PYTHON_HISTORY",
            ],
            SettingsFile => &[
                "GOOGLE_APPLICATION_CREDENTIALS",
                "KUBECONFIG",
                "aws_shared_credentials_file",
            ],
        }
    }

    /// The class after `class`, from no class to the last one. The match has no wildcard, so a
    /// new class in the core does not compile until the walk below reaches it.
    fn next_class(
        class: Option<akm_core::model::ReservedClass>,
    ) -> Option<akm_core::model::ReservedClass> {
        use akm_core::model::ReservedClass::*;
        match class {
            None => Some(AkmInternal),
            Some(AkmInternal) => Some(ProcessControl),
            Some(ProcessControl) => Some(StartupHook),
            Some(StartupHook) => Some(HelperProgram),
            Some(HelperProgram) => Some(ShellStartup),
            Some(ShellStartup) => Some(TrafficRedirect),
            Some(TrafficRedirect) => Some(UserLocation),
            Some(UserLocation) => Some(OutputCapture),
            Some(OutputCapture) => Some(SettingsFile),
            Some(SettingsFile) => None,
        }
    }

    /// T-R3-02, through the production launch boundary (`run_launch`). A key of the scope whose
    /// variable is reserved, of any class and in any case, stops the launch before any dialog:
    /// nothing is journaled, no value is copied and no terminal is made. A registry token of a
    /// reserved family and an ordinary name still start.
    #[test]
    fn t_r3_02_every_reserved_class_and_every_shell_name_is_refused_at_the_launch_boundary() {
        let classes: Vec<_> =
            std::iter::successors(next_class(None), |class| next_class(Some(*class))).collect();
        assert_eq!(classes.len(), 9);
        for class in classes {
            assert!(!reserved_names(class).is_empty(), "{class:?}");
            for name in reserved_names(class) {
                assert_eq!(reserved_class(name), Some(class), "{name}");
                let fixture =
                    fake_vault(&[DEV[0], ("Reserved", name, "app", "dev", "fake-reserved")]);
                let host = FakeHost::choosing(fixture.directory.path());
                let error = launch(&fixture, &host).expect_err(name);
                assert_eq!(error.kind, "invalid", "{name}");
                assert!(!error.message.contains("fake-"), "{}", error.message);
                assert_nothing_happened(&fixture, &host, &[]);
            }
        }
        for allowed in [
            "CARGO_REGISTRY_TOKEN",
            "CARGO_REGISTRIES_FAKE_TOKEN",
            "OPENAI_API_KEY",
        ] {
            assert_eq!(reserved_class(allowed), None, "{allowed}");
            let fixture = fake_vault(&[("Allowed", allowed, "app", "dev", "fake-allowed")]);
            let host = FakeHost::choosing(fixture.directory.path());
            assert_eq!(launch(&fixture, &host).unwrap(), Some(1), "{allowed}");
        }
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
            &operation(),
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
                &operation(),
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
                &operation(),
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
            &operation(),
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

    fn block(inherited: &[(&str, &str)], secrets: &[(&str, &str)]) -> Vec<String> {
        let secrets: Vec<(String, String)> = secrets
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        launch_environment(
            inherited
                .iter()
                .map(|(name, value)| (OsStr::new(name), OsStr::new(value))),
            &secrets,
        )
        .strings()
    }

    #[test]
    fn case_7_the_child_environment_holds_the_selected_variables_and_no_akm_variable() {
        assert_eq!(
            TERMINAL_ARGUMENTS, "/d",
            "AutoRun stays off, and the page cannot add an argument"
        );
        assert_eq!(
            block(
                &[
                    ("PATH", "C:\\Windows"),
                    ("AKM_PASSWORD", "fake-inherited"),
                    ("akm_journal_root", "fake-root"),
                    ("Akm_Other", "fake-other"),
                ],
                &[
                    ("OPENAI_API_KEY", "fake-openai-value"),
                    ("TOKEN", "fake-token-value"),
                ],
            ),
            [
                "OPENAI_API_KEY=fake-openai-value",
                "PATH=C:\\Windows",
                "TOKEN=fake-token-value",
            ],
            "PATH stays inherited, untouched"
        );
    }

    /// A real child process, suspended and then resumed, with no window. It is made by the
    /// function that makes a terminal, with the arguments of a terminal plus the ones that make
    /// it print what it received.
    #[cfg(windows)]
    #[test]
    fn a_real_child_gets_the_folder_and_the_selected_variables_and_no_akm_variable() {
        assert!(
            std::env::var_os("AKM_JOURNAL_ROOT").is_some(),
            "the test needs one real AKM_ variable to inherit (.cargo/config.toml)"
        );
        let folder = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let printed = output.path().join("child.txt");
        let shell = system_command_prompt().unwrap();
        assert_eq!(shell.file_name().unwrap(), "cmd.exe");
        let arguments = format!(
            "{TERMINAL_ARGUMENTS} /c cd>\"{0}\"&&set>>\"{0}\"",
            printed.display()
        );
        let mut child = suspended_shell(
            &shell,
            &arguments,
            folder.path(),
            &[("OPENAI_API_KEY".into(), "fake-openai-value".into())],
            crate::process::Console::Hidden,
        )
        .unwrap();
        assert!(!printed.exists(), "a suspended child ran");
        child.resume(&Held::for_test()).unwrap();
        assert_eq!(
            child.wait(crate::test_support::PATIENCE.as_millis() as u32),
            Some(0)
        );
        let text = std::fs::read_to_string(&printed).unwrap();
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
        assert!(block(&[], &[("AKM_PASSWORD", "fake-leak")]).is_empty());
        assert!(block(&[], &[("akm_password", "fake-leak")]).is_empty());
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
            // The one rule of the core gives the same answer (class AkmInternal).
            assert_eq!(
                reserved_class(name) == Some(akm_core::model::ReservedClass::AkmInternal),
                removed,
                "{name:?}"
            );
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
