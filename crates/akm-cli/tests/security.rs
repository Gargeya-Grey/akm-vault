use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use akm_core::audit::AuditLog;
use akm_core::journal::{JournalRoot, VaultEvidence};
use akm_core::model::{META_ENVIRONMENT, META_ENV_VAR, META_PROJECT};
use akm_core::Vault;
use keepass::config::KdfConfig;
use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use serde_json::{json, Value};
use tempfile::TempDir;

#[path = "../src/export.rs"]
mod export;

const PASSWORD: &str = "fake-cli-test-password";

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_akm"));
    let journal_root = std::env::var_os("AKM_JOURNAL_ROOT");
    for (name, _) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("AKM_") || upper.starts_with("CLI_TEST_") {
            command.env_remove(name);
        }
    }
    command.stdin(Stdio::null());
    if let Some(root) = journal_root {
        command.env("AKM_JOURNAL_ROOT", root);
    }
    command
}

/// A journal handle for a test: the log, and the path of its journal file (bound, not yet created).
struct Journal {
    log: AuditLog,
    path: PathBuf,
}

impl Journal {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl std::ops::Deref for Journal {
    type Target = AuditLog;
    fn deref(&self) -> &AuditLog {
        &self.log
    }
}

fn journal_for(root: JournalRoot, vault: &Path) -> Journal {
    let log = AuditLog::new(root, VaultEvidence::from_file(vault));
    let path = log.create_journal_path().unwrap();
    Journal { log, path }
}

/// The journal of a vault in the Cargo-provided test root.
fn default_journal(vault: &Path) -> Journal {
    journal_for(JournalRoot::from_environment(), vault)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn temporary_directory() -> TempDir {
    let base = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("akm-cli-security-")
        .tempdir_in(base)
        .unwrap()
}

struct Fixture {
    directory: TempDir,
    vault: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_keys(&[("Demo", "DEMO_SECRET", "fake-intended-secret", "demo", "dev")])
    }

    fn with_keys(keys: &[(&str, &str, &str, &str, &str)]) -> Self {
        let directory = temporary_directory();
        let vault = directory.path().join("vault.kdbx");
        write_database(&vault, PASSWORD, keys);
        let root = directory.path().join("journal-root");
        Self {
            directory,
            vault,
            root,
        }
    }

    fn log(&self) -> Journal {
        self.log_for(&self.vault)
    }

    fn log_for(&self, vault: &Path) -> Journal {
        journal_for(JournalRoot::at(&self.root), vault)
    }

    fn cli(&self) -> Command {
        let mut command = cli();
        command
            .env("AKM_JOURNAL_ROOT", &self.root)
            .env("AKM_PASSWORD", PASSWORD)
            .arg("--vault")
            .arg(&self.vault);
        command
    }

    fn report(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn probe_command(&self, report: &Path) -> Command {
        let mut command = self.cli();
        command
            .env("CLI_TEST_REPORT", report)
            .args(["run", "--project", "demo", "--env", "dev", "--"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "child_probe", "--ignored", "--nocapture"]);
        command
    }
}

fn write_database(path: &Path, password: &str, keys: &[(&str, &str, &str, &str, &str)]) {
    let mut db = Database::new();
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        for (title, name, secret, project, environment) in keys {
            let mut entry = group.add_entry();
            entry.set_unprotected(fields::TITLE, *title);
            entry.set_protected(fields::PASSWORD, *secret);
            entry.set_unprotected(META_ENV_VAR, *name);
            entry.set_unprotected(META_PROJECT, *project);
            entry.set_unprotected(META_ENVIRONMENT, *environment);
        }
    }
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(password),
    )
    .unwrap();
}

/// The stored secret of a key, read from the raw database so that the test adds no
/// journal record.
fn stored_secret(vault: &Path, title: &str) -> String {
    let mut db = Database::open(
        &mut fs::File::open(vault).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let mut found = None;
    db.foreach_entry_mut(|entry| {
        if entry.get(fields::TITLE) == Some(title) {
            found = entry.get(fields::PASSWORD).map(str::to_string);
        }
    });
    found.expect("a key with that title")
}

fn report(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

#[test]
fn password_argument_is_not_accepted_or_echoed() {
    let directory = temporary_directory();
    for args in [
        vec!["--password", "argument-must-stay-private", "list"],
        vec!["list", "--password", "argument-must-stay-private"],
        vec!["init", "--password", "argument-must-stay-private"],
        vec!["export", "--password", "argument-must-stay-private"],
    ] {
        let output = cli()
            .arg("--vault")
            .arg(directory.path().join("must-not-open.kdbx"))
            .args(args)
            .output()
            .unwrap();
        let error = stderr(&output);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            error.contains("unexpected argument '--password'"),
            "{error}"
        );
        assert!(!error.contains("argument-must-stay-private"), "{error}");
    }
}

#[test]
fn unscoped_export_is_rejected_before_opening_the_vault() {
    let directory = temporary_directory();
    let output = cli()
        .env("AKM_PASSWORD", PASSWORD)
        .arg("--vault")
        .arg(directory.path().join("must-not-open.kdbx"))
        .arg("export")
        .output()
        .unwrap();
    let error = stderr(&output);
    assert!(!output.status.success());
    assert!(
        error.contains("--project") && error.contains("--env"),
        "{error}"
    );
    assert!(!error.contains("vault not found"), "{error}");
    assert!(output.stdout.is_empty());
}

#[test]
fn help_does_not_disclose_password_or_vault_environment_values() {
    for args in [
        vec!["--help"],
        vec!["run", "--help"],
        vec!["export", "--help"],
    ] {
        let output = cli()
            .env("AKM_PASSWORD", "fake-password-never-in-help")
            .env("AKM_VAULT", "fake-vault-never-in-help")
            .args(args)
            .output()
            .unwrap();
        assert_success(&output);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("--password"), "{text}");
        assert!(!text.contains("fake-password-never-in-help"), "{text}");
        assert!(!text.contains("fake-vault-never-in-help"), "{text}");
    }
}

#[test]
fn scope_is_complete_nonempty_and_exclusive_before_unlock() {
    let directory = temporary_directory();
    for verb in ["run", "export"] {
        for selection in [
            vec![],
            vec!["--project", "demo"],
            vec!["--env", "dev"],
            vec!["--project", "", "--env", "dev"],
            vec!["--project", "demo", "--env", "   "],
            vec!["--all", "--project", "demo"],
            vec!["--all", "--env", "dev"],
        ] {
            let mut command = cli();
            command
                .arg("--vault")
                .arg(directory.path().join("must-not-open.kdbx"))
                .arg(verb);
            command.args(&selection);
            if verb == "run" {
                command.arg("--").arg(std::env::current_exe().unwrap());
            }
            let output = command.output().unwrap();
            let error = stderr(&output);
            assert_eq!(
                output.status.code(),
                Some(2),
                "{verb} {selection:?}: {error}"
            );
            assert!(!error.contains("password"), "{error}");
            assert!(!error.contains("vault not found"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn new_vault_password_policy_also_applies_to_environment_passwords() {
    let directory = temporary_directory();
    let path = directory.path().join("new.kdbx");
    let output = cli()
        .env("AKM_PASSWORD", "tiny")
        .arg("--vault")
        .arg(&path)
        .arg("init")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("12"), "{}", stderr(&output));
    assert!(!path.exists());

    let output = cli()
        .env("AKM_PASSWORD", PASSWORD)
        .arg("--vault")
        .arg(&path)
        .arg("init")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(Vault::open(&path, PASSWORD).unwrap().list().is_empty());
}

#[test]
fn existing_short_password_vaults_still_unlock() {
    let directory = temporary_directory();
    let path = directory.path().join("old-password.kdbx");
    write_database(&path, "tiny", &[]);
    let output = cli()
        .env("AKM_PASSWORD", "tiny")
        .arg("--vault")
        .arg(path)
        .arg("list")
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(output.stdout, b"no keys\n");
}

#[test]
fn run_scrubs_all_akm_variables_preserves_normal_environment_and_child_exit() {
    let fixture = Fixture::new();
    let path = fixture.report("child.json");
    let audit = fixture.log();
    let output = fixture
        .probe_command(&path)
        .env("AKM_VAULT", &fixture.vault)
        .env("aKm_ToKeN", "fake-mixed-case-private-token")
        .env("AkM_Another", "fake-private-token")
        .env("CLI_TEST_NORMAL", "ordinary-inherited-value")
        .env("CLI_TEST_AUDIT", audit.path())
        .env("CLI_TEST_BLOCK_AUDIT", "1")
        .env("CLI_TEST_EXIT", "7")
        .env("DEMO_SECRET", "wrong-inherited-secret")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7), "{}", stderr(&output));
    let child = report(&path);
    assert_eq!(child["akm_names"], json!([]));
    assert_eq!(child["secret"], "fake-intended-secret");
    assert_eq!(child["normal"], "ordinary-inherited-value");
    assert_eq!(child["path"], std::env::var("PATH").unwrap());
    assert_eq!(child["last_audit_action"], "run-requested");
    assert!(audit.path().is_dir());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn normal_executable_receives_literal_arguments() {
    let fixture = Fixture::new();
    let path = fixture.report("exe.json");
    let arguments = [
        "space argument",
        "literal&value",
        "literal|value",
        "literal<value",
        "literal>value",
        "literal^value",
        "(literal)",
        "%CLI_TEST_NORMAL%",
        "!CLI_TEST_NORMAL!",
        "literal\"quote",
    ];
    let mut command = fixture.probe_command(&path);
    for argument in arguments {
        command.args(["--skip", argument]);
    }
    let output = command.output().unwrap();
    assert_success(&output);
    assert_eq!(report(&path)["arguments"], json!(arguments));
}

#[test]
fn child_run_does_not_hold_the_vault_open_or_block_other_cli_reads() {
    let fixture = Fixture::new();
    let path = fixture.report("running.json");
    let release = fixture.report("release");
    let mut child = fixture
        .probe_command(&path)
        .env("CLI_TEST_RELEASE", &release)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let was_running = path.exists() && child.try_wait().unwrap().is_none();
    let read = fixture.cli().args(["list", "--json"]).output().unwrap();
    fs::write(&release, b"release").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(was_running, "{}", stderr(&output));
    assert_success(&output);
    assert_success(&read);
    let keys: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(keys[0]["title"], "Demo");
    assert_eq!(keys[0]["secret"], "***");
}

#[test]
fn no_match_does_not_launch_a_child_or_export_inherited_secrets() {
    let fixture = Fixture::new();
    let path = fixture.report("must-not-exist.json");
    for verb in ["run", "export"] {
        let mut command = fixture.cli();
        command
            .env("CLI_TEST_REPORT", &path)
            .args([verb, "--project", "missing", "--env", "dev"]);
        if verb == "run" {
            command.arg("--").arg(std::env::current_exe().unwrap());
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("no keys matched"),
            "{}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty());
        assert!(!path.exists());
    }
}

#[test]
fn export_scopes_keys_and_all_is_an_explicit_opt_in() {
    let fixture = Fixture::with_keys(&[
        ("Demo", "DEMO_SECRET", "fake-dev", "demo", "dev"),
        ("Other", "OTHER_SECRET", "fake-other", "other", "prod"),
    ]);
    let scoped = fixture
        .cli()
        .args(["export", "--project", "demo", "--env", "dev"])
        .output()
        .unwrap();
    assert_success(&scoped);
    assert_eq!(scoped.stdout, b"DEMO_SECRET=fake-dev\n");
    let all = fixture.cli().args(["export", "--all"]).output().unwrap();
    assert_success(&all);
    assert_eq!(
        all.stdout,
        b"DEMO_SECRET=fake-dev\nOTHER_SECRET=fake-other\n"
    );

    let path = fixture.report("all.json");
    let output = fixture
        .cli()
        .env("CLI_TEST_REPORT", &path)
        .args(["run", "--all", "--"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "child_probe", "--ignored", "--nocapture"])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(report(&path)["other_secret"], "fake-other");
}

#[test]
fn imported_reserved_control_names_cannot_be_injected_or_exported() {
    for name in [
        "AKM_PASSWORD",
        "AKM_TOKEN",
        "PATH",
        "PATHEXT",
        "COMSPEC",
        "SYSTEMROOT",
        "WINDIR",
        "NODE_OPTIONS",
    ] {
        let fixture =
            Fixture::with_keys(&[("Unsafe", name, "fake-reserved-secret", "demo", "dev")]);
        let path = fixture.report("must-not-exist.json");
        for verb in ["run", "export"] {
            let mut command = fixture.cli();
            command.env("CLI_TEST_REPORT", &path).args([verb, "--all"]);
            if verb == "run" {
                command.arg("--").arg(std::env::current_exe().unwrap());
            }
            let output = command.output().unwrap();
            assert!(!output.status.success(), "accepted {name} for {verb}");
            assert!(stderr(&output).contains("reserved"), "{}", stderr(&output));
            assert!(!stderr(&output).contains("fake-reserved-secret"));
            assert!(output.stdout.is_empty());
            assert!(!path.exists());
        }
    }
}

#[test]
fn reveals_are_audited_before_any_secret_is_printed() {
    let fixture = Fixture::new();
    let audit = fixture.log();
    let output = fixture
        .cli()
        .args(["get", "Demo", "--field", "secret"])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(output.stdout, b"fake-intended-secret\n");
    assert_eq!(audit.read(1).unwrap()[0].action, "reveal-requested");
    fs::remove_file(audit.path()).unwrap();
    fs::create_dir(audit.path()).unwrap();
    for args in [
        vec!["get", "Demo", "--reveal"],
        vec!["get", "Demo", "--field", "secret"],
        vec!["export", "--all"],
    ] {
        let output = fixture.cli().args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!stderr(&output).contains("fake-intended-secret"));
    }
    let path = fixture.report("must-not-exist.json");
    let output = fixture.probe_command(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(!path.exists());
}

#[test]
fn missing_audit_is_not_reported_as_verified_or_proof_of_inactivity() {
    let fixture = Fixture::new();
    let log = fixture.log();
    assert!(!log.path().exists());
    let output = fixture.cli().args(["audit", "--verify"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        stderr(&output)
    );
    assert!(text.contains("Completeness unknown"), "{text}");
    assert!(!text.contains("chain verified"), "{text}");
}

#[test]
fn unsafe_journal_appends_refuse_disclosure_export_and_launch_without_echo_or_byte_changes() {
    let fixture = Fixture::new();
    let log = fixture.log();
    log.append("add-completed", None, None).unwrap();
    let mut no_newline = fs::read(log.path()).unwrap();
    assert_eq!(no_newline.pop(), Some(b'\n'));
    for bytes in [no_newline, b"{\"seq\":\"FAKE_JOURNAL_MARKER\"}\n".to_vec()] {
        fs::write(log.path(), &bytes).unwrap();
        for args in [
            vec!["get", "Demo", "--reveal"],
            vec!["get", "Demo", "--field", "secret"],
            vec!["export", "--all"],
        ] {
            let output = fixture.cli().args(args).output().unwrap();
            assert!(!output.status.success(), "unsafe append disclosed data");
            assert!(output.stdout.is_empty());
            for private in ["FAKE_JOURNAL_MARKER", "fake-intended-secret"] {
                assert!(!stderr(&output).contains(private));
            }
            assert_eq!(fs::read(log.path()).unwrap(), bytes);
        }
        let path = fixture.report("unsafe-must-not-run.json");
        let output = fixture.probe_command(&path).output().unwrap();
        assert!(!output.status.success());
        assert!(!path.exists());
        assert!(!stderr(&output).contains("FAKE_JOURNAL_MARKER"));
        assert_eq!(fs::read(log.path()).unwrap(), bytes);
    }
}

#[test]
fn skipped_whitespace_boundary_never_bypasses_cli_disclosure_and_launch_logging() {
    let fixture = Fixture::new();
    let log = fixture.log();
    log.append("add-completed", None, None).unwrap();
    let good = fs::read(log.path()).unwrap();
    for prefix in [b"".as_slice(), good.as_slice()] {
        for suffix in [b"\x0c".as_slice(), b"\x0c\n", b" \t\r", b"\x0b\n"] {
            let bytes = [prefix, suffix].concat();
            fs::write(log.path(), &bytes).unwrap();
            for args in [
                vec!["get", "Demo", "--field", "secret"],
                vec!["export", "--all"],
            ] {
                let output = fixture.cli().args(args).output().unwrap();
                assert!(
                    !output.status.success(),
                    "unsafe skipped whitespace permitted disclosure"
                );
                assert!(output.stdout.is_empty());
                assert!(!stderr(&output).contains("fake-intended-secret"));
                assert_eq!(fs::read(log.path()).unwrap(), bytes);
            }
            let path = fixture.report("whitespace-must-not-run.json");
            assert!(!fixture
                .probe_command(&path)
                .output()
                .unwrap()
                .status
                .success());
            assert!(!path.exists());
            assert_eq!(fs::read(log.path()).unwrap(), bytes);
        }
    }
}

#[test]
fn new_exports_emit_requested_events_and_corrupt_audit_displays_known_sequence() {
    let fixture = Fixture::new();
    let log = fixture.log();
    assert_success(&fixture.cli().args(["export", "--all"]).output().unwrap());
    assert!(log
        .read(2)
        .unwrap()
        .iter()
        .any(|entry| entry.seq == 1 && entry.action == "export-requested"));
    let bytes =
        fs::read_to_string(log.path())
            .unwrap()
            .replacen("export-requested", "export-altered", 1);
    fs::write(log.path(), &bytes).unwrap();
    let output = fixture.cli().args(["audit", "--verify"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text.contains("line 1") && text.contains("sequence 1"),
        "{text}"
    );
    assert_eq!(fs::read_to_string(log.path()).unwrap(), bytes);
}

// 1A-R5-06: the release CLI shows a damaged record's declared sequence as unverified.
#[test]
fn audit_shows_a_damaged_records_declared_sequence_without_its_contents() {
    let fixture = Fixture::new();
    let log = fixture.log();
    let bytes = b"{\"seq\":7,\"ts\":\"bad\",\"detail\":\"fake-hidden-detail\"}
";
    fs::write(log.path(), bytes).unwrap();
    let output = fixture.cli().args(["audit", "--verify"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text.contains(
            "physical line 1: a line is not a valid record, declared sequence 7 (not verified)"
        ),
        "{text}"
    );
    assert!(
        !text.contains("fake-hidden-detail") && !stderr(&output).contains("fake-hidden-detail")
    );
    assert_eq!(fs::read(log.path()).unwrap(), bytes);
}

// 1A-R6-01: a completed export exits 0 even when its warning cannot be printed.
#[test]
fn an_unconfirmed_completion_keeps_a_successful_exit_even_without_stderr() {
    use std::io::Read as _;
    let big = "f".repeat(1 << 20);
    for stderr_open in [false, true] {
        let fixture = Fixture::with_keys(&[("Big", "BIG_SECRET", &big, "demo", "dev")]);
        let log = fixture.log();
        let mut child = fixture
            .cli()
            .args(["export", "--all"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr_pipe = child.stderr.take().unwrap();
        let reader = if stderr_open {
            Some(std::thread::spawn(move || {
                let mut text = String::new();
                stderr_pipe.read_to_string(&mut text).unwrap();
                text
            }))
        } else {
            drop(stderr_pipe);
            None
        };
        // The CLI now blocks on the unread stdout pipe, after its request record.
        let deadline = Instant::now() + Duration::from_secs(60);
        while !fs::read_to_string(log.path())
            .unwrap_or_default()
            .contains("export-requested")
        {
            assert!(Instant::now() < deadline, "no request record");
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut journal = fs::OpenOptions::new()
            .append(true)
            .open(log.path())
            .unwrap();
        journal.write_all(b"not-json\n").unwrap();
        drop(journal);
        let damaged = fs::read(log.path()).unwrap();
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).unwrap();
        let status = child.wait().unwrap();
        assert!(output.len() > big.len(), "the whole export was delivered");
        assert_eq!(status.code(), Some(0), "stderr_open={stderr_open}");
        assert_eq!(fs::read(log.path()).unwrap(), damaged);
        if let Some(reader) = reader {
            let text = reader.join().unwrap();
            assert!(text.contains("could not be confirmed"), "{text}");
            assert!(text.contains("Do not repeat the export"), "{text}");
            assert!(!text.contains("ffffffffffffffff"));
        }
    }
}

// 1A-R6-04: an append refused for a damaged journal labels its sequence as declared.
#[test]
fn refused_appends_label_a_damaged_records_sequence_as_declared() {
    let fixture = Fixture::new();
    let log = fixture.log();
    let bytes = b"{\"seq\":7,\"ts\":\"bad\",\"detail\":\"fake-hidden-detail\"}\n";
    fs::write(log.path(), bytes).unwrap();
    let output = fixture.cli().args(["export", "--all"]).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let text = stderr(&output);
    assert!(
        text.contains(
            "physical line 1 (a line is not a valid record), declared sequence 7 (not verified)"
        ),
        "{text}"
    );
    assert!(!text.contains("fake-hidden-detail") && !text.contains("fake-intended-secret"));
    assert_eq!(fs::read(log.path()).unwrap(), bytes);
}

// 1A-R5-04: a known export outcome is recorded and paired with its request.
#[test]
fn exports_pair_requested_and_completed_records_by_operation_without_the_destination() {
    let fixture = Fixture::new();
    let log = fixture.log();
    let target = fixture.directory.path().join("paired-export.env");
    assert_success(&fixture.cli().args(["export", "--all"]).output().unwrap());
    assert_success(
        &fixture
            .cli()
            .args(["export", "--all", "--output"])
            .arg(&target)
            .output()
            .unwrap(),
    );
    assert!(target.exists());
    let mut entries = log.read(10).unwrap();
    entries.sort_by_key(|entry| entry.seq);
    let actions: Vec<&str> = entries.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "export-requested",
            "export-completed",
            "export-requested",
            "export-completed"
        ]
    );
    let operation = |detail: &Option<String>| {
        let detail = detail.as_deref().unwrap();
        let id = detail
            .strip_prefix("operation ")
            .and_then(|rest| rest.split(';').next())
            .unwrap_or_else(|| panic!("no operation ID in {detail:?}"));
        assert_eq!(id.len(), 36, "{detail:?}");
        id.to_string()
    };
    for pair in entries.chunks(2) {
        assert_eq!(operation(&pair[0].detail), operation(&pair[1].detail));
        assert!(pair[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("export completed; 1 keys"));
    }
    assert_ne!(operation(&entries[0].detail), operation(&entries[2].detail));
    let journal = fs::read_to_string(log.path()).unwrap();
    assert!(
        !journal.contains("paired-export"),
        "the destination must not be journaled"
    );
    assert!(!journal.contains("fake-intended-secret"));
}

#[test]
fn a_failed_file_export_records_only_its_request() {
    let fixture = Fixture::new();
    let log = fixture.log();
    let target = fixture.directory.path().join("existing.env");
    fs::write(&target, b"fake-existing-bytes").unwrap();
    let output = fixture
        .cli()
        .args(["export", "--all", "--output"])
        .arg(&target)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(&target).unwrap(), b"fake-existing-bytes");
    let actions: Vec<String> = log
        .read(10)
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    assert!(
        !actions.iter().any(|action| action == "export-completed"),
        "{actions:?}"
    );
}

#[test]
fn audit_displays_the_rows_from_its_consistency_snapshot() {
    let fixture = Fixture::new();
    let log = fixture.log();
    log.append(
        "copy-requested",
        Some("fake-key-id-requested"),
        Some("fake detail requested"),
    )
    .unwrap();
    log.append(
        "add-completed",
        Some("fake-key-id-completed"),
        Some("fake detail completed"),
    )
    .unwrap();
    for args in [vec!["audit"], vec!["audit", "--verify"]] {
        let output = fixture.cli().args(args).output().unwrap();
        assert_success(&output);
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("copy-requested")
                && text.contains("key fake-key")
                && text.contains("fake detail requested"),
            "{text}"
        );
        assert!(
            text.contains("add-completed") && text.contains("fake detail completed"),
            "{text}"
        );
        assert!(
            text.contains("not authenticated") && text.contains("Completeness unknown"),
            "{text}"
        );
    }
}

#[test]
fn corrupt_audit_is_reported_without_repairing_or_echoing_damaged_bytes() {
    let fixture = Fixture::new();
    let log = fixture.log();
    let bytes = b"{\"possibly-sensitive-malformed-data\":";
    fs::write(log.path(), bytes).unwrap();
    let output = fixture.cli().args(["audit", "--verify"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        stderr(&output)
    );
    assert!(
        text.contains("damaged") && text.contains("Completeness unknown"),
        "{text}"
    );
    assert!(
        !text.contains("possibly-sensitive-malformed-data"),
        "{text}"
    );
    assert!(
        !text.contains("sequence "),
        "a malformed record has no known sequence: {text}"
    );
    assert_eq!(fs::read(log.path()).unwrap(), bytes);
}

/// A pipe whose reader is already gone: every write to it fails, deterministically.
fn closed_pipe() -> Stdio {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    Stdio::from(writer)
}

fn status_with(
    mut command: Command,
    stdout_closed: bool,
    stderr_closed: bool,
    stdin: Option<&[u8]>,
) -> std::process::ExitStatus {
    command.stdout(if stdout_closed {
        closed_pipe()
    } else {
        Stdio::null()
    });
    command.stderr(if stderr_closed {
        closed_pipe()
    } else {
        Stdio::null()
    });
    match stdin {
        Some(input) => {
            command.stdin(Stdio::piped());
            let mut child = command.spawn().unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            child.wait().unwrap()
        }
        None => command.status().unwrap(),
    }
}

// 1A-R7-01, the class: after a saved change, closed output pipes never change the exit.
// Every post-commit journal append fails too, so each command also has a warning to print.
#[test]
fn committed_changes_exit_zero_with_closed_output_pipes() {
    const LEGACY_PASSWORD: &str = "correct horse battery staple";
    for (stdout_closed, stderr_closed) in [(true, false), (false, true), (true, true)] {
        let case = format!("stdout_closed={stdout_closed} stderr_closed={stderr_closed}");
        let fixture = Fixture::new();
        fs::create_dir(fixture.log().path()).unwrap();

        let mut add = fixture.cli();
        add.args([
            "add",
            "Added",
            "--project",
            "demo",
            "--env",
            "dev",
            "--secret-stdin",
        ]);
        let add = status_with(
            add,
            stdout_closed,
            stderr_closed,
            Some(b"fake-added-secret\n"),
        );
        assert_eq!(add.code(), Some(0), "add: {case}");
        assert_eq!(stored_secret(&fixture.vault, "Added"), "fake-added-secret");

        let mut rm = fixture.cli();
        rm.args(["rm", "Added", "--yes"]);
        assert_eq!(
            status_with(rm, stdout_closed, stderr_closed, None).code(),
            Some(0),
            "rm: {case}"
        );
        assert!(Vault::open(&fixture.vault, PASSWORD)
            .unwrap()
            .find("Added")
            .is_err());

        let created = fixture.directory.path().join("created.kdbx");
        fs::create_dir(fixture.log_for(&created).path()).unwrap();
        let mut init = cli();
        init.env("AKM_PASSWORD", PASSWORD)
            .arg("--vault")
            .arg(&created)
            .arg("init");
        assert_eq!(
            status_with(init, stdout_closed, stderr_closed, None).code(),
            Some(0),
            "init: {case}"
        );
        assert!(Vault::open(&created, PASSWORD).is_ok());

        let older = fixture.directory.path().join("older.kdbx");
        fs::write(
            &older,
            include_bytes!("../../akm-core/tests/fixtures/kdbx40.kdbx"),
        )
        .unwrap();
        fs::create_dir(fixture.log_for(&older).path()).unwrap();
        let mut upgrade = cli();
        upgrade
            .env("AKM_PASSWORD", LEGACY_PASSWORD)
            .arg("--vault")
            .arg(&older)
            .args(["upgrade", "--yes"]);
        assert_eq!(
            status_with(upgrade, stdout_closed, stderr_closed, None).code(),
            Some(0),
            "upgrade: {case}"
        );
        assert!(!Vault::open(&older, LEGACY_PASSWORD)
            .unwrap()
            .needs_upgrade());
    }
}

// 1A-R7-01, the other side: a failed primary output is an ordinary error, never a panic (101).
#[test]
fn failed_primary_output_is_an_error_not_a_panic() {
    let fixture = Fixture::new();
    for args in [
        vec!["list"],
        vec!["list", "--json"],
        vec!["get", "Demo"],
        vec!["get", "Demo", "--field", "secret"],
        vec!["get", "Demo", "--reveal"],
        vec!["export", "--all"],
        vec!["audit"],
    ] {
        let output = fixture
            .cli()
            .args(&args)
            .stdout(closed_pipe())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).starts_with("error:"),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("fake-intended-secret"));
        let disclosure =
            args.contains(&"secret") || args.contains(&"--reveal") || args[0] == "export";
        assert_eq!(
            stderr(&output).contains("Some output may already have been written"),
            disclosure,
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn committed_mutations_report_audit_warnings_without_false_failure() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.log().path()).unwrap();
    let mut child = fixture
        .cli()
        .args([
            "add",
            "Added",
            "--project",
            "demo",
            "--env",
            "dev",
            "--secret-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fake-added-secret\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_success(&output);
    assert!(stderr(&output).contains("warning:"), "{}", stderr(&output));
    assert!(!stderr(&output).contains("error:"));
    assert_eq!(stored_secret(&fixture.vault, "Added"), "fake-added-secret");

    let output = fixture
        .cli()
        .args(["rm", "Added", "--yes"])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stderr(&output).contains("warning:"), "{}", stderr(&output));
    assert!(Vault::open(&fixture.vault, PASSWORD)
        .unwrap()
        .find("Added")
        .is_err());
}

fn check_protected_original_in_actual_cli(relative: bool) {
    let fixture = Fixture::new();
    let original = fs::read(&fixture.vault).unwrap();
    let empty_path = fixture.report("empty.kdbx");
    write_database(&empty_path, PASSWORD, &[]);
    let empty = fs::read(&empty_path).unwrap();
    for index in 0..5 {
        let name = format!(
            "vault.kdbx.bak.29990101T00000{index}000000000Z_00000000-0000-4000-8000-{:012}.kdbx",
            index + 1
        );
        fs::write(fixture.report(&name), &empty).unwrap();
    }
    fs::create_dir(fixture.report("vault.kdbx.bak")).unwrap();
    let mut command = if relative {
        let mut command = cli();
        command
            .current_dir(fixture.directory.path())
            .env("AKM_PASSWORD", PASSWORD)
            .args(["--vault", "vault.kdbx"]);
        command
    } else {
        fixture.cli()
    };
    let output = command.args(["rm", "Demo", "--yes"]).output().unwrap();
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Moved"));
    assert!(stderr(&output).contains(".bak could not be refreshed"));
    assert!(Vault::open(&fixture.vault, PASSWORD)
        .unwrap()
        .list()
        .is_empty());
    let backups: Vec<_> = fs::read_dir(fixture.directory.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("vault.kdbx.bak.")
        })
        .collect();
    assert_eq!(backups.len(), 5);
    let protected = backups.iter().find(|p| fs::read(p).unwrap() == original);
    assert!(protected.is_some(), "successful CLI deletion must preserve its exact immediate original even when clock names sort later");
    let recovered = cli()
        .env("AKM_PASSWORD", PASSWORD)
        .arg("--vault")
        .arg(protected.unwrap())
        .args(["get", "Demo", "--field", "secret"])
        .output()
        .unwrap();
    assert_success(&recovered);
    assert_eq!(
        String::from_utf8_lossy(&recovered.stdout).trim(),
        "fake-intended-secret"
    );
}

#[test]
fn protected_original_survives_clock_skew_in_actual_cli() {
    check_protected_original_in_actual_cli(false);
}

#[test]
fn protected_original_survives_relative_cli_path() {
    check_protected_original_in_actual_cli(true);
}

#[test]
fn upgrade_requires_explicit_confirmation_before_unlocking() {
    let output = cli().args(["upgrade"]).output().unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("pass --yes"),
        "{}",
        stderr(&output)
    );
    assert!(!stderr(&output).contains("no terminal available"));

    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.vault).unwrap();
    let output = fixture.cli().args(["upgrade", "--yes"]).output().unwrap();
    assert_success(&output);
    assert_eq!(
        output.stdout,
        b"Vault is already KDBX 4.1; no upgrade needed.\n"
    );
    assert_eq!(fs::read(&fixture.vault).unwrap(), bytes);
}

#[test]
fn older_formats_remain_readable_and_only_upgrade_with_explicit_confirmation() {
    const LEGACY_PASSWORD: &str = "correct horse battery staple";
    for (index, bytes) in [
        include_bytes!("../../akm-core/tests/fixtures/kdbx3.kdbx").as_slice(),
        include_bytes!("../../akm-core/tests/fixtures/kdbx40.kdbx").as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let directory = temporary_directory();
        let path = directory.path().join("older.kdbx");
        fs::write(&path, bytes).unwrap();
        let original = Vault::open(&path, LEGACY_PASSWORD)
            .unwrap_or_else(|error| panic!("legacy fixture {index} did not open: {error}"))
            .list();
        let command = || {
            let mut command = cli();
            command
                .env("AKM_PASSWORD", LEGACY_PASSWORD)
                .arg("--vault")
                .arg(&path);
            command
        };
        let output = command().args(["list", "--json"]).output().unwrap();
        assert_success(&output);
        assert_eq!(fs::read(&path).unwrap(), *bytes);

        let output = command()
            .args(["add", "New fake key", "--secret", "fake-upgrade-secret"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("akm upgrade"),
            "{}",
            stderr(&output)
        );
        assert_eq!(fs::read(&path).unwrap(), *bytes);

        let output = command().arg("upgrade").output().unwrap();
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("pass --yes"),
            "{}",
            stderr(&output)
        );
        assert_eq!(fs::read(&path).unwrap(), *bytes);

        if index == 1 {
            fs::create_dir(default_journal(&path).path()).unwrap();
        }
        let output = command().args(["upgrade", "--yes"]).output().unwrap();
        assert_success(&output);
        assert_eq!(output.stdout, b"Upgraded to KDBX 4.1.\n");
        if index == 1 {
            assert!(stderr(&output).contains("warning:"), "{}", stderr(&output));
        }
        let upgraded = Vault::open(&path, LEGACY_PASSWORD).unwrap();
        assert!(!upgraded.needs_upgrade());
        assert_eq!(upgraded.list(), original);
        assert_eq!(
            fs::read(path.with_file_name("older.kdbx.bak")).unwrap(),
            *bytes
        );
    }
}

#[test]
fn file_export_failures_remove_owned_plaintext_and_preserve_existing_files() {
    #[derive(Clone, Copy, Debug)]
    enum Failure {
        PartialWrite,
        Flush,
        Sync,
    }

    struct FailingIo(Failure);
    struct FailingWriter<'a> {
        file: &'a mut fs::File,
        failure: Failure,
        written: usize,
    }
    impl Write for FailingWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if matches!(self.failure, Failure::PartialWrite) {
                if self.written != 0 {
                    return Err(std::io::Error::other("injected partial write failure"));
                }
                let count = self.file.write(&bytes[..bytes.len().min(7)])?;
                self.written += count;
                assert_eq!(self.file.metadata()?.len(), 7);
                return Ok(count);
            }
            self.file.write(bytes)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()?;
            assert_eq!(self.file.metadata()?.len(), 30);
            if matches!(self.failure, Failure::Flush) {
                return Err(std::io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }
    impl export::ExportIo for FailingIo {
        fn write(
            &mut self,
            file: &mut fs::File,
            envs: &[(String, String)],
        ) -> akm_core::Result<()> {
            export::write(
                &mut FailingWriter {
                    file,
                    failure: self.0,
                    written: 0,
                },
                envs,
            )
        }

        fn sync(&mut self, file: &fs::File) -> akm_core::Result<()> {
            assert_eq!(file.metadata()?.len(), 30);
            Err(std::io::Error::other("injected sync failure").into())
        }
    }

    let mut leaked = Vec::new();
    for failure in [Failure::PartialWrite, Failure::Flush, Failure::Sync] {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let existing = directory.path().join("unrelated.env");
        fs::write(&existing, b"unrelated existing file\n").unwrap();
        let result = export::write_new_with(
            &target,
            &[("EXPORT_SECRET".into(), "fake-test-value".into())],
            &mut FailingIo(failure),
        );
        let expected_error = match failure {
            Failure::PartialWrite => "injected partial write failure",
            Failure::Flush => "injected flush failure",
            Failure::Sync => "injected sync failure",
        };
        let error = result.unwrap_err().to_string();
        assert!(error.contains(expected_error), "{failure:?}: {error}");
        if target.exists() || fs::read_dir(directory.path()).unwrap().count() != 1 {
            leaked.push(failure);
        }
        assert_eq!(fs::read(&existing).unwrap(), b"unrelated existing file\n");

        assert!(export::write_new_with(
            &existing,
            &[("EXPORT_SECRET".into(), "fake-test-value".into())],
            &mut FailingIo(failure),
        )
        .is_err());
        assert_eq!(fs::read(&existing).unwrap(), b"unrelated existing file\n");
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "{failure:?}"
        );
    }
    assert!(leaked.is_empty(), "{leaked:?} left owned plaintext files");
}

#[test]
fn file_export_rejects_nonpersistent_acls_before_unlock_and_before_writing() {
    use std::cell::Cell;

    struct NonpersistentIo {
        queried: bool,
        wrote: bool,
    }
    impl export::ExportIo for NonpersistentIo {
        fn filesystem_flags(&mut self, _file: &fs::File) -> akm_core::Result<u32> {
            self.queried = true;
            Ok(0)
        }

        fn write(
            &mut self,
            file: &mut fs::File,
            envs: &[(String, String)],
        ) -> akm_core::Result<()> {
            self.wrote = true;
            export::write(file, envs)
        }
    }

    let directory = temporary_directory();
    let target = directory.path().join("out.env");
    let queried = Cell::new(false);
    let validated = export::destination_with_query(&target, |_| {
        queried.set(true);
        Ok(0)
    });
    let mut io = NonpersistentIo {
        queried: false,
        wrote: false,
    };
    let written = export::write_new_with(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
        &mut io,
    );
    assert!(
        validated.is_err() && written.is_err() && queried.get() && io.queried && !io.wrote,
        "nonpersistent ACLs accepted: validation={validated:?}, write={written:?}, preflight query={}, file query={}, wrote={}",
        queried.get(), io.queried, io.wrote,
    );
    assert!(validated
        .unwrap_err()
        .to_string()
        .contains("persistent ACLs required"));
    assert!(written
        .unwrap_err()
        .to_string()
        .contains("persistent ACLs required"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
#[cfg(windows)]
fn file_export_rejects_an_ignored_private_dacl_before_writing() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, READ_CONTROL,
    };

    struct IgnoredAclIo {
        wrote: bool,
    }
    impl export::ExportIo for IgnoredAclIo {
        fn create_private(&mut self, path: &Path) -> akm_core::Result<fs::File> {
            Ok(fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .access_mode(FILE_GENERIC_WRITE | DELETE | READ_CONTROL)
                .share_mode(FILE_SHARE_DELETE)
                .open(path)?)
        }

        fn write(
            &mut self,
            file: &mut fs::File,
            envs: &[(String, String)],
        ) -> akm_core::Result<()> {
            self.wrote = true;
            export::write(file, envs)
        }
    }

    let directory = temporary_directory();
    let target = directory.path().join("out.env");
    let mut io = IgnoredAclIo { wrote: false };
    let result = export::write_new_with(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
        &mut io,
    );
    assert!(
        result.is_err() && !io.wrote,
        "export accepted an ignored DACL: result={result:?}, wrote={}",
        io.wrote,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("protected owner-only Windows permissions"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn file_export_publication_failure_removes_its_plaintext_temp() {
    struct FailedPublication;
    impl export::ExportIo for FailedPublication {
        fn publish(&mut self, file: &fs::File, _destination: &Path) -> akm_core::Result<()> {
            assert_eq!(file.metadata()?.len(), 30);
            Err(std::io::Error::other("injected publication failure").into())
        }
    }

    let directory = temporary_directory();
    let target = directory.path().join("out.env");
    let result = export::write_new_with(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
        &mut FailedPublication,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("injected publication failure"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn file_export_never_overwrites_a_target_created_after_validation() {
    struct LateTarget;
    impl export::ExportIo for LateTarget {
        fn publish(&mut self, file: &fs::File, destination: &Path) -> akm_core::Result<()> {
            fs::write(destination, b"unrelated raced destination\n")?;
            export::publish_new(file, destination)
        }
    }

    let directory = temporary_directory();
    let target = export::destination(&directory.path().join("out.env")).unwrap();
    assert!(!target.exists());
    assert!(export::write_new_with(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
        &mut LateTarget,
    )
    .is_err());
    assert_eq!(fs::read(&target).unwrap(), b"unrelated raced destination\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
#[cfg(windows)]
fn file_export_cleanup_and_publication_never_claim_a_replaced_temp_name() {
    struct ReplacedTemp {
        renamed: PathBuf,
        replacement: Option<PathBuf>,
        succeed: bool,
    }
    impl export::ExportIo for ReplacedTemp {
        fn publish(&mut self, file: &fs::File, destination: &Path) -> akm_core::Result<()> {
            let temp = fs::read_dir(destination.parent().unwrap())?
                .next()
                .unwrap()?
                .path();
            assert!(temp
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".akm-export-"));
            fs::rename(&temp, &self.renamed)?;
            fs::write(&temp, b"unrelated replacement file\n")?;
            self.replacement = Some(temp);
            if self.succeed {
                export::publish_new(file, destination)
            } else {
                Err(std::io::Error::other("injected publication failure after replacement").into())
            }
        }
    }

    for succeed in [false, true] {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let mut io = ReplacedTemp {
            renamed: directory.path().join("renamed-owned.tmp"),
            replacement: None,
            succeed,
        };
        let result = export::write_new_with(
            &target,
            &[("EXPORT_SECRET".into(), "fake-test-value".into())],
            &mut io,
        );
        assert_eq!(result.is_ok(), succeed, "{result:?}");
        assert_eq!(
            fs::read(io.replacement.unwrap()).unwrap(),
            b"unrelated replacement file\n",
        );
        assert!(!io.renamed.exists());
        if succeed {
            assert_eq!(
                fs::read(&target).unwrap(),
                b"EXPORT_SECRET=fake-test-value\n"
            );
        } else {
            assert!(!target.exists());
        }
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1 + usize::from(succeed)
        );
    }
}

#[test]
fn file_export_public_api_keeps_non_env_names_and_leaves_no_temp() {
    let directory = temporary_directory();
    let target = export::destination(&directory.path().join("keys.txt")).unwrap();
    export::write_new(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
    )
    .unwrap();
    assert_eq!(
        fs::read(&target).unwrap(),
        b"EXPORT_SECRET=fake-test-value\n"
    );
    #[cfg(windows)]
    assert_eq!(file_dacl(&target), "D:P(A;;FA;;;OW)");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn file_export_fails_closed_when_acl_capability_queries_fail() {
    struct UnknownAcl;
    impl export::ExportIo for UnknownAcl {
        fn filesystem_flags(&mut self, _file: &fs::File) -> akm_core::Result<u32> {
            Err(std::io::Error::other("injected volume query failure").into())
        }

        fn write(
            &mut self,
            _file: &mut fs::File,
            _envs: &[(String, String)],
        ) -> akm_core::Result<()> {
            panic!("unknown ACL capabilities must fail before writing");
        }
    }

    let directory = temporary_directory();
    let target = directory.path().join("out.env");
    let result = export::destination_with_query(&target, |_| {
        Err(std::io::Error::other("injected volume query failure").into())
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("injected volume query failure"));
    let result = export::write_new_with(
        &target,
        &[("EXPORT_SECRET".into(), "fake-test-value".into())],
        &mut UnknownAcl,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("injected volume query failure"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn file_export_is_native_utf8_and_never_overwrites_an_existing_file() {
    let fixture = Fixture::with_keys(&[(
        "Unicode",
        "UNICODE_SECRET",
        "fake-caf\u{e9}-\u{1f512}",
        "demo",
        "dev",
    )]);
    let path = fixture.report("keys with spaces.env");
    let output = fixture
        .cli()
        .args(["export", "--project", "demo", "--env", "dev", "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert_success(&output);
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(&path).unwrap(),
        "UNICODE_SECRET='fake-caf\u{e9}-\u{1f512}'\n".as_bytes()
    );
    let output = fixture
        .cli()
        .args(["export", "--all", "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read(&path).unwrap(),
        "UNICODE_SECRET='fake-caf\u{e9}-\u{1f512}'\n".as_bytes()
    );
    #[cfg(windows)]
    assert_eq!(file_dacl(&path), "D:P(A;;FA;;;OW)");

    let unrelated = fixture.report("unrelated.env");
    fs::write(&unrelated, b"unrelated existing file\n").unwrap();
    let output = fixture
        .cli()
        .args(["export", "--all", "--output"])
        .arg(&unrelated)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(unrelated).unwrap(), b"unrelated existing file\n");
}

#[cfg(windows)]
fn file_dacl(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = null_mut();
    unsafe {
        assert_eq!(
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            ),
            0,
        );
        let mut text = null_mut();
        let mut length = 0;
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            &mut length,
        );
        LocalFree(descriptor);
        assert_ne!(converted, 0);
        let text_value =
            String::from_utf16(std::slice::from_raw_parts(text, length as usize - 1)).unwrap();
        LocalFree(text.cast());
        text_value
    }
}

#[cfg(windows)]
#[test]
fn file_export_rejects_unc_device_and_alternate_stream_paths_before_unlocking() {
    for path in [
        r"\\server\share\keys.env",
        r"\\?\C:\keys.env",
        r"\\.\NUL",
        "NUL",
        "CON.txt",
        "keys.env:stream",
        r"C:\keys.env:stream",
    ] {
        let output = cli()
            .args(["export", "--all", "--output", path])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("local file path"),
            "{path}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("password"));
    }
}

#[cfg(windows)]
#[test]
fn npm_cmd_resolves_only_from_path_and_forwards_literal_metacharacters() {
    let fixture = Fixture::new();
    let directory = fixture.directory.path().join("tools with spaces");
    let working = fixture.directory.path().join("project");
    fs::create_dir(&directory).unwrap();
    fs::create_dir(&working).unwrap();
    let shim = directory.join("npm.cmd");
    fs::write(&shim, b"@echo off\r\n@\"%CLI_TEST_PROBE%\" --exact child_probe --ignored --nocapture %*\r\n@exit /b %errorlevel%\r\n").unwrap();
    fs::write(working.join("npm.cmd"), b"@exit /b 93\r\n").unwrap();
    let probe = directory.join("normal probe.exe");
    fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();
    let arguments = [
        "space argument",
        "literal&value",
        "literal|value",
        "literal<value",
        "literal>value",
        "literal^value",
        "(literal)",
        "%CLI_TEST_NORMAL%",
        "!CLI_TEST_NORMAL!",
    ];
    for (index, program) in [PathBuf::from("npm"), PathBuf::from("npm.cmd"), shim]
        .iter()
        .enumerate()
    {
        let path = fixture.report(&format!("batch-{index}.json"));
        let mut command = fixture.cli();
        command
            .current_dir(&working)
            .env("PATH", &directory)
            .env("PATHEXT", ".EXE;.CMD")
            .env("CLI_TEST_PROBE", &probe)
            .env("CLI_TEST_REPORT", &path)
            .env("CLI_TEST_NORMAL", "must-not-expand")
            .env("CLI_TEST_EXIT", "7")
            .args(["run", "--all", "--"])
            .arg(program);
        for argument in arguments {
            command.args(["--skip", argument]);
        }
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(7),
            "{program:?}: {}",
            stderr(&output)
        );
        let child = report(&path);
        assert_eq!(child["arguments"], json!(arguments), "{program:?}");
        assert_eq!(child["secret"], "fake-intended-secret");
        assert_eq!(child["akm_names"], json!([]));
    }
}

#[test]
#[ignore = "subprocess fixture, invoked only by the executable regression tests"]
fn child_probe() {
    let output = std::env::var_os("CLI_TEST_REPORT").expect("child fixture requires a report path");
    let arguments: Vec<String> = std::env::args().collect();
    let forwarded: Vec<&str> = arguments
        .windows(2)
        .filter(|pair| pair[0] == "--skip")
        .map(|pair| pair[1].as_str())
        .collect();
    let akm_names: Vec<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .filter(|name| name.to_ascii_uppercase().starts_with("AKM_"))
        .collect();
    let mut last_action = None;
    if let Some(path) = std::env::var_os("CLI_TEST_AUDIT") {
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(PASSWORD));
        assert!(!text.contains("fake-intended-secret"));
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        last_action = Some(last["action"].as_str().unwrap().to_string());
        if std::env::var_os("CLI_TEST_BLOCK_AUDIT").is_some() {
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
        }
    }
    fs::write(
        output,
        serde_json::to_vec(&json!({
            "akm_names": akm_names,
            "secret": std::env::var("DEMO_SECRET").ok(),
            "other_secret": std::env::var("OTHER_SECRET").ok(),
            "normal": std::env::var("CLI_TEST_NORMAL").ok(),
            "path": std::env::var("PATH").ok(),
            "arguments": forwarded,
            "last_audit_action": last_action,
        }))
        .unwrap(),
    )
    .unwrap();
    if let Some(release) = std::env::var_os("CLI_TEST_RELEASE") {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !Path::new(&release).exists() {
            assert!(
                Instant::now() < deadline,
                "parent did not release child fixture"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let code = std::env::var("CLI_TEST_EXIT")
        .ok()
        .map(|code| code.parse().unwrap())
        .unwrap_or(0);
    std::process::exit(code);
}

/// Unit 1a, 1A-R8-01: the default view shows a fixed mask, the same for every secret.
#[test]
fn the_default_view_and_metadata_fields_print_nothing_derived_from_the_secret() {
    let secrets = [
        "*",
        "fake8chr",
        "fake9char",
        "fake-forty-character-secret-value-000000",
        "fake-ünïcødé-secret-☃",
    ];
    let mut fields: Vec<String> = Vec::new();
    for name in [
        "id",
        "title",
        "project",
        "env",
        "environment",
        "issuer",
        "url",
        "notes",
        "scopes",
        "env_var",
        "env-var",
        "expires",
    ] {
        fields.push(name.into());
        fields.push(name.to_uppercase());
        fields.push(format!("  {name} "));
    }
    let mut views: Vec<Vec<String>> = Vec::new();
    for secret in secrets {
        let fixture = Fixture::with_keys(&[("Demo", "DEMO_SECRET", secret, "demo", "dev")]);
        let id_output = fixture
            .cli()
            .args(["get", "Demo", "--field", "id"])
            .output()
            .unwrap();
        assert_success(&id_output);
        let id = String::from_utf8(id_output.stdout)
            .unwrap()
            .trim()
            .to_owned();
        let mut view = Vec::new();
        let default = fixture.cli().args(["get", "Demo"]).output().unwrap();
        assert_success(&default);
        let text = String::from_utf8(default.stdout)
            .unwrap()
            .replace(&id, "<id>");
        assert!(
            text.lines().any(|line| line == "Secret:      ********"),
            "the mask line for a secret of {} characters:\n{text}",
            secret.chars().count()
        );
        view.push(text);
        for field in &fields {
            let output = fixture
                .cli()
                .args(["get", "Demo", "--field", field])
                .output()
                .unwrap();
            assert_success(&output);
            view.push(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .replace(&id, "<id>"),
            );
        }
        let audit = fixture.log();
        assert!(
            audit
                .read(100)
                .unwrap_or_default()
                .iter()
                .all(|entry| entry.action != "reveal-requested"),
            "a metadata view records no disclosure"
        );
        views.push(view);
    }
    for (index, view) in views.iter().enumerate().skip(1) {
        assert_eq!(view, &views[0], "the output differs for secret {index}");
    }
}

/// Unit 1a, 1A-R8-01: the two disclosures still record first and fail closed.
#[test]
fn the_secret_disclosures_still_fail_closed_with_a_damaged_journal() {
    for args in [
        vec!["get", "Demo", "--reveal"],
        vec!["get", "Demo", "--field", "secret"],
        vec!["get", "Demo", "--field", " SECRET "],
    ] {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.log().path()).unwrap();
        let output = fixture.cli().args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("fake-intended-secret"),
            "{args:?}"
        );
    }
}

/// Unit 1a, 1A-R8-05: the README names exactly the actions the CLI records, in their order.
#[test]
fn the_readme_names_the_recorded_audit_actions_in_their_order() {
    let fixture = Fixture::new();
    let path = fixture.report("names.json");
    let audit = fixture.log();
    let run = fixture.probe_command(&path).output().unwrap();
    assert_success(&run);
    assert_success(
        &fixture
            .cli()
            .args(["export", "--project", "demo", "--env", "dev"])
            .output()
            .unwrap(),
    );
    assert_success(
        &fixture
            .cli()
            .args(["get", "Demo", "--reveal"])
            .output()
            .unwrap(),
    );
    let mut entries = audit.read(100).unwrap();
    entries.sort_by_key(|entry| entry.seq);
    let recorded: Vec<&str> = entries.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(
        recorded,
        [
            "run-requested",
            "export-requested",
            "export-completed",
            "reveal-requested"
        ],
        "the recorded actions and their order"
    );
    let readme =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    let named: std::collections::BTreeSet<&str> = readme
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|word| {
            word.ends_with("-requested") || word.ends_with("-completed") || word.ends_with("-start")
        })
        .collect();
    // The lifecycle actions are recorded by other commands; lifecycle.rs asserts that each is written.
    let lifecycle = [
        "rotate-completed",
        "remove-completed",
        "restore-completed",
        "purge-completed",
    ];
    let expected: std::collections::BTreeSet<&str> =
        recorded.iter().copied().chain(lifecycle).collect();
    assert_eq!(
        named, expected,
        "the README must name exactly the recorded actions"
    );
    // 1A-A1-04: the README states when each record is written, in the order above.
    let line = |action: &str| {
        let name = format!("`{action}`");
        readme
            .lines()
            .find(|line| line.contains(&name))
            .unwrap_or_else(|| panic!("the README names {action}"))
            .to_string()
    };
    for (action, timing) in [
        ("run-requested", "before the child starts"),
        ("reveal-requested", "before `get --reveal`"),
        ("export-requested", "before any export byte is written"),
        ("export-completed", "after the export is written"),
    ] {
        assert!(line(action).contains(timing), "{action}: {timing}");
    }
    let at = |action: &str| readme.find(&format!("`{action}`")).unwrap();
    assert!(
        at("export-requested") < at("export-completed"),
        "the README lists the request before the completion"
    );
}
