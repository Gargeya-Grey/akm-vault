//! Unit 2: secret-change time, native history, Recycle Bin, deletion records and restore.
//! Every test uses a fake vault, a controlled clock and literal expected values.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use akm_core::model::{META_ID, META_ROTATION_DAYS, META_SCOPES, META_SECRET_CHANGED};
use akm_core::schedule::Remaining;
use akm_core::{ApiKeyInput, Clock, Error, KeyDisclosure, Secret, SecretChange, Vault};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use keepass::db::{fields, Database, Entry};
use keepass::DatabaseKey;

const PASSWORD: &str = "correct horse battery staple";

#[derive(Clone)]
struct ManualClock(Arc<Mutex<DateTime<Utc>>>);

impl ManualClock {
    fn at(text: &str) -> Self {
        Self(Arc::new(Mutex::new(utc(text))))
    }
    fn set(&self, text: &str) {
        *self.0.lock().unwrap() = utc(text);
    }
    fn advance(&self, duration: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += duration;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn naive(text: &str) -> NaiveDateTime {
    utc(text).naive_utc()
}

struct Fixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
    clock: ManualClock,
    vault: Vault,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        let clock = ManualClock::at("2026-01-01T00:00:00Z");
        let vault = Vault::create(&path, PASSWORD, None)
            .unwrap()
            .with_clock(Arc::new(clock.clone()));
        Self {
            _directory: directory,
            path,
            clock,
            vault,
        }
    }

    fn raw(&self) -> Database {
        raw(&self.path)
    }

    fn secret(&self, id: &str) -> String {
        let key = self.vault.get_by_id(id).unwrap();
        let proof = self
            .vault
            .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
            .unwrap();
        self.vault.secret(id, &proof).unwrap().to_string()
    }
}

fn raw(path: &Path) -> Database {
    Database::parse(
        &fs::read(path).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap()
}

fn input(title: &str, secret: &str) -> ApiKeyInput {
    let mut input = ApiKeyInput::new(title, secret);
    input.project = Some("demo".into());
    input.environment = Some("dev".into());
    input.env_var = Some("DEMO_KEY".into());
    input.rotation_days = Some(30);
    input
}

fn with_notes(mut input: ApiKeyInput, notes: &str) -> ApiKeyInput {
    input.notes = Some(notes.into());
    input
}

fn history_len(db: &Database, id: &str) -> usize {
    let id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
    db.entry(id)
        .and_then(|entry| entry.history.as_ref().map(|h| h.get_entries().len()))
        .unwrap_or(0)
}

fn fast_database() -> Database {
    let mut db = Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    db
}

fn write_database(path: &Path, db: &Database) {
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
}

// ---------------------------------------------------------------- H9: the secret-change time

#[test]
fn a_new_key_records_its_creation_as_the_secret_change_time() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    assert_eq!(key.secret_changed_at, Some(utc("2026-01-01T00:00:00Z")));
    assert_eq!(key.secret_change, SecretChange::Known);
    assert_eq!(key.history_count, 0);
    assert_eq!(
        key.rotation_remaining(utc("2026-01-11T00:00:00Z")),
        Some(Remaining::In { days: 20 }),
        "30 days after the secret change, minus 10 days"
    );
    let stored = f.raw();
    let entry = stored.iter_all_entries().next().unwrap();
    assert_eq!(entry.get(META_SECRET_CHANGED), Some("2026-01-01T00:00:00Z"));
}

#[test]
fn a_notes_only_edit_does_not_reset_the_secret_change_time() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-11T00:00:00Z");
    let edited = f
        .vault
        .update(
            &key.id,
            with_notes(input("Alpha", "secret-one"), "new note"),
        )
        .unwrap();
    assert_eq!(edited.notes.as_deref(), Some("new note"));
    assert_eq!(edited.updated_at, Some(utc("2026-01-11T00:00:00Z")));
    assert_eq!(edited.secret_changed_at, Some(utc("2026-01-01T00:00:00Z")));
    assert_eq!(edited.history_count, 1);
    assert_eq!(
        edited.rotation_remaining(utc("2026-01-11T00:00:00Z")),
        Some(Remaining::In { days: 20 }),
        "the rotation age still counts from the secret change, not from the notes edit"
    );
}

#[test]
fn an_edit_that_changes_nothing_adds_no_history_no_save_and_no_record() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let bytes = fs::read(&f.path).unwrap();
    let records = f.vault.audit_log().read(100).unwrap().len();
    f.clock.set("2026-02-01T00:00:00Z");
    let again = f
        .vault
        .update(&key.id, input("Alpha", "secret-one"))
        .unwrap();
    assert_eq!(again, key);
    assert_eq!(
        fs::read(&f.path).unwrap(),
        bytes,
        "the file was not written"
    );
    assert_eq!(f.vault.audit_log().read(100).unwrap().len(), records);
    assert_eq!(history_len(&f.raw(), &key.id), 0);
}

#[test]
fn a_changed_secret_in_an_update_resets_the_time_and_keeps_the_old_secret_in_history() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-21T00:00:00Z");
    let changed = f
        .vault
        .update(&key.id, input("Alpha", "secret-two"))
        .unwrap();
    assert_eq!(changed.secret_changed_at, Some(utc("2026-01-21T00:00:00Z")));
    assert_eq!(changed.history_count, 1);
    assert_eq!(f.secret(&key.id), "secret-two");
    let stored = f.raw();
    let entry = stored.iter_all_entries().next().unwrap();
    let previous = entry.historical(0).unwrap();
    assert_eq!(previous.get(fields::PASSWORD), Some("secret-one"));
    assert_eq!(
        previous.get(META_SECRET_CHANGED),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!(
        previous.times.last_modification,
        Some(naive("2026-01-01T00:00:00Z"))
    );
}

#[test]
fn an_import_resets_the_time_only_for_a_changed_secret() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-05T00:00:00Z");
    let before = fs::read(&f.path).unwrap();
    assert_eq!(
        f.vault
            .import_env(
                &[("DEMO_KEY".into(), "secret-one".into())],
                Some("demo"),
                Some("dev")
            )
            .unwrap(),
        (0, 0)
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);

    f.clock.set("2026-01-09T00:00:00Z");
    assert_eq!(
        f.vault
            .import_env(
                &[
                    ("DEMO_KEY".into(), "secret-two".into()),
                    ("ADDED".into(), "fresh".into())
                ],
                Some("demo"),
                Some("dev")
            )
            .unwrap(),
        (1, 1)
    );
    let updated = f.vault.get_by_id(&key.id).unwrap();
    assert_eq!(updated.secret_changed_at, Some(utc("2026-01-09T00:00:00Z")));
    assert_eq!(updated.history_count, 1);
    let added = f.vault.find("ADDED").unwrap();
    assert_eq!(added.secret_changed_at, Some(utc("2026-01-09T00:00:00Z")));
}

#[test]
fn two_edits_in_the_same_second_keep_both_versions_in_order() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let first = f
        .vault
        .update(&key.id, with_notes(input("Alpha", "secret-one"), "first"))
        .unwrap();
    let second = f
        .vault
        .update(&key.id, with_notes(input("Alpha", "secret-one"), "second"))
        .unwrap();
    assert_eq!(first.updated_at, Some(utc("2026-01-01T00:00:01Z")));
    assert_eq!(second.updated_at, Some(utc("2026-01-01T00:00:02Z")));
    assert_eq!(second.history_count, 2);
    let versions = f.vault.history(&key.id).unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|v| v.modified_at.unwrap())
            .collect::<Vec<_>>(),
        vec![utc("2026-01-01T00:00:01Z"), utc("2026-01-01T00:00:00Z")]
    );
    let raw = f.raw();
    let entry = raw.iter_all_entries().next().unwrap();
    assert_eq!(
        entry.historical(0).unwrap().get(fields::NOTES),
        Some("first")
    );
    assert_eq!(entry.historical(1).unwrap().get(fields::NOTES), None);
}

#[test]
fn a_clock_that_goes_back_never_moves_a_time_backwards() {
    let mut f = Fixture::new();
    f.clock.set("2026-06-01T00:00:00Z");
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-01T00:00:00Z");
    let rotated = f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    assert_eq!(rotated.changed_at, utc("2026-06-01T00:00:01Z"));
    assert_eq!(
        rotated.previous_changed_at,
        Some(utc("2026-06-01T00:00:00Z"))
    );
    let after = f.vault.get_by_id(&key.id).unwrap();
    assert_eq!(after.updated_at, Some(utc("2026-06-01T00:00:01Z")));
    assert_eq!(after.secret_changed_at, Some(utc("2026-06-01T00:00:01Z")));
    let versions = f.vault.history(&key.id).unwrap();
    assert_eq!(versions[0].modified_at, Some(utc("2026-06-01T00:00:00Z")));
}

#[test]
fn a_failed_save_changes_no_time_and_adds_no_history() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-02-01T00:00:00Z");
    fs::write(&f.path, b"another device replaced the file").unwrap();
    for attempt in 0..3 {
        let result = match attempt {
            0 => f
                .vault
                .update(&key.id, input("Alpha", "secret-two"))
                .map(|_| ()),
            1 => f
                .vault
                .rotate(&key.id, Secret::new("secret-two"))
                .map(|_| ()),
            _ => f.vault.remove(&key.id).map(|_| ()),
        };
        assert!(
            matches!(result, Err(Error::VaultChangedOnDisk)),
            "attempt {attempt}"
        );
        assert_eq!(
            f.vault.get_by_id(&key.id).unwrap(),
            key,
            "attempt {attempt}"
        );
        assert!(f.vault.recycled().is_empty(), "attempt {attempt}");
    }
    assert_eq!(f.secret(&key.id), "secret-one");
}

#[test]
fn a_legacy_key_without_a_time_shows_unknown_and_never_an_invented_date() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = fast_database();
    let id = {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "Legacy");
        entry.set_protected(fields::PASSWORD, "legacy-secret");
        entry.set_unprotected("AKM_ROTATION_DAYS", "30");
        entry.times.creation = Some(naive("2020-01-01T00:00:00Z"));
        entry.times.last_modification = Some(naive("2020-01-02T00:00:00Z"));
        entry.times.location_changed = Some(naive("2020-01-01T00:00:00Z"));
        entry.id().to_string()
    };
    write_database(&path, &db);
    let clock = ManualClock::at("2026-03-01T00:00:00Z");
    let mut vault = Vault::open(&path, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(clock.clone()));

    let legacy = vault.get_by_id(&id).unwrap();
    assert_eq!(legacy.secret_changed_at, None);
    assert_eq!(legacy.secret_change, SecretChange::Unknown);
    assert_eq!(legacy.rotation_remaining(utc("2026-03-01T00:00:00Z")), None);

    let mut edit = vault.edit_input(&id).unwrap();
    edit.notes = Some("only a note".into());
    let noted = vault.update(&id, edit).unwrap();
    assert_eq!(
        noted.secret_change,
        SecretChange::Unknown,
        "a notes edit proves nothing"
    );
    assert_eq!(noted.secret_changed_at, None);

    clock.set("2026-03-02T00:00:00Z");
    let rotated = vault.rotate(&id, Secret::new("fresh-secret")).unwrap();
    assert_eq!(rotated.previous_changed_at, None);
    assert_eq!(rotated.changed_at, utc("2026-03-02T00:00:00Z"));
    assert_eq!(
        vault.get_by_id(&id).unwrap().secret_change,
        SecretChange::Known
    );
}

#[test]
fn a_secret_changed_in_another_program_is_unknown_not_the_old_time() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = fast_database();
    let id = {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "API Keys".into();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "Outside");
        entry.set_protected(fields::PASSWORD, "first-secret");
        entry.set_unprotected(META_SECRET_CHANGED, "2026-01-01T00:00:00Z");
        entry.times.last_modification = Some(naive("2026-01-01T00:00:00Z"));
        let before: keepass::db::Entry = (*entry).clone();
        entry.history.as_mut().unwrap().add_entry(before);
        entry.set_protected(fields::PASSWORD, "second-secret");
        entry.times.last_modification = Some(naive("2026-02-01T00:00:00Z"));
        entry.id().to_string()
    };
    write_database(&path, &db);
    let vault = Vault::open(&path, PASSWORD).unwrap();
    let key = vault.get_by_id(&id).unwrap();
    assert_eq!(key.secret_change, SecretChange::ChangedOutside);
    assert_eq!(key.secret_changed_at, None);
    assert_eq!(key.rotation_remaining(utc("2026-03-01T00:00:00Z")), None);
}

// ---------------------------------------------------------------- rotate

#[test]
fn rotate_replaces_the_secret_records_time_history_and_a_secret_free_record() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "old-secret-value")).unwrap();
    f.clock.set("2026-01-15T12:30:00Z");
    let rotation = f
        .vault
        .rotate(&key.id, Secret::new("new-secret-value"))
        .unwrap();
    assert_eq!(rotation.changed_at, utc("2026-01-15T12:30:00Z"));
    assert_eq!(
        rotation.previous_changed_at,
        Some(utc("2026-01-01T00:00:00Z"))
    );
    assert_eq!(rotation.key.history_count, 1);
    assert_eq!(f.secret(&key.id), "new-secret-value");

    let rendered = format!("{rotation:?} {}", serde_json::to_string(&rotation).unwrap());
    assert!(!rendered.contains("old-secret-value") && !rendered.contains("new-secret-value"));

    let records = f.vault.audit_log().read(100).unwrap();
    let record = records
        .iter()
        .find(|record| record.action == "rotate-completed")
        .unwrap();
    assert_eq!(record.key_id.as_deref(), Some(key.id.as_str()));
    let text = serde_json::to_string(&records).unwrap();
    assert!(!text.contains("old-secret-value") && !text.contains("new-secret-value"));
    assert!(!text.contains("Alpha") && !text.contains("demo"));
}

#[test]
fn rotate_refuses_an_unchanged_or_empty_secret_and_changes_nothing() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let bytes = fs::read(&f.path).unwrap();
    f.clock.set("2026-02-01T00:00:00Z");
    for value in ["secret-one", ""] {
        let error = f.vault.rotate(&key.id, Secret::new(value)).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)), "{value:?}");
    }
    assert!(f
        .vault
        .rotate(&key.id, Secret::new("secret-one"))
        .unwrap_err()
        .to_string()
        .contains("same as the current"));
    assert_eq!(fs::read(&f.path).unwrap(), bytes);
    assert_eq!(f.vault.get_by_id(&key.id).unwrap(), key);
}

#[test]
fn a_repeated_rotation_with_the_same_value_is_refused_so_a_lost_reply_cannot_rotate_twice() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-02-01T00:00:00Z");
    f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    f.clock.set("2026-02-02T00:00:00Z");
    assert!(f.vault.rotate(&key.id, Secret::new("secret-two")).is_err());
    let after = f.vault.get_by_id(&key.id).unwrap();
    assert_eq!(after.secret_changed_at, Some(utc("2026-02-01T00:00:00Z")));
    assert_eq!(after.history_count, 1);
}

#[test]
fn rotate_needs_a_writable_vault_and_an_existing_live_key() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    assert!(matches!(
        f.vault.rotate("not-a-uuid", Secret::new("x")),
        Err(Error::EntryNotFound(_))
    ));
    f.vault.remove(&key.id).unwrap();
    assert!(
        matches!(
            f.vault.rotate(&key.id, Secret::new("secret-two")),
            Err(Error::EntryNotFound(_))
        ),
        "a recycled key cannot be rotated; restore it first"
    );
}

// ---------------------------------------------------------------- history

#[test]
fn history_lists_versions_newest_first_with_field_names_and_no_values() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-02T00:00:00Z");
    f.vault
        .update(&key.id, with_notes(input("Alpha", "secret-one"), "a note"))
        .unwrap();
    f.clock.set("2026-01-03T00:00:00Z");
    f.vault
        .update(&key.id, with_notes(input("Alpha", "secret-two"), "a note"))
        .unwrap();
    let versions = f.vault.history(&key.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].index, 0);
    assert_eq!(versions[0].modified_at, Some(utc("2026-01-02T00:00:00Z")));
    assert!(versions[0].secret_changed);
    assert!(versions[0].changed.contains(&"Password".to_string()));
    assert!(versions[0]
        .changed
        .contains(&"AKM_SECRET_CHANGED".to_string()));
    assert_eq!(versions[1].modified_at, Some(utc("2026-01-01T00:00:00Z")));
    assert!(!versions[1].secret_changed);
    assert_eq!(versions[1].changed, vec!["Notes".to_string()]);
    let text = serde_json::to_string(&versions).unwrap();
    assert!(
        !text.contains("secret-one") && !text.contains("secret-two") && !text.contains("a note")
    );
}

// ---------------------------------------------------------------- Recycle Bin, restore, purge

#[test]
fn remove_moves_the_key_to_a_recycle_bin_that_it_creates() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-04T08:00:00Z");
    f.vault.remove(&key.id).unwrap();
    assert!(f.vault.list().is_empty());
    assert!(f.vault.get_by_id(&key.id).is_err());

    let recycled = f.vault.recycled();
    assert_eq!(recycled.len(), 1);
    assert_eq!(recycled[0].key.id, key.id);
    assert_eq!(recycled[0].key.title, "Alpha");
    assert_eq!(recycled[0].deleted_at, Some(utc("2026-01-04T08:00:00Z")));

    let stored = f.raw();
    let bin = stored
        .recycle_bin()
        .expect("RecycleBinUUID points at a group");
    assert_eq!(bin.name, "Recycle Bin");
    assert_eq!(bin.enable_searching, Some(false));
    assert_eq!(bin.enable_autotype, Some(false));
    assert_eq!(stored.meta.recyclebin_enabled, Some(true));
    assert_eq!(
        stored.meta.recyclebin_changed,
        Some(naive("2026-01-04T08:00:00Z"))
    );
    let entry = bin.entries().next().unwrap();
    assert_eq!(entry.id().to_string(), key.id);
    assert_eq!(entry.previous_parent().unwrap().name, "API Keys");
    assert_eq!(
        entry.times.location_changed,
        Some(naive("2026-01-04T08:00:00Z"))
    );
    assert_eq!(
        entry.times.last_modification,
        Some(naive("2026-01-01T00:00:00Z")),
        "a move is not an edit"
    );
    assert!(
        stored.deleted_objects.is_empty(),
        "a recycle is not a permanent delete"
    );
    assert_eq!(stored.root().groups().count(), 2);
}

#[test]
fn a_recycled_key_is_never_exported_injected_or_imported_over() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.vault.remove(&key.id).unwrap();
    assert!(f
        .vault
        .plan_env(Some("demo"), Some("dev"), false)
        .unwrap()
        .is_empty());
    assert!(f.vault.find("Alpha").is_err());
    let (added, updated) = f
        .vault
        .import_env(
            &[("DEMO_KEY".into(), "secret-one".into())],
            Some("demo"),
            Some("dev"),
        )
        .unwrap();
    assert_eq!((added, updated), (1, 0));
    assert_ne!(f.vault.find("DEMO_KEY").unwrap().id, key.id);
}

#[test]
fn remove_reuses_an_existing_recycle_bin_and_a_stale_pointer_gets_a_new_one() {
    let mut f = Fixture::new();
    let first = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let second = f.vault.add(input("Beta", "secret-two")).unwrap();
    f.vault.remove(&first.id).unwrap();
    f.vault.remove(&second.id).unwrap();
    let stored = f.raw();
    assert_eq!(stored.root().groups().count(), 2, "one bin for both keys");
    assert_eq!(stored.recycle_bin().unwrap().entries().count(), 2);

    let mut stale = fast_database();
    stale.meta.recyclebin_enabled = Some(true);
    stale.meta.recyclebin_uuid = Some(uuid::Uuid::new_v4());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    write_database(&path, &stale);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let key = vault.add(input("Gamma", "secret-three")).unwrap();
    vault.remove(&key.id).unwrap();
    let stored = raw(&path);
    assert_ne!(stored.meta.recyclebin_uuid, stale.meta.recyclebin_uuid);
    assert_eq!(stored.recycle_bin().unwrap().entries().count(), 1);
}

#[test]
fn a_vault_with_the_recycle_bin_turned_off_refuses_remove_and_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = fast_database();
    db.meta.recyclebin_enabled = Some(false);
    write_database(&path, &db);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let key = vault.add(input("Alpha", "secret-one")).unwrap();
    let bytes = fs::read(&path).unwrap();
    let error = vault.remove(&key.id).unwrap_err();
    assert!(
        error.to_string().contains("Recycle Bin is turned off"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(vault.list().len(), 1);
    vault.purge(&key.id).unwrap();
    assert!(vault.list().is_empty(), "an explicit purge still works");
}

#[test]
fn restore_returns_the_same_key_to_its_group_with_its_history() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-02T00:00:00Z");
    f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    f.clock.set("2026-01-03T00:00:00Z");
    f.vault.remove(&key.id).unwrap();
    f.clock.set("2026-01-04T00:00:00Z");
    let restored = f.vault.restore(&key.id).unwrap();
    assert_eq!(restored.id, key.id);
    assert_eq!(restored.history_count, 1);
    assert_eq!(
        restored.secret_changed_at,
        Some(utc("2026-01-02T00:00:00Z"))
    );
    assert_eq!(f.vault.list().len(), 1);
    assert!(f.vault.recycled().is_empty());
    assert_eq!(f.secret(&key.id), "secret-two");
    let stored = f.raw();
    let root = stored.root();
    let group = root.group_by_name("API Keys").unwrap();
    let entry = group.entries().next().unwrap();
    assert_eq!(entry.id().to_string(), key.id);
    assert_eq!(
        entry.times.location_changed,
        Some(naive("2026-01-04T00:00:00Z"))
    );
    let records = f.vault.audit_log().read(100).unwrap();
    let restored_record = records
        .iter()
        .find(|record| record.action == "restore-completed")
        .unwrap();
    assert_eq!(restored_record.key_id.as_deref(), Some(key.id.as_str()));
}

#[test]
fn restore_refuses_a_name_that_a_live_key_now_uses_and_a_key_that_is_not_recycled() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    assert!(matches!(
        f.vault.restore(&key.id),
        Err(Error::EntryNotFound(_))
    ));
    f.vault.remove(&key.id).unwrap();
    let live = f.vault.add(input("Alpha again", "secret-two")).unwrap();
    let error = f.vault.restore(&key.id).unwrap_err();
    assert!(error.to_string().contains("DEMO_KEY"), "{error}");
    assert_eq!(f.vault.recycled().len(), 1);
    f.vault.remove(&live.id).unwrap();
    assert_eq!(f.vault.restore(&key.id).unwrap().id, key.id);
}

#[test]
fn purge_deletes_for_good_with_a_record_later_than_every_version_of_the_key() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.clock.set("2026-01-10T00:00:00Z");
    f.vault
        .update(&key.id, with_notes(input("Alpha", "secret-one"), "noted"))
        .unwrap();
    f.clock.set("2026-01-20T00:00:00Z");
    f.vault.remove(&key.id).unwrap();
    f.clock.set("2026-01-01T00:00:00Z");
    let purged = f.vault.purge(&key.id).unwrap();
    assert_eq!(purged.id, key.id);
    assert!(f.vault.recycled().is_empty());
    assert!(f.vault.get_by_id(&key.id).is_err());
    assert!(matches!(
        f.vault.restore(&key.id),
        Err(Error::EntryNotFound(_))
    ));
    let stored = f.raw();
    assert_eq!(stored.iter_all_entries().count(), 0);
    let uuid: uuid::Uuid = key.id.parse().unwrap();
    assert_eq!(
        stored.deleted_objects.get(&uuid),
        Some(&Some(naive("2026-01-20T00:00:01Z"))),
        "the clock went back, so the record follows the last known time of the key"
    );
    let record = f.vault.audit_log().read(100).unwrap();
    assert_eq!(record.last().unwrap().action, "purge-completed");
    assert_eq!(
        record.last().unwrap().key_id.as_deref(),
        Some(key.id.as_str())
    );
}

#[test]
fn nothing_purges_by_itself_and_a_retention_purge_removes_only_old_recycled_keys() {
    let mut f = Fixture::new();
    let old = f.vault.add(input("Old", "secret-one")).unwrap();
    let mut other = input("Newer", "secret-two");
    other.env_var = Some("OTHER_KEY".into());
    let newer = f.vault.add(other).unwrap();
    let live = f
        .vault
        .add(input_named("Live", "secret-three", "LIVE_KEY"))
        .unwrap();
    f.clock.set("2026-01-02T00:00:00Z");
    f.vault.remove(&old.id).unwrap();
    f.clock.set("2026-02-10T00:00:00Z");
    f.vault.remove(&newer.id).unwrap();
    f.clock.set("2027-06-01T00:00:00Z");
    f.vault
        .update(
            &live.id,
            with_notes(input_named("Live", "secret-three", "LIVE_KEY"), "n"),
        )
        .unwrap();
    assert_eq!(
        f.vault.recycled().len(),
        2,
        "no save and no passing time purges"
    );

    f.clock.set("2026-02-20T00:00:00Z");
    let purged = f.vault.purge_recycled_older_than(30).unwrap();
    assert_eq!(
        purged.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(),
        vec![old.id.as_str()]
    );
    let left = f.vault.recycled();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].key.id, newer.id);
    assert_eq!(f.vault.list().len(), 1);
    assert_eq!(f.raw().deleted_objects.len(), 1);
}

fn input_named(title: &str, secret: &str, var: &str) -> ApiKeyInput {
    let mut input = input(title, secret);
    input.env_var = Some(var.into());
    input
}

#[test]
fn unrelated_history_attachments_groups_custom_data_and_kdf_survive_every_operation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    fs::write(&path, include_bytes!("fixtures/kdbx40.kdbx")).unwrap();
    let clock = ManualClock::at("2026-04-01T00:00:00Z");
    let mut vault = Vault::open(&path, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(clock.clone()));
    vault.upgrade().unwrap();
    let before = foreign_parts(&raw(&path));
    assert!(before.contains("Other entry") && before.contains("shared.txt"));

    let target = vault.find("Foreign fixture").unwrap();
    clock.advance(Duration::days(1));
    vault
        .rotate(&target.id, Secret::new("rotated-fake"))
        .unwrap();
    clock.advance(Duration::days(1));
    let mut edit = vault.edit_input(&target.id).unwrap();
    edit.notes = Some("a note".into());
    vault.update(&target.id, edit).unwrap();
    clock.advance(Duration::days(1));
    vault.remove(&target.id).unwrap();
    assert_eq!(foreign_parts(&raw(&path)), before);
    clock.advance(Duration::days(1));
    vault.restore(&target.id).unwrap();
    assert_eq!(foreign_parts(&raw(&path)), before);
    clock.advance(Duration::days(1));
    vault.remove(&target.id).unwrap();
    clock.advance(Duration::days(1));
    vault.purge(&target.id).unwrap();
    assert_eq!(foreign_parts(&raw(&path)), before);

    let stored = raw(&path);
    let root = stored.root();
    let group = root.group_by_name("Unrelated").unwrap();
    let shared = group.entry_by_name("Other entry").unwrap();
    assert_eq!(
        shared.attachment_by_name("shared.txt").unwrap().get(),
        b"attachment fixture bytes"
    );
    assert!(stored.meta.custom_data.contains_key("fixture.meta"));
    assert!(stored
        .deleted_objects
        .contains_key(&target.id.parse().unwrap()));
}

/// Everything that is not the AKM key: the unrelated group, its entries with their history,
/// attachments and custom data, the metadata custom data and the cipher and KDF settings.
fn foreign_parts(db: &Database) -> String {
    let mut out = format!("{:?}\n", db.config);
    // `AKM_BACKUPS` is AKM's own ledger of its backups (FR-01), which every save updates.
    // Every other item is foreign and must not change.
    let mut custom: Vec<_> = db
        .meta
        .custom_data
        .iter()
        .filter(|(key, _)| key.as_str() != "AKM_BACKUPS")
        .map(|(key, item)| format!("{key}={item:?}"))
        .collect();
    custom.sort();
    out += &custom.join(";");
    let root = db.root();
    let unrelated = root
        .group_by_name("Unrelated")
        .expect("the unrelated group");
    let mut entries: Vec<_> = unrelated.entries().collect();
    entries.sort_by_key(|entry| entry.get(fields::TITLE).map(str::to_string));
    for entry in entries {
        let mut names: Vec<_> = entry
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={v:?}"))
            .collect();
        names.sort();
        out += &format!(
            "\n{} {:?} {}",
            entry.get(fields::TITLE).unwrap(),
            entry.times,
            names.join(";")
        );
        for (name, attachment) in entry.attachments_named() {
            out += &format!(" [{name}:{:?}]", attachment.get());
        }
        let versions = entry.history.as_ref().map_or(0, |h| h.get_entries().len());
        for index in 0..versions {
            let version = entry.historical(index).unwrap();
            let mut fields: Vec<_> = version
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={v:?}"))
                .collect();
            fields.sort();
            out += &format!("\n  history {index}: {}", fields.join(";"));
            for (name, attachment) in version.attachments_named() {
                out += &format!(" [{name}:{:?}]", attachment.get());
            }
        }
    }
    out
}

// ------------------------------------------- review 1, finding 6: foreign history sets the floor

const FOREIGN_SECRET: &str = "foreign-older-secret";

impl Fixture {
    /// A foreign editor leaves a history version with a last-modification time `modified`
    /// and its own secret, then the vault is opened again with the same controlled clock.
    fn with_foreign_history(&mut self, id: &str, modified: &str) {
        self.with_foreign_history_at(id, naive(modified));
    }

    fn with_foreign_history_at(&mut self, id: &str, modified: NaiveDateTime) {
        let mut db = self.raw();
        let entry_id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
        {
            let mut entry = db.entry_mut(entry_id).unwrap();
            let mut older = (*entry).clone();
            older.history = None;
            older.set_protected(fields::PASSWORD, FOREIGN_SECRET);
            older.times.last_modification = Some(modified);
            entry
                .history
                .get_or_insert_with(Default::default)
                .add_entry(older);
        }
        write_database(&self.path, &db);
        self.vault = Vault::open(&self.path, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(self.clock.clone()));
    }

    fn foreign_history_is_intact(&self, id: &str, count: usize) {
        let stored = self.raw();
        let entry_id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
        let entry = stored.entry(entry_id).unwrap();
        let versions = entry.history.as_ref().unwrap().get_entries().len();
        assert_eq!(versions, count);
        let foreign = (0..versions)
            .map(|index| entry.historical(index).unwrap())
            .find(|version| version.get(fields::PASSWORD) == Some(FOREIGN_SECRET))
            .expect("the foreign version stays in the history");
        assert_eq!(
            foreign.times.last_modification,
            Some(naive("2030-01-01T00:00:00Z"))
        );
    }

    fn assert_known_after_future_history(&self, id: &str, changed: &str) {
        let key = self.vault.get_by_id(id).unwrap();
        assert_eq!(key.secret_change, SecretChange::Known);
        assert_eq!(key.secret_changed_at, Some(utc(changed)));
        assert_eq!(key.updated_at, Some(utc(changed)));
        assert_eq!(
            key.rotation_due_at(),
            Some(utc(changed) + Duration::days(30)),
            "the rotation due instant counts from the new secret-change time"
        );
        let versions = self.vault.history(id).unwrap();
        for version in &versions {
            assert!(
                version.modified_at.unwrap() < utc(changed),
                "every history version is older than the new time: {version:?}"
            );
        }
        let stored = self.raw();
        let entry_id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
        let entry = stored.entry(entry_id).unwrap();
        assert_eq!(entry.get(META_SECRET_CHANGED), Some(changed));
    }
}

#[test]
fn rotate_after_a_foreign_future_history_version_is_known_and_ordered() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    assert_eq!(
        f.vault.get_by_id(&key.id).unwrap().secret_change,
        SecretChange::ChangedOutside,
        "the foreign change is real before AKM rotates"
    );
    f.clock.set("2026-02-01T00:00:00Z");
    let rotated = f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    assert_eq!(rotated.changed_at, utc("2030-01-01T00:00:01Z"));
    assert_eq!(rotated.previous_changed_at, None);
    f.assert_known_after_future_history(&key.id, "2030-01-01T00:00:01Z");
    f.foreign_history_is_intact(&key.id, 2);
    assert_eq!(f.secret(&key.id), "secret-two");
}

#[test]
fn update_with_a_new_secret_after_a_foreign_future_history_version_is_known_and_ordered() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2026-02-01T00:00:00Z");
    f.vault
        .update(&key.id, input("Alpha", "secret-two"))
        .unwrap();
    f.assert_known_after_future_history(&key.id, "2030-01-01T00:00:01Z");
    f.foreign_history_is_intact(&key.id, 2);
}

#[test]
fn import_of_a_new_secret_after_a_foreign_future_history_version_is_known_and_ordered() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2026-02-01T00:00:00Z");
    let counts = f
        .vault
        .import_env(
            &[("DEMO_KEY".to_string(), "secret-two".to_string())],
            Some("demo"),
            Some("dev"),
        )
        .unwrap();
    assert_eq!(counts, (0, 1));
    f.assert_known_after_future_history(&key.id, "2030-01-01T00:00:01Z");
    f.foreign_history_is_intact(&key.id, 2);
}

#[test]
fn a_notes_edit_after_a_foreign_future_history_version_orders_the_time_and_keeps_the_foreign_change_visible(
) {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2026-02-01T00:00:00Z");
    let edited = f
        .vault
        .update(
            &key.id,
            with_notes(input("Alpha", "secret-one"), "a later note"),
        )
        .unwrap();
    assert_eq!(edited.updated_at, Some(utc("2030-01-01T00:00:01Z")));
    assert_eq!(
        edited.secret_change,
        SecretChange::ChangedOutside,
        "AKM did not change the secret, so the foreign change stays visible"
    );
    assert_eq!(edited.secret_changed_at, None);
    f.foreign_history_is_intact(&key.id, 2);
}

#[test]
fn a_foreign_history_version_in_the_same_second_as_the_clock_still_orders_the_time() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2030-01-01T00:00:00Z");
    let rotated = f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    assert_eq!(rotated.changed_at, utc("2030-01-01T00:00:01Z"));
    f.assert_known_after_future_history(&key.id, "2030-01-01T00:00:01Z");
}

#[test]
fn two_rotations_after_a_foreign_future_history_version_stay_ordered_and_known() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2026-02-01T00:00:00Z");
    f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    let second = f
        .vault
        .rotate(&key.id, Secret::new("secret-three"))
        .unwrap();
    assert_eq!(second.changed_at, utc("2030-01-01T00:00:02Z"));
    assert_eq!(
        second.previous_changed_at,
        Some(utc("2030-01-01T00:00:01Z"))
    );
    f.assert_known_after_future_history(&key.id, "2030-01-01T00:00:02Z");
    f.foreign_history_is_intact(&key.id, 3);
}

#[test]
fn remove_and_restore_stamp_after_a_foreign_future_history_version() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "2030-01-01T00:00:00Z");
    f.clock.set("2026-02-01T00:00:00Z");
    f.vault.remove(&key.id).unwrap();
    assert_eq!(
        f.vault.recycled()[0].deleted_at,
        Some(utc("2030-01-01T00:00:01Z"))
    );
    f.vault.restore(&key.id).unwrap();
    let stored = f.raw();
    let entry_id = keepass::db::EntryId::from_uuid(key.id.parse().unwrap());
    assert_eq!(
        stored.entry(entry_id).unwrap().times.location_changed,
        Some(naive("2030-01-01T00:00:02Z"))
    );
}

#[test]
fn a_foreign_history_version_in_year_9999_gives_a_time_that_reads_back_as_known() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_history(&key.id, "9999-12-31T23:59:59Z");
    f.clock.set("2026-02-01T00:00:00Z");
    let rotation = f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    assert_eq!(
        rotation.changed_at.to_rfc3339(),
        "+10000-01-01T00:00:00+00:00"
    );
    let after = f.vault.get_by_id(&key.id).unwrap();
    assert_eq!(after.secret_change, SecretChange::Known);
    assert_eq!(after.secret_changed_at, Some(rotation.changed_at));
}

#[test]
fn a_foreign_history_version_at_the_last_representable_second_refuses_without_a_panic() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let last = chrono::NaiveDate::from_ymd_opt(262_142, 12, 31)
        .unwrap()
        .and_hms_opt(23, 59, 59)
        .unwrap();
    f.with_foreign_history_at(&key.id, last);
    f.clock.set("2026-02-01T00:00:00Z");
    let bytes = fs::read(&f.path).unwrap();
    let error = f
        .vault
        .rotate(&key.id, Secret::new("secret-two"))
        .unwrap_err();
    assert!(
        error.to_string().contains("too far in the future"),
        "{error}"
    );
    assert_eq!(fs::read(&f.path).unwrap(), bytes, "nothing was saved");
    assert_eq!(f.secret(&key.id), "secret-one");
}

#[test]
fn a_foreign_future_location_time_orders_a_rotation_and_a_removal_after_it() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    let mut db = f.raw();
    let entry_id = keepass::db::EntryId::from_uuid(key.id.parse().unwrap());
    db.entry_mut(entry_id).unwrap().times.location_changed = Some(naive("2030-01-01T00:00:00Z"));
    write_database(&f.path, &db);
    f.vault = Vault::open(&f.path, PASSWORD)
        .unwrap()
        .with_clock(Arc::new(f.clock.clone()));
    f.clock.set("2026-02-01T00:00:00Z");

    let rotated = f.vault.rotate(&key.id, Secret::new("secret-two")).unwrap();
    assert_eq!(rotated.changed_at, utc("2030-01-01T00:00:01Z"));
    f.vault.remove(&key.id).unwrap();
    assert_eq!(
        f.vault.recycled()[0].deleted_at,
        Some(utc("2030-01-01T00:00:02Z"))
    );
}

// ------------------------- diff review of fix round 2: an update keeps what it does not change

impl Fixture {
    /// Another KeePass program rewrites the entry of `id` in the vault file, then the vault is
    /// opened again with the same controlled clock.
    fn with_foreign_edit(&mut self, id: &str, edit: impl FnOnce(&mut Entry)) {
        let mut db = self.raw();
        let entry_id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
        edit(&mut db.entry_mut(entry_id).unwrap());
        write_database(&self.path, &db);
        self.vault = Vault::open(&self.path, PASSWORD)
            .unwrap()
            .with_clock(Arc::new(self.clock.clone()));
    }

    fn entry(&self, id: &str) -> Entry {
        let entry_id = keepass::db::EntryId::from_uuid(id.parse().unwrap());
        (*self.raw().entry(entry_id).unwrap()).clone()
    }
}

/// D2B-01, D2B-02: another KeePass program stored a title with outer spaces and a rotation
/// interval above AKM's limit. An edit of the notes changes the notes and nothing else: the
/// title keeps its spaces, and the interval is not judged, because the edit does not change it.
#[test]
fn an_update_writes_and_judges_only_the_fields_that_it_changes() {
    const TITLE: &str = "  Foreign\u{200B} e\u{301}  ";
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_edit(&key.id, |entry| {
        entry.set_unprotected(fields::TITLE, TITLE);
        entry.set_unprotected(META_ROTATION_DAYS, "50000");
        entry.fields.remove(fields::USERNAME);
        entry.fields.remove(META_SCOPES);
    });
    let before = f.entry(&key.id);

    f.clock.advance(Duration::days(1));
    let mut edit = f.vault.edit_input(&key.id).unwrap();
    edit.notes = Some("new note".into());
    let saved = f.vault.update(&key.id, edit).unwrap();

    assert_eq!(saved.title, TITLE);
    assert_eq!(saved.rotation_days, Some(50_000));
    let mut after = f.entry(&key.id);
    assert_eq!(after.get(fields::NOTES), Some("new note"));
    after.fields.remove(fields::NOTES);
    assert_eq!(after.fields, before.fields, "an untouched field changed");
    assert_eq!(after.tags, before.tags);
    assert_eq!(after.times.expires, before.times.expires);

    let mut edit = f.vault.edit_input(&key.id).unwrap();
    edit.rotation_days = Some(50_001);
    assert!(
        matches!(f.vault.update(&key.id, edit), Err(Error::Invalid(_))),
        "an interval that the edit changes is judged"
    );
}

/// D2B-03: a key that another program wrote, with no field of AKM. An update that gives the
/// values that the key already has saves nothing: the vault file keeps its bytes.
#[test]
fn an_update_with_the_stored_values_of_a_key_from_another_program_saves_nothing() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_edit(&key.id, |entry| {
        entry
            .fields
            .retain(|name, _| name == fields::TITLE || name == fields::PASSWORD);
        entry.times.expires = None;
    });
    let stored = f.vault.get_by_id(&key.id).unwrap();
    let bytes = fs::read(&f.path).unwrap();

    f.clock.advance(Duration::days(1));
    let same = f.vault.edit_input(&key.id).unwrap();
    let answer = f.vault.update(&key.id, same).unwrap();

    assert_eq!(answer, stored);
    assert!(
        fs::read(&f.path).unwrap() == bytes,
        "the vault file was written"
    );
}

/// FX-02: a key of another program has no creation time. An edit of its notes changes the
/// notes, and the key still has no creation time in the file.
#[test]
fn an_update_gives_no_creation_time_to_a_key_that_has_none() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_edit(&key.id, |entry| entry.times.creation = None);
    assert_eq!(f.entry(&key.id).times.creation, None, "the fixture");

    f.clock.advance(Duration::days(1));
    let edit = with_notes(f.vault.edit_input(&key.id).unwrap(), "a note");
    let answer = f.vault.update(&key.id, edit).unwrap();

    assert_eq!(answer.notes.as_deref(), Some("a note"));
    assert_eq!(answer.created_at, None);
    assert_eq!(f.entry(&key.id).times.creation, None);
}

/// D4-01, and contract 13 for a whole entry: another program made a key that holds a title and
/// nothing else. An edit of its notes leaves the entry as it was, except for the notes, the
/// time of the change, AKM's ID field and the history. So the edit invents no secret, no
/// secret-change time and no other fact.
#[test]
fn an_update_of_one_field_changes_that_field_its_stamp_and_the_history_and_nothing_else() {
    let mut f = Fixture::new();
    let key = f.vault.add(input("Alpha", "secret-one")).unwrap();
    f.with_foreign_edit(&key.id, |entry| {
        entry.fields.retain(|name, _| name == fields::TITLE);
        entry.times.creation = None;
        entry.times.expires = None;
    });
    let before = f.entry(&key.id);

    f.clock.advance(Duration::days(1));
    let edit = with_notes(f.vault.edit_input(&key.id).unwrap(), "a note");
    f.vault.update(&key.id, edit).unwrap();

    let after = f.entry(&key.id);
    assert_ne!(
        after.times.last_modification,
        before.times.last_modification
    );
    let mut expected = before;
    expected.set_unprotected(fields::NOTES, "a note");
    expected.set_unprotected(META_ID, &key.id);
    expected.times.last_modification = after.times.last_modification;
    expected.history = after.history.clone();
    assert!(
        after == expected,
        "the edit changed more:
{after:#?}"
    );
}

/// FX-03: `api-key` is a username like any other, and a key with no username has none.
#[test]
fn a_username_reads_back_exactly_and_a_key_without_one_has_none() {
    let mut f = Fixture::new();
    let mut named = input_named("Alpha", "secret-one", "ALPHA_KEY");
    named.username = Some("api-key".into());
    let named = f.vault.add(named).unwrap();
    let plain = f
        .vault
        .add(input_named("Beta", "secret-two", "BETA_KEY"))
        .unwrap();

    let reopened = Vault::open(&f.path, PASSWORD).unwrap();
    let username = |id: &str| reopened.get_by_id(id).unwrap().username;
    assert_eq!(username(&named.id).as_deref(), Some("api-key"));
    assert_eq!(username(&plain.id), None);
    assert_eq!(f.entry(&plain.id).get(fields::USERNAME), Some(""));
}
