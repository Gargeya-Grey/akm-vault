//! FV-04: `akm edit` through the real binary. It changes the metadata of one key and keeps
//! every field that the command did not name, its id, its secret and its history. Every test
//! uses a fake vault and its own journal root.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use keepass::config::{DatabaseVersion, KdfConfig};
use keepass::db::{fields, Entry};
use keepass::{Database, DatabaseKey};
use tempfile::TempDir;

const PASSWORD: &str = "fake-cli-edit-password";
const SECRET: &str = "fake-gq7v-x2m9-k4w8";
const EXPIRY: &str = "2027-03-04T05:06:07";
const SCOPES: &str = r#"["literal,scope","read"]"#;
const NOTES: &str = "  padded note  ";

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Neither stream holds any part of the secret that is six characters or longer.
fn assert_no_secret(output: &Output) {
    let shown = format!("{}{}", stdout(output), stderr(output));
    for part in SECRET.as_bytes().windows(6) {
        let part = std::str::from_utf8(part).unwrap();
        assert!(!shown.contains(part), "the output holds {part:?}: {shown}");
    }
}

/// A KDBX 4.1 vault with one key, "Gamma", in the shape that AKM writes, plus values that a
/// round trip could change: an expiry with a time of day, a scope with a comma, and a note
/// with spaces at both ends. KDBX cannot store a tag with a comma.
fn write_vault(path: &Path) {
    let mut db = Database::new();
    db.config.version = DatabaseVersion::KDB4(1);
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        let id = entry.id().to_string();
        entry.set_unprotected(fields::TITLE, "Gamma");
        entry.set_unprotected(fields::USERNAME, "fake-user");
        entry.set_protected(fields::PASSWORD, SECRET);
        entry.set_unprotected(fields::URL, "https://example.invalid/gamma");
        entry.set_unprotected(fields::NOTES, NOTES);
        entry.set_unprotected("AKM_ID", id);
        entry.set_unprotected("AKM_ISSUER", "Fake issuer");
        entry.set_unprotected("AKM_PROJECT", "demo");
        entry.set_unprotected("AKM_ENV", "dev");
        entry.set_unprotected("AKM_SCOPES", SCOPES);
        entry.set_unprotected("AKM_ENV_VAR", "GAMMA_KEY");
        entry.set_unprotected("AKM_ROTATION_DAYS", "90");
        entry.set_unprotected("AKM_SECRET_CHANGED", "2026-01-01T00:00:00Z");
        entry.tags = vec!["fake-tag".into(), "plain".into()];
        let written = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        entry.times.creation = Some(written);
        entry.times.last_modification = Some(written);
        entry.times.expires = Some(true);
        entry.times.expiry =
            chrono::NaiveDateTime::parse_from_str(EXPIRY, "%Y-%m-%dT%H:%M:%S").unwrap();
    }
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
}

struct Fixture {
    _directory: TempDir,
    vault: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join("opencode");
        fs::create_dir_all(&base).unwrap();
        let directory = tempfile::Builder::new()
            .prefix("akm-cli-edit-")
            .tempdir_in(base)
            .unwrap();
        let vault = directory.path().join("vault.kdbx");
        write_vault(&vault);
        let root = directory.path().join("journal-root");
        Self {
            _directory: directory,
            vault,
            root,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
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
        let output = command
            .env("AKM_JOURNAL_ROOT", &self.root)
            .env("AKM_PASSWORD", PASSWORD)
            .stdin(Stdio::null())
            .arg("--vault")
            .arg(&self.vault)
            .args(args)
            .output()
            .unwrap();
        assert_no_secret(&output);
        output
    }

    /// The one key, read from the raw file so that the test adds no journal record.
    fn entry(&self) -> Entry {
        let db = Database::parse(
            &fs::read(&self.vault).unwrap(),
            DatabaseKey::new().with_password(PASSWORD),
        )
        .unwrap();
        let entry = db
            .iter_all_entries()
            .find(|entry| entry.get(fields::PASSWORD).is_some())
            .unwrap();
        (*entry).clone()
    }

    fn journal(&self) -> String {
        stdout(&self.run(&["audit", "--limit", "100"]))
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(output),
        stderr(output)
    );
}

/// `after` equals `before` in every field, tag, time and attachment, except the fields
/// `changed`, the modification time and the history that a save adds.
fn assert_only_changed(before: &Entry, after: &Entry, changed: &[&str]) {
    let mut after = after.clone();
    for &field in changed {
        match before.fields.get(field) {
            Some(value) => {
                after.fields.insert(field.into(), value.clone());
            }
            None => {
                after.fields.remove(field);
            }
        }
    }
    after.times.last_modification = before.times.last_modification;
    after.history = before.history.clone();
    assert!(after == *before, "more than {changed:?} changed");
}

#[test]
fn an_edit_of_one_field_keeps_every_other_field_the_id_the_secret_and_its_change_time() {
    let f = Fixture::new();
    let before = f.entry();
    let output = f.run(&["edit", "Gamma", "--project", "fake-other-project"]);
    assert_success(&output);
    let text = stdout(&output);
    assert!(text.contains("Updated \"Gamma\""), "{text}");
    assert!(text.contains("Project:     fake-other-project"), "{text}");
    assert!(text.contains("Secret:      ********"), "{text}");

    let after = f.entry();
    assert_eq!(after.get("AKM_PROJECT"), Some("fake-other-project"));
    assert_only_changed(&before, &after, &["AKM_PROJECT"]);
    assert_eq!(after.id(), before.id());
    assert_eq!(after.get(fields::PASSWORD), Some(SECRET));
    assert_eq!(
        after.get("AKM_SECRET_CHANGED"),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!(after.get("AKM_SCOPES"), Some(SCOPES));
    assert_eq!(after.get(fields::NOTES), Some(NOTES));
    assert_eq!(after.tags, ["fake-tag", "plain"]);
    assert_eq!(
        after.times.expiry.format("%Y-%m-%dT%H:%M:%S").to_string(),
        EXPIRY
    );
    let get = stdout(&f.run(&["get", "Gamma"]));
    assert!(get.contains("History: 1 earlier version"), "{get}");
    assert!(
        get.contains("Secret changed: 2026-01-01 00:00:00 UTC"),
        "{get}"
    );
    assert!(f.journal().contains("update-completed"));
}

#[test]
fn each_option_replaces_its_own_field_and_a_list_replaces_the_stored_list() {
    for (args, field, value) in [
        (
            vec!["--title", "Gamma renamed"],
            fields::TITLE,
            "Gamma renamed",
        ),
        (vec!["--env", "fake-prod"], "AKM_ENV", "fake-prod"),
        (
            vec!["--env-var", "GAMMA_TOKEN"],
            "AKM_ENV_VAR",
            "GAMMA_TOKEN",
        ),
        (vec!["--issuer", "Fake other"], "AKM_ISSUER", "Fake other"),
        (
            vec!["--username", "fake-other"],
            fields::USERNAME,
            "fake-other",
        ),
        (
            vec!["--url", "https://example.invalid/x"],
            fields::URL,
            "https://example.invalid/x",
        ),
        (
            vec!["--notes", "fake new note"],
            fields::NOTES,
            "fake new note",
        ),
        (
            vec!["--rotation-days", "36500"],
            "AKM_ROTATION_DAYS",
            "36500",
        ),
        (
            vec!["--scope", "x,y", "--scope", "z"],
            "AKM_SCOPES",
            r#"["x,y","z"]"#,
        ),
    ] {
        let f = Fixture::new();
        let before = f.entry();
        let mut command = vec!["edit", "Gamma"];
        command.extend(&args);
        let output = f.run(&command);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        let after = f.entry();
        assert_eq!(after.get(field), Some(value), "{args:?}");
        assert_only_changed(&before, &after, &[field]);
    }

    let f = Fixture::new();
    let before = f.entry();
    assert_success(&f.run(&["edit", "Gamma", "--tag", "a b", "--tag", "c"]));
    let mut after = f.entry();
    assert_eq!(after.tags, ["a b", "c"]);
    after.tags = before.tags.clone();
    assert_only_changed(&before, &after, &[]);

    let f = Fixture::new();
    let before = f.entry();
    assert_success(&f.run(&["edit", "Gamma", "--expires", "2028-01-02T03:04:05Z"]));
    let mut after = f.entry();
    assert_eq!(
        after.times.expiry.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "2028-01-02T03:04:05"
    );
    after.times.expiry = before.times.expiry;
    assert_only_changed(&before, &after, &[]);
}

#[test]
fn clear_empties_an_optional_field_and_keeps_the_others() {
    let f = Fixture::new();
    let before = f.entry();
    assert_success(&f.run(&["edit", "Gamma", "--clear", "expires"]));
    let mut after = f.entry();
    assert_eq!(after.times.expires, Some(false));
    let shown = stdout(&f.run(&["get", "Gamma", "--field", "expires"]));
    assert_eq!(shown.trim(), "");
    after.times.expires = before.times.expires;
    assert_only_changed(&before, &after, &[]);

    for (clear, field) in [
        ("project", "AKM_PROJECT"),
        ("notes", fields::NOTES),
        ("rotation-days", "AKM_ROTATION_DAYS"),
    ] {
        let f = Fixture::new();
        let before = f.entry();
        assert_success(&f.run(&["edit", "Gamma", "--clear", clear]));
        let after = f.entry();
        assert_eq!(after.get(field), None, "{clear}");
        assert_only_changed(&before, &after, &[field]);
    }

    let f = Fixture::new();
    let before = f.entry();
    assert_success(&f.run(&["edit", "Gamma", "--clear", "scopes"]));
    let after = f.entry();
    assert_eq!(after.get("AKM_SCOPES"), Some("[]"));
    assert_only_changed(&before, &after, &["AKM_SCOPES"]);
}

#[test]
fn an_edit_that_changes_nothing_saves_nothing_and_says_so() {
    let f = Fixture::new();
    let bytes = fs::read(&f.vault).unwrap();
    let output = f.run(&["edit", "Gamma", "--project", "demo", "--notes", NOTES]);
    assert_success(&output);
    assert!(
        stdout(&output).contains("nothing was saved"),
        "{}",
        stdout(&output)
    );
    assert_eq!(fs::read(&f.vault).unwrap(), bytes);
    assert!(!f.journal().contains("update-completed"));
}

#[test]
fn a_refused_edit_changes_nothing() {
    let f = Fixture::new();
    let bytes = fs::read(&f.vault).unwrap();
    for (args, refusal) in [
        (vec!["edit", "Gamma", "--env-var", "PATH"], "reserved"),
        (
            vec!["edit", "Gamma", "--env-var", "lower_case"],
            "not a valid environment variable name",
        ),
        (vec!["edit", "Gamma", "--expires", "soon"], "expir"),
        (
            vec!["edit", "Gamma", "--rotation-days", "0"],
            "greater than zero",
        ),
        (
            vec!["edit", "Gamma", "--rotation-days", "36501"],
            "at most 36500",
        ),
        (
            vec!["edit", "Gamma", "--project", "x", "--clear", "project"],
            "both sets and clears",
        ),
        (vec!["edit", "Gamma", "--clear", "title"], "invalid value"),
        (vec!["edit", "Gamma"], "required"),
        (
            vec!["edit", "Nope", "--notes", "x"],
            "no key has the id or exact title",
        ),
        (vec!["edit", "Gam", "--notes", "x"], "Candidates"),
        (
            vec!["edit", "Gamma", "--secret", "fake-x"],
            "unexpected argument",
        ),
        (vec!["edit", "Gamma", "--tag", "a,b"], "cannot be stored"),
        (vec!["edit", "Gamma", "--tag", "a;b"], "cannot be stored"),
        (
            vec!["edit", "Gamma", "--tag", " padded"],
            "cannot be stored",
        ),
        (
            vec!["add", "New", "--tags", "a;b", "--secret-stdin"],
            "cannot be stored",
        ),
    ] {
        let output = f.run(&args);
        assert!(!output.status.success(), "{args:?} was accepted");
        assert!(
            stderr(&output)
                .to_lowercase()
                .contains(&refusal.to_lowercase()),
            "{args:?}: {}",
            stderr(&output)
        );
        assert_eq!(fs::read(&f.vault).unwrap(), bytes, "{args:?}");
    }
    assert!(!f.journal().contains("update-completed"));
}
