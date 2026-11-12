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

// ---------------------------------------------------------------- FR-18: whole seconds only

/// FR-18: an instant with a fraction of a second is refused with a message that says why and
/// shows the accepted form. A zero fraction and every whole-second form still parse.
#[test]
fn an_expiry_with_a_fraction_of_a_second_is_refused_and_a_zero_fraction_is_not() {
    for input in [
        "2026-11-10T12:00:00.5Z",
        "2026-11-10T12:00:00.123Z",
        "2026-11-10T12:00:00.000001Z",
        "2026-11-10T12:00:00.999999999+02:00",
    ] {
        let message = parse_expiry(input)
            .expect_err(&format!("{input} was accepted"))
            .to_string();
        assert!(
            message.contains("fraction of a second"),
            "{input}: {message}"
        );
    }
    let message = parse_expiry("2026-11-10T12:00:00.5Z")
        .unwrap_err()
        .to_string();
    assert!(message.contains("fraction of a second"), "{message}");
    assert!(message.contains("2026-11-10T12:00:00Z"), "{message}");

    for (input, instant) in [
        ("2026-11-10T12:00:00.000Z", "2026-11-10T12:00:00Z"),
        (
            "2026-11-10T12:00:00.000000000+02:00",
            "2026-11-10T10:00:00Z",
        ),
        ("2026-11-10T12:00:00Z", "2026-11-10T12:00:00Z"),
        ("2026-11-10", "2026-11-10T23:59:59Z"),
    ] {
        assert_eq!(parse_expiry(input).unwrap(), utc(instant), "{input}");
    }
}

// ------------------------------------------- core review 1, finding 6: extreme day counts

/// Finding 6 (the FR-15 class): every day count that a caller passes gives a controlled
/// result. A count beyond the calendar lists every key with an expiry, or none; a purge cutoff
/// beyond the calendar purges nothing. No value panics, and nothing is written.
#[test]
fn extreme_day_counts_for_expiry_and_purge_never_panic_and_change_nothing() {
    let (directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let mut dated = ApiKeyInput::new("Dated", "fake-secret");
    dated.expires_at = Some(parse_expiry("2026-11-10").unwrap());
    vault.add(dated).unwrap();
    vault
        .add(ApiKeyInput::new("Undated", "fake-other"))
        .unwrap();
    let recycled = vault
        .add(ApiKeyInput::new("Recycled", "fake-recycled"))
        .unwrap();
    vault.remove(&recycled.id).unwrap();
    let path = directory.path().join("vault.kdbx");
    let before = std::fs::read(&path).unwrap();
    for days in [i64::MAX, i64::from(u32::MAX), 106_751_991_167_301, 1 << 40] {
        let titles: Vec<String> = vault.expiring(days).into_iter().map(|k| k.title).collect();
        assert_eq!(titles, ["Dated"], "{days}");
    }
    for days in [i64::MIN, -i64::from(u32::MAX), -(1 << 40)] {
        assert!(vault.expiring(days).is_empty(), "{days}");
    }
    for days in [u32::MAX, 100_000_000] {
        assert!(
            vault.purge_recycled_older_than(days).unwrap().is_empty(),
            "{days}"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), before, "nothing was written");
    assert_eq!(vault.recycled().len(), 1);
}

/// Finding 6, the structural guard: only `schedule.rs` computes with dates. Its functions give
/// a checked or a saturated result, and `#![deny(clippy::arithmetic_side_effects)]` refuses
/// an overflowing operator there. This scan fails when the source of the core, the CLI or the
/// desktop back end names a date-arithmetic constructor or method anywhere else. The limit:
/// it cannot see the subtraction of two dates with no named method after it.
#[test]
fn date_arithmetic_lives_only_in_the_schedule_module() {
    const NAMES: &[&str] = &[
        "TimeDelta",
        "Days",
        "Months",
        "checked_add_signed",
        "checked_sub_signed",
        "signed_duration_since",
        "checked_add_days",
        "checked_sub_days",
        "checked_add_months",
        "checked_sub_months",
        "num_weeks",
        "num_days",
        "num_hours",
        "num_minutes",
        "num_seconds",
        "num_milliseconds",
        "num_microseconds",
        "num_nanoseconds",
    ];
    const CALLS: &[&str] = &[
        "timestamp",
        "timestamp_millis",
        "timestamp_micros",
        "timestamp_nanos_opt",
        "and_local_timezone",
    ];
    const DURATION: &[&str] = &[
        "weeks",
        "days",
        "hours",
        "minutes",
        "seconds",
        "milliseconds",
        "microseconds",
        "nanoseconds",
        "try_weeks",
        "try_days",
        "try_hours",
        "try_minutes",
        "try_seconds",
        "try_milliseconds",
        "new",
        "zero",
        "max_value",
        "min_value",
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    let mut folders: Vec<PathBuf> = [
        "crates/akm-core/src",
        "crates/akm-cli/src",
        "desktop/src-tauri/src",
    ]
    .iter()
    .map(|folder| root.join(folder))
    .collect();
    while let Some(folder) = folders.pop() {
        for item in std::fs::read_dir(&folder).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 10, "the scan found the source files");
    let mut found = Vec::new();
    for path in files {
        if path.ends_with("akm-core/src/schedule.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap();
            let mut words: Vec<(usize, &str)> = Vec::new();
            let mut start = None;
            for (at, c) in code.char_indices().chain([(code.len(), ' ')]) {
                let part = c.is_alphanumeric() || c == '_';
                match (part, start) {
                    (true, None) => start = Some(at),
                    (false, Some(from)) => {
                        words.push((from, &code[from..at]));
                        start = None;
                    }
                    _ => {}
                }
            }
            for (index, &(at, word)) in words.iter().enumerate() {
                let rest = code[at + word.len()..].trim_start();
                let next = words.get(index + 1).map(|&(_, w)| w);
                let chrono_duration = word == "Duration"
                    && rest.starts_with("::")
                    && next.is_some_and(|w| DURATION.contains(&w))
                    || word == "Duration" && code.contains("chrono");
                if NAMES.contains(&word)
                    || CALLS.contains(&word) && rest.starts_with('(')
                    || chrono_duration
                {
                    found.push(format!("{}:{}: {word}", path.display(), number + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "date arithmetic outside akm-core/src/schedule.rs; add a checked or saturating function there and call it:\n{}",
        found.join("\n")
    );
}

// ---------------------------------------------- core review 1, finding 7: whole seconds only

/// Finding 7 (class O, validate where input comes in): the rule lives in `ApiKeyInput`, so
/// core `add` and `update` refuse a sub-second expiry from any caller, the desktop draft
/// included. Nothing is stored, and the stored key keeps its expiry.
#[test]
fn core_add_and_update_refuse_a_sub_second_expiry_and_change_nothing() {
    let (directory, mut vault, _) = vault_at("2026-01-01T00:00:00Z");
    let fractional = [
        utc("2030-01-02T00:00:45.900Z"),
        utc("2030-01-02T00:00:45.000000001Z"),
        utc("2030-01-02T00:00:45.999999999Z"),
    ];
    for expires in fractional {
        let mut input = ApiKeyInput::new("Fractional", "fake-secret");
        input.expires_at = Some(expires);
        let message = vault.add(input).unwrap_err().to_string();
        assert!(
            message.contains("fraction of a second"),
            "{expires}: {message}"
        );
    }
    assert!(vault.list().is_empty(), "nothing was stored");

    let mut whole = ApiKeyInput::new("Whole", "fake-secret");
    whole.expires_at = Some(utc("2030-01-02T00:00:45Z"));
    let key = vault.add(whole).unwrap();
    let path = directory.path().join("vault.kdbx");
    let before = std::fs::read(&path).unwrap();
    for expires in fractional {
        let mut edit = vault.edit_input(&key.id).unwrap();
        edit.expires_at = Some(expires);
        let message = vault.update(&key.id, edit).unwrap_err().to_string();
        assert!(
            message.contains("fraction of a second"),
            "{expires}: {message}"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), before, "nothing was written");
    assert_eq!(
        vault.get_by_id(&key.id).unwrap().expires_at,
        Some(utc("2030-01-02T00:00:45Z"))
    );
}

/// Finding 7, the text path: chrono keeps nine fraction digits and drops the rest, so the check
/// reads the text that the person wrote. Any nonzero fraction digit refuses, at any position.
#[test]
fn an_expiry_fraction_beyond_nine_digits_is_refused_from_the_text() {
    for input in [
        "2030-01-02T00:00:45.0000000001Z",
        "2030-01-02T00:00:45.000000000000000009Z",
        "2030-01-02T00:00:45.0000000001+02:00",
    ] {
        let message = parse_expiry(input)
            .expect_err(&format!("{input} was accepted"))
            .to_string();
        assert!(
            message.contains("fraction of a second"),
            "{input}: {message}"
        );
    }
    assert_eq!(
        parse_expiry("2030-01-02T00:00:45.000000000000Z").unwrap(),
        utc("2030-01-02T00:00:45Z")
    );
}
