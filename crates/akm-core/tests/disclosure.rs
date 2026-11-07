//! Unit 1h (1H-04, L1): no key, input or listing carries a secret into `Debug` or
//! `serde`, and every sink needs a proof that the journal recorded its request.
use akm_core::{ApiKeyInput, EnvDisclosure, KeyDisclosure, Vault};

const PASSWORD: &str = "fake-disclosure-password";
const SECRET: &str = "fake-disclosure-secret-4711";

fn fake_vault() -> (tempfile::TempDir, std::path::PathBuf, Vault) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let vault = Vault::create(&path, PASSWORD, None).unwrap();
    (directory, path, vault)
}

fn input() -> ApiKeyInput {
    let mut input = ApiKeyInput::new("Demo", SECRET);
    input.project = Some("demo".into());
    input.environment = Some("dev".into());
    input.env_var = Some("DEMO_TOKEN".into());
    input
}

#[test]
fn debug_output_of_keys_and_inputs_holds_no_secret_bytes() {
    let (_directory, _path, mut vault) = fake_vault();
    let added = vault.add(input()).unwrap();
    let listed = vault.list();
    let found = vault.find("demo").unwrap();
    let by_id = vault.get_by_id(&added.id).unwrap();
    let expiring = vault.expiring(36500);
    for (what, text) in [
        ("added", format!("{added:?} {added:#?}")),
        ("listed", format!("{listed:?}")),
        ("found", format!("{found:?}")),
        ("by id", format!("{by_id:?}")),
        ("expiring", format!("{expiring:?}")),
        ("input", format!("{:?} {:#?}", input(), input())),
    ] {
        assert!(!text.contains(SECRET), "{what} Debug shows the secret");
    }
}

#[test]
fn json_of_keys_and_inputs_holds_no_secret_bytes() {
    let (_directory, _path, mut vault) = fake_vault();
    let added = vault.add(input()).unwrap();
    let texts = [
        serde_json::to_string(&added).unwrap(),
        serde_json::to_string_pretty(&vault.list()).unwrap(),
        serde_json::to_string(&vault.find("demo").unwrap()).unwrap(),
        serde_json::to_string(&input()).unwrap(),
    ];
    for text in texts {
        assert!(!text.contains(SECRET), "{text}");
    }
}

fn block_journal(vault: &akm_core::Vault) {
    let journal = vault.audit_log().create_journal_path().unwrap();
    let _ = std::fs::remove_file(&journal);
    std::fs::create_dir(&journal).unwrap();
}

#[test]
fn a_failed_journal_record_leaves_no_proof_for_a_key_or_a_scope() {
    let (_directory, _path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    block_journal(&vault);
    for kind in [KeyDisclosure::Reveal, KeyDisclosure::Copy] {
        assert!(vault.disclose_key(kind, &key, None).is_err(), "{kind:?}");
    }
    for kind in [EnvDisclosure::Export, EnvDisclosure::Run] {
        assert!(vault.disclose_env(kind, &plan, None).is_err(), "{kind:?}");
    }
}

#[test]
fn a_proof_covers_only_the_key_or_the_selection_that_was_recorded() {
    let (_directory, path, mut vault) = fake_vault();
    let first = vault.add(input()).unwrap();
    let mut other = input();
    other.title = "Other".into();
    other.secret = "fake-other-secret".into();
    other.env_var = Some("OTHER_TOKEN".into());
    other.project = Some("other".into());
    let second = vault.add(other).unwrap();

    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &first, Some("test"))
        .unwrap();
    assert_eq!(&*vault.secret(&first.id, &proof).unwrap(), SECRET);
    assert!(vault.secret(&second.id, &proof).is_err());

    let demo = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    let all = vault.plan_env(None, None, true).unwrap();
    assert_eq!(demo.len(), 1);
    assert_eq!(all.len(), 2);
    let env_proof = vault
        .disclose_env(EnvDisclosure::Export, &demo, Some("test"))
        .unwrap();
    assert!(vault.env_vars(&all, &env_proof).is_err());
    assert!(vault.env_vars(&demo, &proof).is_err(), "a key proof");
    assert!(
        vault.secret(&first.id, &env_proof).is_err(),
        "a scope proof"
    );
    assert_eq!(
        vault.env_vars(&demo, &env_proof).unwrap(),
        vec![("DEMO_TOKEN".to_string(), SECRET.to_string())]
    );

    let actions: Vec<String> = akm_core::audit::AuditLog::for_vault(&path)
        .read(100)
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    assert!(actions.iter().any(|action| action == "reveal-requested"));
    assert!(actions.iter().any(|action| action == "export-requested"));
}

#[test]
fn a_plan_and_an_update_input_never_print_the_secret() {
    let (_directory, _path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    assert!(!format!("{plan:?}").contains(SECRET));
    let kept = vault.edit_input(&key.id).unwrap();
    assert!(!format!("{kept:?}").contains(SECRET));
    assert!(!serde_json::to_string(&kept).unwrap().contains(SECRET));
    let updated = vault.update(&key.id, kept).unwrap();
    assert_eq!(updated.title, "Demo");
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &updated, None)
        .unwrap();
    assert_eq!(&*vault.secret(&key.id, &proof).unwrap(), SECRET);
}

#[test]
fn candidate_lists_and_the_recycle_bin_hold_no_secret_bytes() {
    let (_directory, _path, mut vault) = fake_vault();
    let added = vault.add(input()).unwrap();
    let live = vault.candidates("dem");
    assert_eq!(live.len(), 1);
    vault.remove(&added.id).unwrap();
    let recycled = vault.recycled_candidates("dem");
    assert_eq!(recycled.len(), 1);
    for text in [
        format!("{live:?}"),
        serde_json::to_string(&live).unwrap(),
        format!("{recycled:?}"),
        serde_json::to_string(&recycled).unwrap(),
    ] {
        assert!(!text.contains(SECRET), "{text}");
    }
}

// ----------------- review 1, finding 3: a proof belongs to the vault session that recorded it

mod bound {
    use super::*;
    use akm_core::journal::JournalRoot;
    use akm_core::{ApiKey, Error};

    pub struct Pair {
        pub _directory: tempfile::TempDir,
        pub a: Vault,
        pub b: Vault,
        pub key: ApiKey,
    }

    /// Vault B is a byte copy of vault A, so both hold the same key IDs. Each has its own
    /// journal folder.
    pub fn clones() -> Pair {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("a.kdbx");
        let mut a = Vault::create_in(
            &first,
            PASSWORD,
            None,
            JournalRoot::at(directory.path().join("journal-a")),
        )
        .unwrap();
        let key = a.add(input()).unwrap();
        let second = directory.path().join("b.kdbx");
        std::fs::copy(&first, &second).unwrap();
        let b = Vault::open_in(
            &second,
            PASSWORD,
            JournalRoot::at(directory.path().join("journal-b")),
        )
        .unwrap();
        Pair {
            _directory: directory,
            a,
            b,
            key,
        }
    }

    pub fn message(error: Error) -> String {
        error
            .to_string()
            .trim_start_matches("invalid input: ")
            .to_string()
    }
}

#[test]
fn a_vault_records_no_disclosure_for_a_key_it_does_not_hold() {
    let pair = bound::clones();
    let stranger = tempfile::tempdir().unwrap();
    let other = akm_core::journal::JournalRoot::at(stranger.path().join("journal"));
    let mut elsewhere =
        Vault::create_in(stranger.path().join("c.kdbx"), PASSWORD, None, other).unwrap();
    let foreign = elsewhere.add(input()).unwrap();
    assert_ne!(foreign.id, pair.key.id);

    let before = pair.a.audit_log().read(100).unwrap().len();
    let error = pair
        .a
        .disclose_key(KeyDisclosure::Reveal, &foreign, None)
        .unwrap_err();
    assert!(
        matches!(error, akm_core::Error::EntryNotFound(_)),
        "{error}"
    );
    let after = pair.a.audit_log().read(100).unwrap().len();
    assert_eq!(
        before, after,
        "no request was recorded for a stranger's key"
    );
}

#[test]
fn a_proof_from_one_vault_never_discloses_from_another_vault_with_the_same_key_id() {
    let pair = bound::clones();
    block_journal(&pair.b);
    let proof = pair
        .a
        .disclose_key(KeyDisclosure::Reveal, &pair.key, Some("test"))
        .unwrap();
    let error = pair.b.secret(&pair.key.id, &proof).unwrap_err();
    assert_eq!(
        bound::message(error),
        "this disclosure record was issued for another vault session"
    );
    assert!(pair
        .b
        .disclose_key(KeyDisclosure::Reveal, &pair.key, None)
        .is_err());
}

#[test]
fn a_scope_proof_and_a_scope_plan_from_one_vault_never_work_on_another_vault() {
    let pair = bound::clones();
    block_journal(&pair.b);
    let plan_a = pair.a.plan_env(Some("demo"), Some("dev"), false).unwrap();
    let plan_b = pair.b.plan_env(Some("demo"), Some("dev"), false).unwrap();
    let proof = pair
        .a
        .disclose_env(EnvDisclosure::Export, &plan_a, Some("test"))
        .unwrap();
    assert_eq!(
        bound::message(pair.b.env_vars(&plan_b, &proof).unwrap_err()),
        "this disclosure record was issued for another vault session"
    );
    assert_eq!(
        bound::message(pair.b.env_vars(&plan_a, &proof).unwrap_err()),
        "this selection was planned for another vault session or an earlier state of it"
    );
    assert_eq!(
        bound::message(
            pair.b
                .disclose_env(EnvDisclosure::Run, &plan_a, None)
                .unwrap_err()
        ),
        "this selection was planned for another vault session or an earlier state of it"
    );
    assert_eq!(
        pair.a.env_vars(&plan_a, &proof).unwrap(),
        vec![("DEMO_TOKEN".to_string(), SECRET.to_string())],
        "the vault that recorded it still can"
    );
}

#[test]
fn a_proof_is_used_once_for_a_key_and_for_a_scope() {
    let (_directory, _path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    assert_eq!(&*vault.secret(&key.id, &proof).unwrap(), SECRET);
    assert_eq!(
        bound::message(vault.secret(&key.id, &proof).unwrap_err()),
        "this disclosure record was already used"
    );
    let again = vault.disclose_key(KeyDisclosure::Copy, &key, None).unwrap();
    assert!(
        vault.secret(&key.id, &proof).is_err(),
        "a new request does not revive an old record"
    );
    assert_eq!(&*vault.secret(&key.id, &again).unwrap(), SECRET);

    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    let scope = vault.disclose_env(EnvDisclosure::Run, &plan, None).unwrap();
    assert_eq!(vault.env_vars(&plan, &scope).unwrap().len(), 1);
    assert_eq!(
        bound::message(vault.env_vars(&plan, &scope).unwrap_err()),
        "this disclosure record was already used"
    );
}

#[test]
fn a_refused_presentation_does_not_use_up_the_proof() {
    let (_directory, _path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    assert!(vault.secret("not-a-key-id", &proof).is_err());
    assert_eq!(&*vault.secret(&key.id, &proof).unwrap(), SECRET);
}

#[test]
fn a_proof_and_a_plan_do_not_survive_a_change_of_the_vault() {
    let (_directory, path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    let scope = vault
        .disclose_env(EnvDisclosure::Export, &plan, None)
        .unwrap();
    vault
        .rotate(&key.id, akm_core::Secret::new("fake-rotated-secret"))
        .unwrap();
    assert_eq!(
        bound::message(vault.secret(&key.id, &proof).unwrap_err()),
        "this disclosure record was issued before the vault changed"
    );
    assert!(vault.env_vars(&plan, &scope).is_err());
    assert_eq!(
        bound::message(
            vault
                .disclose_env(EnvDisclosure::Export, &plan, None)
                .unwrap_err()
        ),
        "this selection was planned for another vault session or an earlier state of it"
    );

    let fresh = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    assert_eq!(
        &*vault.secret(&key.id, &fresh).unwrap(),
        "fake-rotated-secret"
    );

    let late = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    vault.reload().unwrap();
    assert!(vault.secret(&key.id, &late).is_err(), "reload is a change");
    drop(path);
}

#[test]
fn a_proof_from_an_earlier_session_of_the_same_file_is_refused() {
    let (_directory, path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, None)
        .unwrap();
    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(
        bound::message(reopened.secret(&key.id, &proof).unwrap_err()),
        "this disclosure record was issued for another vault session"
    );
}
