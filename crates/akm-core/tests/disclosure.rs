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

fn block_journal(vault_path: &std::path::Path) {
    let journal = akm_core::audit::AuditLog::for_vault(vault_path);
    let _ = std::fs::remove_file(journal.path());
    std::fs::create_dir(journal.path()).unwrap();
}

#[test]
fn a_failed_journal_record_leaves_no_proof_for_a_key_or_a_scope() {
    let (_directory, path, mut vault) = fake_vault();
    let key = vault.add(input()).unwrap();
    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    block_journal(&path);
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
