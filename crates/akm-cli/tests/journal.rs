//! Unit 1b: the private per-device journal, through the real `akm` binary.
//! Every test uses fake vaults and its own temporary journal root.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use keepass::config::KdfConfig;
use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use serde_json::Value;
use tempfile::TempDir;

const PASSWORD: &str = "fake-cli-test-password";
const LEGACY_PASSWORD: &str = "correct horse battery staple";
const TITLE: &str = "FakeTitleMarker";
const PROJECT: &str = "fake-project-marker";
const ENVIRONMENT: &str = "fake-env-marker";
const SECRET: &str = "fake-journal-secret";

fn temporary_directory() -> TempDir {
    let base = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("akm-cli-journal-")
        .tempdir_in(base)
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

fn write_database(path: &Path, password: &str) {
    let mut db = Database::new();
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, TITLE);
        entry.set_protected(fields::PASSWORD, SECRET);
        entry.set_unprotected("AKM_ENV_VAR", "DEMO_SECRET");
        entry.set_unprotected("AKM_PROJECT", PROJECT);
        entry.set_unprotected("AKM_ENV", ENVIRONMENT);
    }
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(password),
    )
    .unwrap();
}

struct Fixture {
    directory: TempDir,
    vault: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = temporary_directory();
        let vault_dir = directory.path().join("synced");
        fs::create_dir(&vault_dir).unwrap();
        let vault = vault_dir.join("vault.kdbx");
        write_database(&vault, PASSWORD);
        let root = directory.path().join("journal-root");
        Self {
            directory,
            vault,
            root,
        }
    }

    fn vault_dir(&self) -> &Path {
        self.vault.parent().unwrap()
    }

    fn command(&self, vault: &Path, password: Option<&str>) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_akm"));
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("AKM_")
            {
                command.env_remove(name);
            }
        }
        command
            .env("AKM_JOURNAL_ROOT", &self.root)
            .stdin(Stdio::null())
            .arg("--vault")
            .arg(vault);
        if let Some(password) = password {
            command.env("AKM_PASSWORD", password);
        }
        command
    }

    fn cli(&self) -> Command {
        self.command(&self.vault, Some(PASSWORD))
    }

    fn reveal(&self) -> Output {
        self.cli()
            .args(["get", TITLE, "--field", "secret"])
            .output()
            .unwrap()
    }

    fn audit(&self) -> Output {
        self.command(&self.vault, None)
            .args(["audit", "--limit", "100"])
            .output()
            .unwrap()
    }

    fn journals(&self) -> Vec<PathBuf> {
        journals_in(&self.root)
    }

    fn journal_text(&self) -> String {
        let journals = self.journals();
        assert_eq!(journals.len(), 1, "{journals:?}");
        fs::read_to_string(&journals[0]).unwrap()
    }

    fn mapping(&self) -> Value {
        serde_json::from_slice(&fs::read(self.root.join("mapping.json")).unwrap()).unwrap()
    }
}

fn journals_in(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            name.starts_with("j-") && name.ends_with(".jsonl")
        })
        .collect();
    found.sort();
    found
}

/// Every file name, and every file's bytes, in a folder that a sync client would upload.
fn assert_no_journal_metadata_in(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        assert!(
            !name.contains("audit") && !name.contains("journal"),
            "a journal file appeared beside the vault: {name}"
        );
        if entry.file_type().unwrap().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            for marker in [TITLE, PROJECT, ENVIRONMENT, "reveal-requested"] {
                assert!(
                    !bytes
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes()),
                    "{name} contains {marker}"
                );
            }
        }
    }
}

fn icacls(path: &Path, args: &[&str]) {
    let output = Command::new("icacls")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "icacls {args:?}: {}{}",
        stdout(&output),
        stderr(&output)
    );
}

struct ResetAcl<'a>(&'a Path);
impl Drop for ResetAcl<'_> {
    fn drop(&mut self) {
        let _ = Command::new("icacls")
            .arg(self.0)
            .args(["/reset", "/T", "/C", "/Q"])
            .output();
    }
}

fn junction(link: &Path, target: &Path) {
    let output = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(output.status.success(), "mklink: {}", stdout(&output));
}

fn remove_junction(link: &Path) {
    let _ = fs::remove_dir(link);
}

#[test]
fn nothing_but_the_vault_and_its_backups_appears_beside_a_synced_vault() {
    let fixture = Fixture::new();
    let outside = fixture.directory.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let steps: Vec<(&str, Vec<String>)> = vec![
        ("list", vec!["list".into()]),
        (
            "reveal",
            vec!["get".into(), TITLE.into(), "--reveal".into()],
        ),
        (
            "add",
            vec![
                "add".into(),
                "SecondFakeKey".into(),
                "--project".into(),
                PROJECT.into(),
                "--env".into(),
                ENVIRONMENT.into(),
                "--secret".into(),
                "fake-second-secret".into(),
            ],
        ),
        (
            "export",
            vec![
                "export".into(),
                "--project".into(),
                PROJECT.into(),
                "--env".into(),
                ENVIRONMENT.into(),
            ],
        ),
        (
            "export to a file",
            vec![
                "export".into(),
                "--all".into(),
                "--output".into(),
                outside.join("out.env").display().to_string(),
            ],
        ),
        (
            "run",
            vec![
                "run".into(),
                "--all".into(),
                "--".into(),
                "cmd".into(),
                "/C".into(),
                "exit".into(),
                "0".into(),
            ],
        ),
        (
            "remove",
            vec!["rm".into(), "SecondFakeKey".into(), "--yes".into()],
        ),
    ];
    for (name, args) in steps {
        let output = fixture.cli().args(&args).output().unwrap();
        assert_success(&output);
        assert_no_journal_metadata_in(fixture.vault_dir());
        assert!(
            name == "list" || !fixture.journals().is_empty(),
            "{name}: no private journal"
        );
    }
    let audit = fixture.audit();
    assert_success(&audit);
    assert_no_journal_metadata_in(fixture.vault_dir());
    let journal = fixture.journal_text();
    for marker in [
        TITLE,
        PROJECT,
        ENVIRONMENT,
        "SecondFakeKey",
        "out.env",
        "outside",
    ] {
        assert!(!journal.contains(marker), "the journal contains {marker}");
    }
    assert!(journal.contains("reveal-requested"));
}

#[test]
fn akm_audit_runs_without_a_password_and_shows_key_ids_only() {
    let fixture = Fixture::new();
    let listed = fixture.cli().args(["list", "--json"]).output().unwrap();
    assert_success(&listed);
    let id = serde_json::from_slice::<Value>(&listed.stdout).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_success(&fixture.reveal());

    let audit = fixture.audit();
    assert_success(&audit);
    let text = stdout(&audit);
    assert!(text.contains("reveal-requested"), "{text}");
    assert!(text.contains(&id[..8]), "{text}");
    assert!(
        text.contains("Recorded chain is consistent, not authenticated."),
        "{text}"
    );
    assert!(text.contains("Completeness unknown"), "{text}");
    assert!(!text.contains(TITLE) && !text.contains(PROJECT), "{text}");
    assert!(stderr(&audit).is_empty(), "{}", stderr(&audit));
}

#[test]
fn a_read_creates_no_folder_file_or_mapping() {
    let fixture = Fixture::new();
    let audit = fixture.audit();
    assert_success(&audit);
    assert!(stdout(&audit).contains("no journal"), "{}", stdout(&audit));
    assert!(!fixture.root.exists(), "a read created the journal folder");
    assert_no_journal_metadata_in(fixture.vault_dir());
}

#[test]
fn a_legacy_sidecar_is_reported_and_left_byte_identical() {
    let fixture = Fixture::new();
    let sidecar = PathBuf::from(format!("{}.audit.jsonl", fixture.vault.display()));
    let legacy = format!(
        "{{\"seq\":1,\"ts\":\"2026-10-01T00:00:00Z\",\"action\":\"copy\",\"key_title\":\"{TITLE}\",\"prev\":\"\",\"hash\":\"x\"}}\n"
    );
    fs::write(&sidecar, &legacy).unwrap();
    let modified = fs::metadata(&sidecar).unwrap().modified().unwrap();
    let lock = PathBuf::from(format!("{}.lock", sidecar.display()));

    assert_success(&fixture.reveal());
    assert_success(
        &fixture
            .cli()
            .args(["add", "AnotherFake", "--secret", "fake-another"])
            .output()
            .unwrap(),
    );
    let audit = fixture.audit();
    assert_success(&audit);

    assert_eq!(fs::read_to_string(&sidecar).unwrap(), legacy);
    assert_eq!(
        fs::metadata(&sidecar).unwrap().modified().unwrap(),
        modified
    );
    assert!(!lock.exists(), "the legacy lock file was created");
    let text = stdout(&audit);
    assert!(text.contains("legacy plaintext journal"), "{text}");
    assert!(text.contains(&sidecar.display().to_string()), "{text}");
    assert!(text.contains(&format!("{} bytes", legacy.len())), "{text}");
    assert!(text.contains("copies that already synced"), "{text}");
    assert!(!fixture.journal_text().contains(TITLE));
}

#[test]
fn older_format_and_read_only_vaults_are_journaled_without_being_written() {
    for (index, bytes) in [
        include_bytes!("../../akm-core/tests/fixtures/kdbx3.kdbx").as_slice(),
        include_bytes!("../../akm-core/tests/fixtures/kdbx40.kdbx").as_slice(),
        include_bytes!("../../akm-core/tests/fixtures/kdbx40.kdbx").as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let mut fixture = Fixture::new();
        fs::remove_file(&fixture.vault).unwrap();
        fixture.vault = fixture.vault_dir().join("older.kdbx");
        fs::write(&fixture.vault, bytes).unwrap();
        let read_only = index == 2;
        if read_only {
            let mut permissions = fs::metadata(&fixture.vault).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&fixture.vault, permissions).unwrap();
        }
        let output = fixture
            .command(&fixture.vault, Some(LEGACY_PASSWORD))
            .args(["export", "--all"])
            .output()
            .unwrap();
        assert_success(&output);
        assert!(!output.stdout.is_empty());
        assert_eq!(fs::read(&fixture.vault).unwrap(), *bytes, "fixture {index}");
        assert_eq!(
            fs::metadata(&fixture.vault)
                .unwrap()
                .permissions()
                .readonly(),
            read_only
        );
        assert_eq!(fixture.journals().len(), 1, "fixture {index}");
        assert!(fixture.journal_text().contains("export-requested"));
        let names: Vec<String> = fs::read_dir(fixture.vault_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["older.kdbx"], "fixture {index}: {names:?}");
        if read_only {
            let mut permissions = fs::metadata(&fixture.vault).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(&fixture.vault, permissions).unwrap();
        }
    }
}

#[test]
fn an_existing_folder_with_inherited_permissions_is_refused_without_a_fallback() {
    let fixture = Fixture::new();
    fs::create_dir(&fixture.root).unwrap();
    let before = fs::read(&fixture.vault).unwrap();

    let reveal = fixture.reveal();
    assert!(!reveal.status.success());
    assert!(reveal.stdout.is_empty(), "a secret was disclosed");
    let error = stderr(&reveal);
    assert!(error.contains("journal"), "{error}");
    assert!(error.contains("permission"), "{error}");
    assert!(!error.contains(SECRET));

    let export = fixture.cli().args(["export", "--all"]).output().unwrap();
    assert!(!export.status.success() && export.stdout.is_empty());

    let run = fixture
        .cli()
        .args(["run", "--all", "--", "cmd", "/C", "exit", "0"])
        .output()
        .unwrap();
    assert!(!run.status.success());

    let audit = fixture.audit();
    assert!(!audit.status.success());
    assert!(stderr(&audit).contains("journal"), "{}", stderr(&audit));

    assert_eq!(fs::read(&fixture.vault).unwrap(), before);
    assert_no_journal_metadata_in(fixture.vault_dir());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
}

#[test]
fn an_extra_ace_for_another_account_fails_closed_before_disclosure_and_warns_after_a_commit() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    assert!(fixture.root.is_dir());
    let _reset = ResetAcl(&fixture.root);
    icacls(&fixture.root, &["/grant", "*S-1-5-32-545:(OI)(CI)(R)"]);
    let before = fs::read(&fixture.vault).unwrap();

    let reveal = fixture.reveal();
    assert!(!reveal.status.success());
    assert!(reveal.stdout.is_empty());
    assert!(
        stderr(&reveal).contains("permission"),
        "{}",
        stderr(&reveal)
    );

    let add = fixture
        .cli()
        .args(["add", "SavedAnyway", "--secret", "fake-saved-anyway"])
        .output()
        .unwrap();
    assert_success(&add);
    let error = stderr(&add);
    assert!(error.contains("warning:"), "{error}");
    assert!(
        error.contains("Do not repeat the saved operation"),
        "{error}"
    );
    assert_ne!(
        fs::read(&fixture.vault).unwrap(),
        before,
        "the add committed"
    );
    assert_no_journal_metadata_in(fixture.vault_dir());
}

#[test]
fn a_file_with_an_extra_ace_inside_the_folder_is_refused() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    let _reset = ResetAcl(&fixture.root);
    let journal = fixture.journals().remove(0);
    icacls(&journal, &["/grant", "*S-1-1-0:(R)"]);
    let reveal = fixture.reveal();
    assert!(!reveal.status.success() && reveal.stdout.is_empty());
}

#[test]
fn a_root_that_is_or_sits_under_a_junction_is_refused() {
    let fixture = Fixture::new();
    let real = fixture.directory.path().join("real-root");
    fs::create_dir(&real).unwrap();
    let link = fixture.directory.path().join("linked-root");
    junction(&link, &real);

    let mut direct = fixture.cli();
    direct.env("AKM_JOURNAL_ROOT", &link);
    direct.args(["get", TITLE, "--field", "secret"]);
    let output = direct.output().unwrap();
    assert!(!output.status.success() && output.stdout.is_empty());
    assert!(stderr(&output).contains("link"), "{}", stderr(&output));

    let mut nested = fixture.cli();
    nested.env("AKM_JOURNAL_ROOT", link.join("below"));
    nested.args(["get", TITLE, "--field", "secret"]);
    let output = nested.output().unwrap();
    assert!(!output.status.success() && output.stdout.is_empty());

    assert_eq!(
        fs::read_dir(&real).unwrap().count(),
        0,
        "the target was written"
    );
    remove_junction(&link);
    assert_no_journal_metadata_in(fixture.vault_dir());
}

#[test]
fn an_unusable_root_never_falls_back_to_the_default_or_the_sidecar() {
    let fixture = Fixture::new();
    let file = fixture.directory.path().join("not-a-folder");
    fs::write(&file, b"x").unwrap();
    for root in [
        file.join("below"),
        PathBuf::from("relative-root"),
        PathBuf::new(),
    ] {
        let mut command = fixture.cli();
        command.env("AKM_JOURNAL_ROOT", &root);
        command.args(["get", TITLE, "--field", "secret"]);
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{root:?}");
        assert!(output.stdout.is_empty(), "{root:?}");
        assert!(
            stderr(&output).contains("journal"),
            "{root:?}: {}",
            stderr(&output)
        );
    }
    assert_no_journal_metadata_in(fixture.vault_dir());
}

#[test]
fn damage_is_reported_by_kind_keeps_the_bytes_and_refuses_appends() {
    let fixture = Fixture::new();
    for _ in 0..3 {
        assert_success(&fixture.reveal());
    }
    let journal = fixture.journals().remove(0);
    let good = fs::read(&journal).unwrap();
    let lines: Vec<&[u8]> = good.split_inclusive(|byte| *byte == b'\n').collect();
    assert_eq!(lines.len(), 3);

    let mut altered = good.clone();
    let position = altered
        .windows(14)
        .position(|window| window == b"reveal-request")
        .unwrap();
    altered[position] = b'R';
    let dropped_first = [lines[1], lines[2]].concat();
    let cases: Vec<(&str, Vec<u8>, &str, &str)> = vec![
        (
            "truncated tail",
            good[..good.len() - 7].to_vec(),
            "no line end",
            "3",
        ),
        (
            "malformed line",
            [good.as_slice(), b"not-json\n"].concat(),
            "not a valid record",
            "4",
        ),
        ("broken link", dropped_first, "does not follow", "1"),
        ("altered prefix", altered, "does not match its hash", "1"),
    ];
    for (name, bytes, wording, line) in cases {
        fs::write(&journal, &bytes).unwrap();
        let audit = fixture.audit();
        assert_eq!(audit.status.code(), Some(2), "{name}: {}", stdout(&audit));
        let text = stdout(&audit);
        assert!(text.contains(wording), "{name}: {text}");
        assert!(
            text.contains(&format!("physical line {line}")),
            "{name}: {text}"
        );
        assert!(
            text.contains("Original bytes were preserved"),
            "{name}: {text}"
        );
        assert!(
            !text.contains("tamper") && !text.contains("attack") && !text.contains("crash"),
            "{name}: {text}"
        );

        let reveal = fixture.reveal();
        assert!(
            !reveal.status.success() && reveal.stdout.is_empty(),
            "{name}"
        );
        assert_eq!(fs::read(&journal).unwrap(), bytes, "{name}: bytes changed");
    }
    fs::write(&journal, &good).unwrap();
    assert_success(&fixture.reveal());
}

#[test]
fn an_altered_early_record_hides_every_later_record_from_the_history() {
    let fixture = Fixture::new();
    for _ in 0..4 {
        assert_success(&fixture.reveal());
    }
    let journal = fixture.journals().remove(0);
    let text = fs::read_to_string(&journal).unwrap();
    let altered = text.replacen("reveal-requested", "reveal-requestes", 1);
    fs::write(&journal, &altered).unwrap();
    let audit = fixture.audit();
    let shown = stdout(&audit);
    assert_eq!(audit.status.code(), Some(2), "{shown}");
    assert!(
        !shown.contains("#2") && !shown.contains("#3") && !shown.contains("#4"),
        "{shown}"
    );
}

#[test]
fn a_renamed_vault_keeps_its_journal_and_a_copy_never_shares_it() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    assert_success(&fixture.reveal());

    let renamed = fixture.vault_dir().join("renamed.kdbx");
    fs::rename(&fixture.vault, &renamed).unwrap();
    let audit = fixture
        .command(&renamed, None)
        .args(["audit"])
        .output()
        .unwrap();
    assert_success(&audit);
    assert!(stdout(&audit).contains("#2"), "{}", stdout(&audit));
    let reveal = fixture
        .command(&renamed, Some(PASSWORD))
        .args(["get", TITLE, "--field", "secret"])
        .output()
        .unwrap();
    assert_success(&reveal);
    assert_eq!(fixture.journals().len(), 1);
    assert_eq!(fixture.mapping()["journals"].as_array().unwrap().len(), 1);

    let copy = fixture.vault_dir().join("copy.kdbx");
    fs::copy(&renamed, &copy).unwrap();
    let copy_audit = fixture.command(&copy, None).arg("audit").output().unwrap();
    assert_success(&copy_audit);
    assert!(
        !stdout(&copy_audit).contains("#1"),
        "{}",
        stdout(&copy_audit)
    );
    assert!(
        stdout(&copy_audit).contains("no journal"),
        "{}",
        stdout(&copy_audit)
    );
    let copy_reveal = fixture
        .command(&copy, Some(PASSWORD))
        .args(["get", TITLE, "--field", "secret"])
        .output()
        .unwrap();
    assert_success(&copy_reveal);
    assert_eq!(fixture.journals().len(), 2);
    let copy_audit = fixture.command(&copy, None).arg("audit").output().unwrap();
    assert!(stdout(&copy_audit).contains("#1"));
    assert!(
        !stdout(&copy_audit).contains("#2"),
        "{}",
        stdout(&copy_audit)
    );
    assert!(
        stdout(&copy_audit).contains("not merged"),
        "{}",
        stdout(&copy_audit)
    );
    let original = fixture
        .command(&renamed, None)
        .arg("audit")
        .output()
        .unwrap();
    assert!(stdout(&original).contains("#3"), "{}", stdout(&original));
    assert!(!stdout(&original).contains("#4"), "{}", stdout(&original));
}

#[test]
fn a_different_vault_at_the_same_path_starts_a_new_journal() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    assert_success(&fixture.reveal());
    write_database(&fixture.vault, PASSWORD);

    let locked = fixture.audit();
    assert_success(&locked);
    assert!(
        stdout(&locked).contains("cannot be confirmed"),
        "{}",
        stdout(&locked)
    );

    assert_success(&fixture.reveal());
    assert_eq!(fixture.journals().len(), 2);
    let audit = fixture.audit();
    let text = stdout(&audit);
    assert!(text.contains("#1") && !text.contains("#2"), "{text}");
    assert!(text.contains("earlier journal"), "{text}");
    let mapping = fixture.mapping();
    let states: Vec<&str> = mapping["journals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|journal| journal["state"].as_str().unwrap())
        .collect();
    assert_eq!(
        states.iter().filter(|state| **state == "detached").count(),
        1
    );
}

#[test]
fn restoring_an_earlier_copy_is_recorded_in_the_same_journal() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    let earlier = fs::read(&fixture.vault).unwrap();
    assert_success(
        &fixture
            .cli()
            .args(["add", "LaterFake", "--secret", "fake-later"])
            .output()
            .unwrap(),
    );
    fs::write(&fixture.vault, &earlier).unwrap();
    assert_success(&fixture.reveal());
    assert_eq!(fixture.journals().len(), 1);
    let text = fixture.journal_text();
    let actions: Vec<String> = text
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap()["action"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        actions,
        [
            "reveal-requested",
            "add-completed",
            "vault-rolled-back",
            "reveal-requested"
        ]
    );
}

#[test]
fn concurrent_processes_on_different_vaults_keep_every_binding() {
    let fixture = Fixture::new();
    let vaults: Vec<PathBuf> = (0..6)
        .map(|index| {
            let path = fixture.vault_dir().join(format!("many-{index}.kdbx"));
            write_database(&path, PASSWORD);
            path
        })
        .collect();
    let children: Vec<_> = vaults
        .iter()
        .map(|vault| {
            let mut command = fixture.command(vault, Some(PASSWORD));
            command
                .args(["get", TITLE, "--field", "secret"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command.spawn().unwrap()
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert_success(&output);
    }
    assert_eq!(fixture.journals().len(), 6);
    assert_eq!(fixture.mapping()["journals"].as_array().unwrap().len(), 6);
    for vault in &vaults {
        let audit = fixture.command(vault, None).arg("audit").output().unwrap();
        assert!(stdout(&audit).contains("#1"), "{}", stdout(&audit));
        assert!(!stdout(&audit).contains("#2"), "{}", stdout(&audit));
    }
}

#[test]
fn a_damaged_mapping_is_kept_and_fails_closed() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    let mapping = fixture.root.join("mapping.json");
    let mut file = fs::OpenOptions::new().append(true).open(&mapping).unwrap();
    file.write_all(b"{ not json").unwrap();
    drop(file);
    let bytes = fs::read(&mapping).unwrap();
    let reveal = fixture.reveal();
    assert!(!reveal.status.success() && reveal.stdout.is_empty());
    assert!(stderr(&reveal).contains("mapping"), "{}", stderr(&reveal));
    assert_eq!(fs::read(&mapping).unwrap(), bytes);
}

// Review 1, finding 4: a reader that stays open across a same-path replacement must not
// detach or rebind the journal of the vault that is there now, also across processes.

fn active_ids(mapping: &Value) -> Vec<String> {
    mapping["journals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|journal| journal["state"] == "active")
        .map(|journal| journal["id"].as_str().unwrap().to_string())
        .collect()
}

fn stale_disclosure(vault: &akm_core::Vault) -> akm_core::Result<()> {
    let key = vault.list().remove(0);
    vault
        .disclose_key(akm_core::KeyDisclosure::Reveal, &key, None)
        .map(|_| ())
}

#[test]
fn a_stale_reader_cannot_detach_the_journal_that_the_cli_keeps_for_the_current_vault() {
    let fixture = Fixture::new();
    let stale = akm_core::Vault::open_in(
        &fixture.vault,
        PASSWORD,
        akm_core::journal::JournalRoot::at(&fixture.root),
    )
    .unwrap();
    stale_disclosure(&stale).unwrap();

    write_database(&fixture.vault, PASSWORD);
    assert_success(&fixture.reveal());
    let current = active_ids(&fixture.mapping());
    assert_eq!(current.len(), 1);

    for round in 0..3 {
        let error = stale_disclosure(&stale).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no longer the one this session opened"),
            "round {round}: {error}"
        );
        assert_success(&fixture.reveal());
    }

    let mapping = fixture.mapping();
    assert_eq!(
        mapping["journals"].as_array().unwrap().len(),
        2,
        "{mapping}"
    );
    assert_eq!(
        active_ids(&mapping),
        current,
        "the current binding is the same"
    );
    assert_eq!(fixture.journals().len(), 2);

    let audit = fixture.audit();
    assert_success(&audit);
    let text = stdout(&audit);
    assert!(text.contains("#4") && !text.contains("#5"), "{text}");
    assert!(text.contains("earlier journal"), "{text}");
    assert!(!text.contains("cannot be confirmed"), "{text}");
}

#[test]
fn a_stale_reader_that_the_cli_replaced_twice_never_merges_the_histories() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    let first = akm_core::Vault::open_in(
        &fixture.vault,
        PASSWORD,
        akm_core::journal::JournalRoot::at(&fixture.root),
    )
    .unwrap();
    write_database(&fixture.vault, PASSWORD);
    assert_success(&fixture.reveal());
    let second = akm_core::Vault::open_in(
        &fixture.vault,
        PASSWORD,
        akm_core::journal::JournalRoot::at(&fixture.root),
    )
    .unwrap();
    write_database(&fixture.vault, PASSWORD);
    assert_success(&fixture.reveal());
    let current = active_ids(&fixture.mapping());
    assert_eq!(current.len(), 1);

    for _ in 0..2 {
        assert!(stale_disclosure(&first).is_err());
        assert!(stale_disclosure(&second).is_err());
        assert_success(&fixture.reveal());
    }

    let mapping = fixture.mapping();
    assert_eq!(
        mapping["journals"].as_array().unwrap().len(),
        3,
        "{mapping}"
    );
    assert_eq!(active_ids(&mapping), current);
    let counts: Vec<usize> = fixture
        .journals()
        .iter()
        .map(|journal| fs::read_to_string(journal).unwrap().lines().count())
        .collect();
    let mut sorted = counts.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, [1, 1, 3], "{counts:?}");
}

#[test]
fn a_failed_disclosure_keeps_the_rollback_indication_for_the_next_process() {
    let fixture = Fixture::new();
    assert_success(&fixture.reveal());
    let earlier = fs::read(&fixture.vault).unwrap();
    assert_success(
        &fixture
            .cli()
            .args(["add", "LaterFake", "--secret", "fake-later"])
            .output()
            .unwrap(),
    );
    fs::write(&fixture.vault, &earlier).unwrap();

    let journal = fixture.journals().remove(0);
    let good = fs::read(&journal).unwrap();
    let mut damaged = good.clone();
    damaged.extend_from_slice(b"{\"seq\":3,\"ts\":\"torn");
    fs::write(&journal, &damaged).unwrap();
    let mapping = fs::read_to_string(fixture.root.join("mapping.json")).unwrap();

    let refused = fixture.reveal();
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(!stdout(&refused).contains(SECRET), "{}", stdout(&refused));
    assert_eq!(fs::read(&journal).unwrap(), damaged, "damaged bytes stay");
    assert_eq!(
        fs::read_to_string(fixture.root.join("mapping.json")).unwrap(),
        mapping,
        "the mapping did not advance"
    );

    fs::write(&journal, &good).unwrap();
    assert_success(&fixture.reveal());
    assert_success(&fixture.reveal());
    let actions: Vec<String> = fixture
        .journal_text()
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap()["action"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        actions,
        [
            "reveal-requested",
            "add-completed",
            "vault-rolled-back",
            "reveal-requested",
            "reveal-requested"
        ]
    );
}

// ----------------------- review 2, finding 4: clones share a root and never share a journal
//
// The same rule through the real binary, one process per command. A version that this device
// saw belongs to the journal that recorded it, wherever it shows up (unit 1b, D12).

fn plain(path: &Path) -> String {
    let real = fs::canonicalize(path).unwrap();
    real.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

/// The actions of the journal that the mapping binds to `vault`'s path.
fn bound_actions(root: &Path, vault: &Path) -> Vec<String> {
    let mapping: Value =
        serde_json::from_slice(&fs::read(root.join("mapping.json")).unwrap()).unwrap();
    let wanted = plain(vault);
    let bound: Vec<&Value> = mapping["journals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|binding| {
            binding["state"] == "active"
                && binding["path"].as_str().unwrap().to_lowercase() == wanted
        })
        .collect();
    assert_eq!(bound.len(), 1, "{wanted}: {mapping}");
    let file = root.join(format!("j-{}.jsonl", bound[0]["id"].as_str().unwrap()));
    actions_of(&file)
}

fn actions_of(file: &Path) -> Vec<String> {
    fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap()["action"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

impl Fixture {
    fn at(&self, name: &str) -> PathBuf {
        self.vault_dir().join(name)
    }

    fn reveal_at(&self, vault: &Path) -> Output {
        self.command(vault, Some(PASSWORD))
            .args(["get", TITLE, "--field", "secret"])
            .output()
            .unwrap()
    }

    fn add_at(&self, vault: &Path, title: &str) -> Output {
        self.command(vault, Some(PASSWORD))
            .args(["add", title, "--secret", "fake-clone-secret"])
            .output()
            .unwrap()
    }

    fn audit_at(&self, vault: &Path) -> Output {
        self.command(vault, None)
            .args(["audit", "--limit", "100"])
            .output()
            .unwrap()
    }
}

#[test]
fn a_clone_that_replaces_its_original_keeps_every_record_in_its_own_journal_across_processes() {
    let fixture = Fixture::new();
    let a = fixture.vault.clone();
    let b = fixture.at("b.kdbx");
    assert_success(&fixture.reveal_at(&a));
    let original_bytes = fs::read(&a).unwrap();
    fs::copy(&a, &b).unwrap();
    assert_success(&fixture.add_at(&b, "CloneKey"));
    assert_success(&fixture.reveal_at(&b));
    assert_eq!(
        bound_actions(&fixture.root, &a),
        strings(&["reveal-requested"])
    );
    assert_eq!(
        bound_actions(&fixture.root, &b),
        strings(&["add-completed", "reveal-requested"])
    );
    let original_journal = journals_in(&fixture.root)
        .into_iter()
        .find(|file| actions_of(file) == strings(&["reveal-requested"]))
        .unwrap();
    let original_before = fs::read(&original_journal).unwrap();

    fs::copy(&b, &a).unwrap();
    assert_success(&fixture.reveal_at(&a));
    assert_success(&fixture.add_at(&a, "ReplacementKey"));
    assert_success(&fixture.reveal_at(&a));
    assert_success(&fixture.reveal_at(&b));

    assert!(
        fs::read(&original_journal).unwrap() == original_before,
        "the original's journal is byte-identical"
    );
    assert_eq!(
        bound_actions(&fixture.root, &b),
        strings(&[
            "add-completed",
            "reveal-requested",
            "reveal-requested",
            "add-completed",
            "reveal-requested",
            "reveal-requested",
        ]),
        "every record of the replacement and of the clone is in the clone's journal"
    );
    assert_eq!(fixture.journals().len(), 2);
    let mapping = fixture.mapping();
    assert_eq!(
        active_ids(&mapping).len(),
        2,
        "no binding was detached: {mapping}"
    );

    let audit = fixture.audit_at(&a);
    assert_success(&audit);
    let text = stdout(&audit);
    assert!(text.contains("#6") && !text.contains("#7"), "{text}");
    assert!(
        text.contains(
            &original_journal
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        ),
        "the notice names the original's journal file: {text}"
    );

    fs::write(&a, &original_bytes).unwrap();
    assert_success(&fixture.reveal_at(&a));
    assert_eq!(
        bound_actions(&fixture.root, &a),
        strings(&["reveal-requested", "reveal-requested"]),
        "putting the original back returns to the original's journal, with no rollback record"
    );
    assert_eq!(
        bound_actions(&fixture.root, &b).len(),
        6,
        "the clone's journal is unchanged"
    );
}

#[test]
fn a_rollback_indication_lands_in_the_journal_of_the_instance_that_went_back_across_processes() {
    let fixture = Fixture::new();
    let a = fixture.vault.clone();
    let b = fixture.at("b.kdbx");
    assert_success(&fixture.reveal_at(&a));
    fs::copy(&a, &b).unwrap();
    assert_success(&fixture.add_at(&b, "CloneKey"));
    let b_earlier = fs::read(&b).unwrap();
    assert_success(&fixture.add_at(&b, "CloneKeyTwo"));

    fs::write(&a, &b_earlier).unwrap();
    assert_success(&fixture.reveal_at(&a));
    assert_success(&fixture.reveal_at(&b));
    assert_eq!(
        bound_actions(&fixture.root, &b),
        strings(&[
            "add-completed",
            "add-completed",
            "reveal-requested",
            "reveal-requested"
        ]),
        "the clone's old version at the original's path is no rollback of the clone"
    );

    fs::write(&b, &b_earlier).unwrap();
    assert_success(&fixture.reveal_at(&b));
    assert_eq!(
        bound_actions(&fixture.root, &b),
        strings(&[
            "add-completed",
            "add-completed",
            "reveal-requested",
            "reveal-requested",
            "vault-rolled-back",
            "reveal-requested",
        ])
    );
    assert_eq!(
        bound_actions(&fixture.root, &a),
        strings(&["reveal-requested"]),
        "the original's journal has no rollback record from the clone"
    );
}

#[test]
fn stale_and_current_processes_alternate_without_crossing_journals() {
    let fixture = Fixture::new();
    let a = fixture.vault.clone();
    let b = fixture.at("b.kdbx");
    let stale = akm_core::Vault::open_in(
        &a,
        PASSWORD,
        akm_core::journal::JournalRoot::at(&fixture.root),
    )
    .unwrap();
    stale_disclosure(&stale).unwrap();
    fs::copy(&a, &b).unwrap();
    assert_success(&fixture.add_at(&b, "CloneKey"));
    fs::copy(&b, &a).unwrap();

    for round in 0..3 {
        stale_disclosure(&stale).unwrap_or_else(|error| panic!("round {round}: {error}"));
        assert_success(&fixture.reveal_at(&a));
        assert_success(&fixture.reveal_at(&b));
        assert_success(&fixture.add_at(&a, &format!("Round{round}")));
    }

    assert_eq!(
        bound_actions(&fixture.root, &b)
            .iter()
            .filter(|action| *action == "reveal-requested")
            .count(),
        6,
        "two current disclosures in each of three rounds"
    );
    assert_eq!(
        bound_actions(&fixture.root, &a),
        strings(&[
            "reveal-requested",
            "reveal-requested",
            "reveal-requested",
            "reveal-requested"
        ]),
        "the stale session's own four disclosures, and nothing else"
    );
    assert_eq!(fixture.journals().len(), 2);
    assert_eq!(active_ids(&fixture.mapping()).len(), 2);
}

#[test]
fn a_version_that_two_journals_recorded_fails_closed_across_processes() {
    let fixture = Fixture::new();
    let a = fixture.vault.clone();
    let c = fixture.at("c.kdbx");
    let d = fixture.at("d.kdbx");
    let e = fixture.at("e.kdbx");
    assert_success(&fixture.reveal_at(&a));
    let shared = fs::read(&a).unwrap();
    fs::copy(&a, &c).unwrap();
    assert_success(&fixture.reveal_at(&c));
    fs::copy(&a, &d).unwrap();
    assert_success(&fixture.add_at(&d, "DeltaKey"));
    fs::write(&d, &shared).unwrap();

    let mapping = fs::read_to_string(fixture.root.join("mapping.json")).unwrap();
    let before: Vec<Vec<u8>> = fixture
        .journals()
        .iter()
        .map(|file| fs::read(file).unwrap())
        .collect();
    let refused = fixture.reveal_at(&d);
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(!stdout(&refused).contains(SECRET), "{}", stdout(&refused));
    assert!(
        stderr(&refused).contains("more than one journal"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("mapping.json")).unwrap(),
        mapping,
        "the mapping is as it was"
    );
    let after: Vec<Vec<u8>> = fixture
        .journals()
        .iter()
        .map(|file| fs::read(file).unwrap())
        .collect();
    assert_eq!(before, after, "no journal changed");

    let audit = fixture.audit_at(&d);
    assert_success(&audit);
    let text = stdout(&audit);
    assert!(text.contains("more than one journal"), "{text}");
    assert!(text.contains("No records displayed"), "{text}");

    fs::copy(&a, &e).unwrap();
    assert_success(&fixture.reveal_at(&e));
    assert_eq!(
        bound_actions(&fixture.root, &e),
        strings(&["reveal-requested"])
    );
}
