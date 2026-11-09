//! Unit 3, findings L3 and L6: one instant for a date-only expiry, and day counts that keep
//! display rounding apart from scheduling. The table in `fixtures/time/cases.json` is read by
//! the frontend tests too.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use akm_core::schedule::{parse_expiry, Remaining};
use akm_core::{ApiKeyInput, Clock, Vault};
use chrono::{DateTime, Utc};
use serde_json::Value;

const PASSWORD: &str = "correct horse battery staple";

fn table() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/time/cases.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

#[derive(Clone)]
struct ManualClock(Arc<Mutex<DateTime<Utc>>>);

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[test]
fn a_date_only_expiry_is_the_last_second_of_that_utc_date() {
    for case in table()["date_only_expiry"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        assert_eq!(
            parse_expiry(input).unwrap(),
            utc(case["instant"].as_str().unwrap()),
            "{input}"
        );
    }
}

#[test]
fn a_full_instant_is_kept_exactly_whatever_its_offset() {
    for case in table()["full_instant_expiry"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        assert_eq!(
            parse_expiry(input).unwrap(),
            utc(case["instant"].as_str().unwrap()),
            "{input}"
        );
    }
}

#[test]
fn text_that_is_not_a_date_is_refused_with_the_accepted_form() {
    for input in table()["refused_expiry"].as_array().unwrap() {
        let input = input.as_str().unwrap();
        let message = parse_expiry(input).unwrap_err().to_string();
        assert!(message.contains("YYYY-MM-DD"), "{input}: {message}");
    }
}

#[test]
fn the_remaining_time_rounds_up_for_display_and_never_hides_an_overdue_key() {
    for case in table()["remaining"].as_array().unwrap() {
        let due = utc(case["due"].as_str().unwrap());
        let now = utc(case["now"].as_str().unwrap());
        let days = case["days"].as_i64().unwrap();
        let expected = match case["state"].as_str().unwrap() {
            "in" => Remaining::In { days },
            "now" => Remaining::Now,
            "overdue" => Remaining::Overdue { days },
            other => panic!("unknown state {other}"),
        };
        assert_eq!(Remaining::of(due, now), expected, "due {due} now {now}");
        assert_eq!(expected.is_past(), case["state"] != "in");
    }
}

fn vault_at(now: &str) -> (tempfile::TempDir, Vault, Arc<Mutex<DateTime<Utc>>>) {
    let directory = tempfile::tempdir().unwrap();
    let time = Arc::new(Mutex::new(utc(now)));
    let vault = Vault::create(directory.path().join("vault.kdbx"), PASSWORD, None)
        .unwrap()
        .with_clock(Arc::new(ManualClock(time.clone())));
    (directory, vault, time)
}

#[test]
fn the_expired_state_flips_at_the_exact_last_second_of_the_date() {
    let (_directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let mut input = ApiKeyInput::new("Dated", "fake-secret");
    input.expires_at = Some(parse_expiry("2026-11-10").unwrap());
    let key = vault.add(input).unwrap();
    for case in table()["expired_at"].as_array().unwrap() {
        assert_eq!(key.expires_at, Some(utc(case["expires"].as_str().unwrap())));
        let now = utc(case["now"].as_str().unwrap());
        assert_eq!(key.is_expired(now), case["expired"], "{now}");
        assert_eq!(
            key.expiry_remaining(now).unwrap().is_past(),
            case["expired"].as_bool().unwrap(),
            "{now}"
        );
    }
}

#[test]
fn the_rotation_due_time_is_exact_and_its_day_count_is_only_for_display() {
    let (_directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let mut input = ApiKeyInput::new("Rotating", "fake-secret");
    input.rotation_days = Some(30);
    let key = vault.add(input).unwrap();
    assert_eq!(key.rotation_due_at(), Some(utc("2026-01-31T00:00:00Z")));
    for (now, expected) in [
        ("2026-01-01T00:00:00Z", Remaining::In { days: 30 }),
        ("2026-01-30T12:00:00Z", Remaining::In { days: 1 }),
        ("2026-01-31T00:00:00Z", Remaining::Now),
        ("2026-01-31T12:00:00Z", Remaining::Overdue { days: 0 }),
        ("2026-02-01T00:00:00Z", Remaining::Overdue { days: 1 }),
    ] {
        assert_eq!(key.rotation_remaining(utc(now)), Some(expected), "{now}");
    }
}

#[test]
fn an_unknown_secret_change_time_gives_no_due_time_and_no_state() {
    let (_directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let key = vault.add(ApiKeyInput::new("No interval", "fake")).unwrap();
    assert_eq!(key.rotation_due_at(), None);
    assert_eq!(key.rotation_remaining(utc("2027-01-01T00:00:00Z")), None);
    assert_eq!(key.expiry_remaining(utc("2027-01-01T00:00:00Z")), None);
}

// ---------------------------------------------------------------------- FR-15: the interval

/// A vault file written by a program other than AKM, with one key whose stored rotation
/// interval is `days` and whose secret changed on 2026-01-01.
fn vault_with_stored_interval(days: &str) -> (tempfile::TempDir, Vault) {
    use akm_core::model::{META_ROTATION_DAYS, META_SECRET_CHANGED};
    use keepass::db::fields;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = keepass::Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "API Keys".into();
    let mut entry = group.add_entry();
    entry.set_unprotected(fields::TITLE, "Stored interval");
    entry.set_protected(fields::PASSWORD, "fake-stored-secret");
    entry.set_unprotected(META_ROTATION_DAYS, days);
    entry.set_unprotected(META_SECRET_CHANGED, "2026-01-01T00:00:00Z");
    db.save(
        &mut std::fs::File::create(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let vault = Vault::open(&path, PASSWORD).unwrap();
    (directory, vault)
}

/// FR-15: input bounds the interval at 100 years. A value over the bound is refused on add
/// and on edit with a message that names the bound, and the vault does not change.
#[test]
fn a_rotation_interval_over_the_limit_is_refused_by_add_and_edit() {
    let (_directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let limit = akm_core::model::MAX_ROTATION_DAYS;
    assert_eq!(limit, 36_500);
    for days in [limit + 1, 100_000, u32::MAX] {
        let mut input = ApiKeyInput::new("Too long", "fake-secret");
        input.rotation_days = Some(days);
        let text = vault.add(input).unwrap_err().to_string();
        assert!(text.contains("36500"), "{days}: {text}");
        assert!(!text.contains("fake-secret"));
    }
    assert!(vault.list().is_empty(), "nothing was stored");

    let mut at_limit = ApiKeyInput::new("At the limit", "fake-secret");
    at_limit.rotation_days = Some(limit);
    let key = vault.add(at_limit).unwrap();
    assert_eq!(key.rotation_due_at(), Some(utc("2125-12-08T00:00:00Z")));

    let before = vault.get_by_id(&key.id).unwrap();
    let mut edit = vault.edit_input(&key.id).unwrap();
    edit.rotation_days = Some(u32::MAX);
    let text = vault.update(&key.id, edit).unwrap_err().to_string();
    assert!(text.contains("36500"), "{text}");
    assert_eq!(
        vault.get_by_id(&key.id).unwrap().rotation_days,
        before.rotation_days
    );
}

/// FR-15: a stored interval is not trusted. A value no date can hold gives no due date, and
/// nothing panics: listing, reading, the due time and the remaining time all answer.
#[test]
fn a_stored_out_of_range_rotation_interval_gives_no_due_date_and_never_panics() {
    use akm_core::schedule::rotation_due;
    assert_eq!(rotation_due(utc("2026-01-01T00:00:00Z"), u32::MAX), None);
    assert_eq!(
        rotation_due(utc("2026-01-01T00:00:00Z"), 36_500),
        Some(utc("2125-12-08T00:00:00Z"))
    );
    assert_eq!(
        rotation_due(DateTime::<Utc>::MAX_UTC, 1),
        None,
        "the end of the range is not wrapped"
    );

    for days in ["4294967295", "4000000000", "99999999"] {
        let (_directory, vault) = vault_with_stored_interval(days);
        let key = vault.list().remove(0);
        assert_eq!(key.rotation_days, Some(days.parse().unwrap()));
        assert_eq!(key.rotation_due_at(), None, "{days}");
        assert_eq!(key.rotation_remaining(utc("2026-06-01T00:00:00Z")), None);
        let read = vault.get_by_id(&key.id).unwrap();
        assert_eq!(read.title, "Stored interval");
    }
    // A stored interval inside the range still counts.
    let (_directory, vault) = vault_with_stored_interval("36500");
    assert_eq!(
        vault.list().remove(0).rotation_due_at(),
        Some(utc("2125-12-08T00:00:00Z"))
    );
}
