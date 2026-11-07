//! Unit 2 through the real `akm` binary: rotate, history, the Recycle Bin, restore and
//! purge. Every test uses a fake vault, its own journal root and, where a time matters, the
//! debug-only `AKM_TEST_CLOCK`, so the expected times are literal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use keepass::config::{DatabaseVersion, KdfConfig};
use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use tempfile::TempDir;

const PASSWORD: &str = "fake-cli-test-password";
const OLD_SECRET: &str = "fake-old-alpha-value";
const NEW_SECRET: &str = "fake-new-alpha-value";

fn temporary_directory() -> TempDir {
    let base = std::env::temp_dir().join("opencode");
    fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("akm-cli-lifecycle-")
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
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(output),
        stderr(output)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        stdout(output)
    );
}

/// A KDBX 4.1 vault with fast KDF settings. The keys are "legacy": they have no
/// `AKM_SECRET_CHANGED` field, as every key that an older AKM wrote.
fn write_vault(path: &Path, bin_enabled: Option<bool>) {
    let mut db = Database::new();
    db.config.version = DatabaseVersion::KDB4(1);
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    db.meta.recyclebin_enabled = bin_enabled;
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        for (title, var, secret) in [
            ("Alpha", "ALPHA_KEY", OLD_SECRET),
            ("Beta", "BETA_KEY", "fake-beta-value"),
        ] {
            let mut entry = group.add_entry();
            entry.set_unprotected(fields::TITLE, title);
            entry.set_protected(fields::PASSWORD, secret);
            entry.set_unprotected("AKM_ENV_VAR", var);
            entry.set_unprotected("AKM_PROJECT", "demo");
            entry.set_unprotected("AKM_ENV", "dev");
            entry.set_unprotected("AKM_ROTATION_DAYS", "30");
            let written = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            entry.times.creation = Some(written);
            entry.times.last_modification = Some(written);
            entry.times.last_access = Some(written);
            entry.times.location_changed = Some(written);
        }
    }
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
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
        Self::with_bin(None)
    }

    fn with_bin(bin_enabled: Option<bool>) -> Self {
        let directory = temporary_directory();
        let vault_dir = directory.path().join("synced");
        fs::create_dir(&vault_dir).unwrap();
        let vault = vault_dir.join("vault.kdbx");
        write_vault(&vault, bin_enabled);
        let root = directory.path().join("journal-root");
        Self {
            directory,
            vault,
            root,
        }
    }

    fn command(&self, clock: Option<&str>) -> Command {
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
            .env("AKM_PASSWORD", PASSWORD)
            .stdin(Stdio::null())
            .arg("--vault")
            .arg(&self.vault);
        if let Some(clock) = clock {
            command.env("AKM_TEST_CLOCK", clock);
        }
        command
    }

    fn run(&self, clock: &str, args: &[&str]) -> Output {
        self.command(Some(clock)).args(args).output().unwrap()
    }

    fn run_with_input(&self, clock: &str, args: &[&str], input: &str) -> Output {
        let mut child = self
            .command(Some(clock))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn secret(&self, query: &str) -> String {
        let output = self
            .command(None)
            .args(["get", query, "--field", "secret"])
            .output()
            .unwrap();
        assert_success(&output);
        stdout(&output).trim_end().to_string()
    }

    fn raw(&self) -> Database {
        Database::parse(
            &fs::read(&self.vault).unwrap(),
            DatabaseKey::new().with_password(PASSWORD),
        )
        .unwrap()
    }
}

const T0: &str = "2026-05-01T10:00:00Z";

#[test]
fn rotate_replaces_the_secret_and_prints_literal_times_but_never_a_secret() {
    let f = Fixture::new();
    let output = f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{NEW_SECRET}\n"),
    );
    assert_success(&output);
    let text = stdout(&output);
    assert!(text.contains("Rotated \"Alpha\""), "{text}");
    assert!(
        text.contains("Secret changed:          2026-05-01 10:00:00 UTC"),
        "{text}"
    );
    assert!(text.contains("Previous secret change:  unknown"), "{text}");
    assert!(
        text.contains("old secret stays in the vault history"),
        "{text}"
    );
    for streamed in [text.as_str(), &stderr(&output)] {
        assert!(!streamed.contains(OLD_SECRET) && !streamed.contains(NEW_SECRET));
    }
    assert_eq!(f.secret("Alpha"), NEW_SECRET);

    let get = f.run(T0, &["get", "Alpha"]);
    let shown = stdout(&get);
    assert!(
        shown.contains("Secret changed: 2026-05-01 10:00:00 UTC"),
        "{shown}"
    );
    assert!(shown.contains("History: 1 earlier version"), "{shown}");
    assert!(shown.contains("Rotation:    30 days (in 30d)"), "{shown}");

    let later = f.run_with_input(
        "2026-05-11T10:00:00Z",
        &["rotate", "Alpha", "--secret-stdin"],
        "fake-third-value\n",
    );
    assert_success(&later);
    assert!(
        stdout(&later).contains("Previous secret change:  2026-05-01 10:00:00 UTC"),
        "{}",
        stdout(&later)
    );
}

#[test]
fn rotate_refuses_an_unchanged_value_and_does_not_touch_the_vault() {
    let f = Fixture::new();
    let before = fs::read(&f.vault).unwrap();
    let output = f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{OLD_SECRET}\n"),
    );
    assert_failure(&output);
    assert!(
        stderr(&output).contains("same as the current secret"),
        "{}",
        stderr(&output)
    );
    assert!(!stderr(&output).contains(OLD_SECRET));
    assert_eq!(fs::read(&f.vault).unwrap(), before);

    let empty = f.run_with_input(T0, &["rotate", "Alpha", "--secret-stdin"], "\n");
    assert_failure(&empty);
    assert_eq!(fs::read(&f.vault).unwrap(), before);
}

#[test]
fn rotate_takes_no_secret_as_an_argument() {
    let f = Fixture::new();
    let output = f.run(T0, &["rotate", "Alpha", "--secret", NEW_SECRET]);
    assert_failure(&output);
    assert!(
        stderr(&output).contains("unexpected argument"),
        "{}",
        stderr(&output)
    );
    assert_eq!(f.secret("Alpha"), OLD_SECRET);
}

#[test]
fn a_legacy_key_shows_unknown_never_a_date_in_list_get_and_json() {
    let f = Fixture::new();
    let list = f.run(T0, &["list"]);
    assert_success(&list);
    let table = stdout(&list);
    let alpha = table
        .lines()
        .find(|line| line.starts_with("Alpha"))
        .unwrap();
    assert!(alpha.trim_end().ends_with("unknown"), "{alpha}");

    let get = stdout(&f.run(T0, &["get", "Alpha"]));
    assert!(get.contains("Secret changed: unknown"), "{get}");
    assert!(get.contains("History: none"), "{get}");

    let json = stdout(&f.run(T0, &["list", "--json"]));
    let keys: serde_json::Value = serde_json::from_str(&json).unwrap();
    let alpha = keys
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["title"] == "Alpha")
        .unwrap();
    assert_eq!(alpha["secret_changed_at"], serde_json::Value::Null);
    assert_eq!(alpha["secret_change"], "unknown");
    assert_eq!(alpha["history_count"], 0);
    assert_eq!(alpha["secret"], "***");
}

#[test]
fn history_names_the_changed_fields_with_literal_times_and_no_values() {
    let f = Fixture::new();
    assert_success(&f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{NEW_SECRET}\n"),
    ));
    let output = f.run(T0, &["history", "Alpha"]);
    assert_success(&output);
    let text = stdout(&output);
    assert!(text.contains("1 earlier version"), "{text}");
    let row = text
        .lines()
        .find(|line| line.contains("Password"))
        .expect(&text);
    assert!(
        row.contains("AKM_SECRET_CHANGED") && row.contains("secret changed"),
        "{row}"
    );
    assert!(!text.contains(OLD_SECRET) && !text.contains(NEW_SECRET));

    let none = stdout(&f.run(T0, &["history", "Beta"]));
    assert!(none.contains("no earlier versions"), "{none}");
}

#[test]
fn rm_moves_the_key_to_the_recycle_bin_and_restore_brings_back_the_same_key() {
    let f = Fixture::new();
    let id = {
        let list = stdout(&f.run(T0, &["list", "--json"]));
        let keys: serde_json::Value = serde_json::from_str(&list).unwrap();
        keys.as_array()
            .unwrap()
            .iter()
            .find(|key| key["title"] == "Alpha")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let removed = f.run("2026-05-02T08:30:00Z", &["rm", "Alpha", "--yes"]);
    assert_success(&removed);
    let text = stdout(&removed);
    assert!(
        text.contains("Moved \"Alpha\" to the Recycle Bin"),
        "{text}"
    );
    assert!(
        text.contains(&format!("akm restore {id}")) && text.contains(&format!("akm purge {id}")),
        "{text}"
    );
    assert!(!stdout(&f.run(T0, &["list"])).contains("Alpha"));
    assert_failure(&f.run(T0, &["get", "Alpha"]));

    let bin = stdout(&f.run(T0, &["bin"]));
    let row = bin.lines().find(|line| line.contains("Alpha")).expect(&bin);
    assert!(
        row.contains(&id) && row.contains("2026-05-02 08:30:00 UTC"),
        "{row}"
    );
    assert!(!bin.contains(OLD_SECRET));

    let restored = f.run("2026-05-03T00:00:00Z", &["restore", "Alpha"]);
    assert_success(&restored);
    assert!(
        stdout(&restored).contains(&format!("Restored \"Alpha\" ({id})")),
        "{}",
        stdout(&restored)
    );
    assert_eq!(f.secret("Alpha"), OLD_SECRET);
    assert!(stdout(&f.run(T0, &["bin"])).contains("The Recycle Bin is empty"));
}

#[test]
fn purge_asks_for_yes_deletes_for_good_and_leaves_a_deletion_record() {
    let f = Fixture::new();
    assert_success(&f.run(T0, &["rm", "Alpha", "--yes"]));
    let before = fs::read(&f.vault).unwrap();
    let refused = f.run(T0, &["purge", "Alpha"]);
    assert_failure(&refused);
    assert!(
        stderr(&refused).contains("pass --yes"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(fs::read(&f.vault).unwrap(), before);

    let purged = f.run("2026-05-09T00:00:00Z", &["purge", "Alpha", "--yes"]);
    assert_success(&purged);
    assert!(
        stdout(&purged).contains("Deleted \"Alpha\" for good"),
        "{}",
        stdout(&purged)
    );
    assert!(stdout(&f.run(T0, &["bin"])).contains("The Recycle Bin is empty"));
    assert_failure(&f.run(T0, &["restore", "Alpha"]));
    let raw = f.raw();
    assert_eq!(raw.iter_all_entries().count(), 1, "only Beta is left");
    assert_eq!(raw.deleted_objects.len(), 1);
}

#[test]
fn purge_does_not_delete_a_live_key_unless_asked_with_live() {
    let f = Fixture::new();
    let refused = f.run(T0, &["purge", "Beta", "--yes"]);
    assert_failure(&refused);
    assert!(f.run(T0, &["get", "Beta"]).status.success());
    let live = f.run(T0, &["purge", "Beta", "--yes", "--live"]);
    assert_success(&live);
    assert_failure(&f.run(T0, &["get", "Beta"]));
}

#[test]
fn retention_purge_removes_only_old_recycled_keys_and_nothing_purges_by_itself() {
    let f = Fixture::new();
    assert_success(&f.run("2026-01-01T00:00:00Z", &["rm", "Alpha", "--yes"]));
    assert_success(&f.run("2026-02-20T00:00:00Z", &["rm", "Beta", "--yes"]));
    for clock in ["2027-01-01T00:00:00Z", "2028-01-01T00:00:00Z"] {
        assert_success(&f.run(clock, &["list"]));
        assert_success(&f.run(clock, &["audit"]));
    }
    let bin = stdout(&f.run("2028-01-01T00:00:00Z", &["bin"]));
    assert!(
        bin.contains("Alpha") && bin.contains("Beta"),
        "a year passed and nothing purged: {bin}"
    );

    let nothing = f.run(
        "2026-02-25T00:00:00Z",
        &["purge", "--older-than-days", "400", "--yes"],
    );
    assert_success(&nothing);
    assert!(
        stdout(&nothing).contains("Nothing to purge"),
        "{}",
        stdout(&nothing)
    );

    let purged = f.run(
        "2026-02-25T00:00:00Z",
        &["purge", "--older-than-days", "30", "--yes"],
    );
    assert_success(&purged);
    assert!(
        stdout(&purged).contains("Deleted 1 key for good"),
        "{}",
        stdout(&purged)
    );
    let bin = stdout(&f.run(T0, &["bin"]));
    assert!(!bin.contains("Alpha") && bin.contains("Beta"), "{bin}");
}

#[test]
fn purge_needs_exactly_one_of_a_key_or_a_retention_age() {
    let f = Fixture::new();
    assert_failure(&f.run(T0, &["purge", "--yes"]));
    assert_failure(&f.run(T0, &["purge", "Alpha", "--older-than-days", "3", "--yes"]));
}

#[test]
fn a_vault_with_the_recycle_bin_turned_off_refuses_rm_and_changes_nothing() {
    let f = Fixture::with_bin(Some(false));
    let before = fs::read(&f.vault).unwrap();
    let output = f.run(T0, &["rm", "Alpha", "--yes"]);
    assert_failure(&output);
    assert!(
        stderr(&output).contains("Recycle Bin is turned off"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read(&f.vault).unwrap(), before);
    assert_success(&f.run(T0, &["purge", "Alpha", "--yes", "--live"]));
}

#[test]
fn the_journal_records_ids_and_actions_for_rotate_restore_and_purge_without_titles_or_secrets() {
    let f = Fixture::new();
    assert_success(&f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{NEW_SECRET}\n"),
    ));
    assert_success(&f.run(T0, &["rm", "Alpha", "--yes"]));
    assert_success(&f.run(T0, &["restore", "Alpha"]));
    assert_success(&f.run(T0, &["rm", "Alpha", "--yes"]));
    assert_success(&f.run(T0, &["purge", "Alpha", "--yes"]));
    let audit = stdout(
        &f.command(None)
            .args(["audit", "--limit", "50"])
            .output()
            .unwrap(),
    );
    for action in [
        "rotate-completed",
        "remove-completed",
        "restore-completed",
        "purge-completed",
    ] {
        assert!(audit.contains(action), "{action}: {audit}");
    }
    assert!(audit.contains("deleted for good"), "{audit}");
    for forbidden in ["Alpha", "demo", NEW_SECRET, OLD_SECRET] {
        assert!(!audit.contains(forbidden), "{forbidden} in {audit}");
    }
}

#[test]
fn a_journal_failure_after_a_rotation_is_a_warning_and_the_exit_stays_zero() {
    let f = Fixture::new();
    // A first record creates the journal; then damage it so that the next append fails.
    assert_success(&f.run(T0, &["get", "Beta", "--reveal"]));
    let journal = fs::read_dir(&f.root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("j-")
                && path.extension().is_some_and(|ext| ext == "jsonl")
        })
        .unwrap();
    fs::write(&journal, b"damaged journal\n").unwrap();
    let output = f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{NEW_SECRET}\n"),
    );
    assert_success(&output);
    let warning = stderr(&output);
    assert!(
        warning.contains("warning:") && warning.contains("Do not repeat"),
        "{warning}"
    );
    assert!(!warning.contains("error:"));
    let raw = f.raw();
    let alpha = raw
        .iter_all_entries()
        .find(|entry| entry.get(fields::TITLE) == Some("Alpha"))
        .unwrap();
    assert_eq!(alpha.get(fields::PASSWORD), Some(NEW_SECRET));
}

#[test]
fn rotate_and_rm_write_nothing_but_the_vault_and_its_backups_beside_it() {
    let f = Fixture::new();
    assert_success(&f.run_with_input(
        T0,
        &["rotate", "Alpha", "--secret-stdin"],
        &format!("{NEW_SECRET}\n"),
    ));
    assert_success(&f.run(T0, &["rm", "Alpha", "--yes"]));
    let names: Vec<String> = fs::read_dir(f.vault.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|name| name.starts_with("vault.kdbx")),
        "{names:?}"
    );
    let _ = &f.directory;
}
