//! Unit 3, findings L3 and L6 through the real `akm` binary: a date-only expiry is one
//! instant in every time zone, and day counts round up for display and never hide an overdue
//! key. Every test uses a fake vault and the debug-only `AKM_TEST_CLOCK`.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use keepass::config::{DatabaseVersion, KdfConfig};
use keepass::{Database, DatabaseKey};
use tempfile::TempDir;

const PASSWORD: &str = "fake-cli-time-password";

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
            .prefix("akm-cli-time-")
            .tempdir_in(base)
            .unwrap();
        let vault = directory.path().join("vault.kdbx");
        let mut db = Database::new();
        db.config.version = DatabaseVersion::KDB4(1);
        db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
        db.save(
            &mut fs::File::create(&vault).unwrap(),
            DatabaseKey::new().with_password(PASSWORD),
        )
        .unwrap();
        let root = directory.path().join("journal-root");
        Self {
            _directory: directory,
            vault,
            root,
        }
    }

    fn command(&self, clock: &str, zone: &str) -> Command {
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
            .env("AKM_TEST_CLOCK", clock)
            .env("TZ", zone)
            .stdin(Stdio::null())
            .arg("--vault")
            .arg(&self.vault);
        command
    }

    fn run(&self, clock: &str, zone: &str, args: &[&str]) -> Output {
        self.command(clock, zone).args(args).output().unwrap()
    }

    fn try_add(&self, clock: &str, args: &[&str]) -> Output {
        let mut child = self
            .command(clock, "UTC")
            .arg("add")
            .args(args)
            .arg("--secret-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                b"fake-time-secret
",
            )
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn add(&self, clock: &str, args: &[&str]) {
        let output = self.try_add(clock, args);
        assert!(output.status.success(), "{}", text(&output.stderr));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn row(listing: &str, title: &str) -> String {
    listing
        .lines()
        .find(|line| line.starts_with(title))
        .unwrap_or_else(|| panic!("no row for {title} in {listing}"))
        .to_string()
}

const ZONES: [&str; 5] = [
    "UTC",
    "America/Los_Angeles",
    "Pacific/Kiritimati",
    "Asia/Kolkata",
    "Pacific/Auckland",
];

#[test]
fn a_date_only_expiry_is_the_last_second_of_that_date_in_every_time_zone() {
    let fixture = Fixture::new();
    fixture.add(
        "2026-01-01T00:00:00Z",
        &["Dated", "--env-var", "DATED_KEY", "--expires", "2026-11-10"],
    );
    for zone in ZONES {
        let json = fixture.run("2026-11-10T00:00:00Z", zone, &["list", "--json"]);
        assert!(
            text(&json.stdout).contains("\"expires_at\": \"2026-11-10T23:59:59Z\""),
            "{zone}: {}",
            text(&json.stdout)
        );
        for (now, expired) in [
            ("2026-11-10T00:00:00Z", false),
            ("2026-11-10T12:00:00Z", false),
            ("2026-11-10T23:59:58Z", false),
            ("2026-11-10T23:59:59Z", true),
            ("2026-11-11T00:00:00Z", true),
        ] {
            let output = fixture.run(now, zone, &["list"]);
            let line = row(&text(&output.stdout), "Dated");
            assert!(line.contains("2026-11-10"), "{zone} {now}: {line}");
            assert_eq!(line.contains("EXPIRED"), expired, "{zone} {now}: {line}");
        }
    }
}

#[test]
fn rotation_days_round_up_and_an_overdue_key_is_never_shown_as_due_in_zero_days() {
    let fixture = Fixture::new();
    fixture.add(
        "2026-01-01T00:00:00Z",
        &[
            "Rotating",
            "--env-var",
            "ROTATING_KEY",
            "--rotation-days",
            "30",
        ],
    );
    for (now, listed, detail) in [
        ("2026-01-01T00:00:00Z", "in 30d", "(in 30d)"),
        ("2026-01-30T12:00:00Z", "in 1d", "(in 1d)"),
        ("2026-01-31T00:00:00Z", "due now", "(due now)"),
        ("2026-01-31T12:00:00Z", "overdue", "(overdue)"),
        ("2026-02-01T00:00:00Z", "1d overdue", "(1d overdue)"),
        ("2026-02-02T12:00:00Z", "2d overdue", "(2d overdue)"),
    ] {
        let output = fixture.run(now, "UTC", &["list"]);
        let line = row(&text(&output.stdout), "Rotating");
        assert!(line.trim_end().ends_with(listed), "{now}: {line}");
        assert!(!line.contains("in 0d"), "{now}: {line}");
        let get = fixture.run(now, "UTC", &["get", "Rotating"]);
        let shown = text(&get.stdout);
        assert!(
            shown.contains(&format!("Rotation:    30 days {detail}")),
            "{now}: {shown}"
        );
    }
}

#[test]
fn a_full_instant_expiry_and_a_bad_date_are_handled_by_the_same_rule() {
    let fixture = Fixture::new();
    fixture.add(
        "2026-01-01T00:00:00Z",
        &[
            "Exact",
            "--env-var",
            "EXACT_KEY",
            "--expires",
            "2026-11-10T12:00:00+02:00",
        ],
    );
    let json = fixture.run("2026-01-01T00:00:00Z", "UTC", &["list", "--json"]);
    assert!(text(&json.stdout).contains("\"expires_at\": \"2026-11-10T10:00:00Z\""));
    let bad = fixture.try_add("2026-01-01T00:00:00Z", &["Bad", "--expires", "2026-02-30"]);
    assert!(!bad.status.success());
    assert!(
        text(&bad.stderr).contains("YYYY-MM-DD"),
        "{}",
        text(&bad.stderr)
    );
}

// ---------------------------------------------------------------------- FR-15: the interval

#[test]
fn a_rotation_interval_over_the_limit_is_refused_and_a_stored_one_is_listed_and_read() {
    let fixture = Fixture::new();
    for days in ["36501", "4294967295"] {
        let before = fs::read(&fixture.vault).unwrap();
        let refused = fixture.try_add(
            "2026-01-01T00:00:00Z",
            &[
                "Too long",
                "--env-var",
                "TOO_LONG_KEY",
                "--rotation-days",
                days,
            ],
        );
        assert!(!refused.status.success(), "{days}");
        let message = text(&refused.stderr);
        assert!(message.contains("36500"), "{days}: {message}");
        assert_eq!(fs::read(&fixture.vault).unwrap(), before, "vault unchanged");
    }

    // A vault that another program wrote, with an interval no date can hold.
    use keepass::db::fields;
    let mut db = Database::new();
    db.config.version = DatabaseVersion::KDB4(1);
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "Stored");
        entry.set_protected(fields::PASSWORD, "fake-stored-secret");
        entry.set_unprotected("AKM_ENV_VAR", "STORED_KEY");
        entry.set_unprotected("AKM_ROTATION_DAYS", "4294967295");
        entry.set_unprotected("AKM_SECRET_CHANGED", "2026-01-01T00:00:00Z");
    }
    db.save(
        &mut fs::File::create(&fixture.vault).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let now = "2026-06-01T00:00:00Z";
    let list = fixture.run(now, "UTC", &["list"]);
    assert!(list.status.success(), "{}", text(&list.stderr));
    assert!(row(&text(&list.stdout), "Stored").contains("unknown"));
    let json = fixture.run(now, "UTC", &["list", "--json"]);
    assert!(json.status.success(), "{}", text(&json.stderr));
    let get = fixture.run(now, "UTC", &["get", "Stored"]);
    assert!(get.status.success(), "{}", text(&get.stderr));
    let shown = text(&get.stdout);
    assert!(
        shown.contains("Rotation:    4294967295 days (due date unknown"),
        "{shown}"
    );
    let secret = fixture.run(now, "UTC", &["get", "Stored", "--field", "secret"]);
    assert!(secret.status.success(), "{}", text(&secret.stderr));
    assert_eq!(text(&secret.stdout).trim(), "fake-stored-secret");
}

// ---------------------------------------------------------------- FR-18: whole seconds only

#[test]
fn an_expiry_with_a_fraction_of_a_second_is_refused_by_add_and_changes_nothing() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.vault).unwrap();
    let refused = fixture.try_add(
        "2026-01-01T00:00:00Z",
        &["Fractional", "--expires", "2026-11-10T12:00:00.5Z"],
    );
    assert!(!refused.status.success());
    let message = text(&refused.stderr);
    assert!(message.contains("fraction of a second"), "{message}");
    assert!(message.contains("2026-11-10T12:00:00Z"), "{message}");
    assert_eq!(fs::read(&fixture.vault).unwrap(), before);

    fixture.add(
        "2026-01-01T00:00:00Z",
        &[
            "Whole",
            "--env-var",
            "WHOLE_KEY",
            "--expires",
            "2026-11-10T12:00:00.000Z",
        ],
    );
    let json = fixture.run("2026-01-01T00:00:00Z", "UTC", &["list", "--json"]);
    assert!(text(&json.stdout).contains("\"expires_at\": \"2026-11-10T12:00:00Z\""));
}
