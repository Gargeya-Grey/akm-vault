//! FR-02, CORE-R1-02 and CORE-R2-02: one function, `reserved_class`, decides from a variable
//! name whether AKM keeps it out of a child process. A name that no rule reserves is free.
//! AKM has no credential class: every free name is stored, imported, planned and handed over
//! in the same way, whatever words it holds. Every test uses fake data and literal expected
//! values.

use akm_core::model::{reserved_class, ReservedClass};
use akm_core::{ApiKeyInput, EnvDisclosure, Vault};

const PASSWORD: &str = "correct horse battery staple";

/// Names that a reserved rule covers: the cc forms of core review 1, a credential word inside
/// a reserved name, and the file locations and endpoints of core review 2.
const RESERVED: &[(&str, ReservedClass)] = &[
    ("HOST_CC", ReservedClass::HelperProgram),
    ("TARGET_CC", ReservedClass::HelperProgram),
    ("CC_x86_64-unknown-linux-gnu", ReservedClass::HelperProgram),
    ("CC_x86_64_unknown_linux_gnu", ReservedClass::HelperProgram),
    (
        "TARGET_CXXFLAGS_aarch64-apple-darwin",
        ReservedClass::HelperProgram,
    ),
    ("CC_API_KEY", ReservedClass::HelperProgram),
    ("GIT_SSL_KEY", ReservedClass::TrafficRedirect),
    ("SSH_KEY_PATH", ReservedClass::ProcessControl),
    ("PRIVATE_KEY_DIR", ReservedClass::UserLocation),
    ("AKM_TOKEN", ReservedClass::AkmInternal),
    ("CARGO_REGISTRY_TOKEN_FILE", ReservedClass::HelperProgram),
    ("GIT_ASKPASS", ReservedClass::HelperProgram),
    // CORE-R2-02: a credential word does not free a file location or an endpoint.
    (
        "GOOGLE_APPLICATION_CREDENTIALS",
        ReservedClass::SettingsFile,
    ),
    ("KUBECONFIG", ReservedClass::SettingsFile),
    ("AWS_SHARED_CREDENTIALS_FILE", ReservedClass::SettingsFile),
    ("AWS_WEB_IDENTITY_TOKEN_FILE", ReservedClass::SettingsFile),
    ("AZURE_FEDERATED_TOKEN_FILE", ReservedClass::SettingsFile),
    ("DB_PASSWORD_FILE", ReservedClass::SettingsFile),
    ("APP_CONFIG", ReservedClass::SettingsFile),
    (
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        ReservedClass::TrafficRedirect,
    ),
    ("AZURE_AUTHORITY_HOST", ReservedClass::TrafficRedirect),
];

/// Free names: credential words, plain settings, near misses of a reserved shape, and words
/// that only hold a reserved word or a credential word.
const FREE: &[&str] = &[
    "OPENAI_API_KEY",
    "STRIPE_SECRET_KEY",
    "GITHUB_TOKEN",
    "NPM_TOKEN",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "DB_PASSWORD",
    "SMTP_PASSWD",
    "SSH_PASSPHRASE",
    "REDIS_PASS",
    "SERVICE_CREDENTIAL",
    "AZURE_CLIENT_SECRET",
    "API_SECRETS",
    "DENO_AUTH_TOKENS",
    "SIGNING_KEYS",
    "OPENAI_APIKEY",
    "TOKEN",
    "CARGO_REGISTRY_TOKEN",
    "CARGO_REGISTRIES_MINE_TOKEN",
    "YARN_NPM_AUTH_TOKEN",
    "PATH_TOKEN",
    "HOME_ASSISTANT_TOKEN",
    "CCACHE_TOKEN",
    "HOST_TOKEN",
    "TARGET_API_KEY",
    "EDITOR_KEY",
    "LESSON_KEY",
    "GIT_TOKEN",
    "FILE_TOKEN",
    "CONFIG_KEY",
    "PORT",
    "NODE_ENV",
    "DATABASE_URL",
    "LOG_LEVEL",
    "APP_NAME",
    "DEBUG",
    "TZ",
    "RUST_LOG",
    "AWS_REGION",
    "AWS_PROFILE",
    "DOTNET_ENVIRONMENT",
    "YARN_NPM_AUTH_IDENT",
    "FEATURE_FLAGS",
    "SLACK_WEBHOOK",
    "MONKEY_ID",
    "KEYBOARD_LAYOUT",
    "TOKENIZER_MODEL",
    "SECRETARY_NAME",
    "PASSPORT_NUMBER",
    "PASSWORDLESS_LOGIN",
    "BYPASS_CACHE",
    "MY_ASKPASS_HELPER",
    "GITHUB_ASKPASS_NOT",
    "USER_PROFILE",
    "LOG_FILES",
    "APP_CONFIGS",
    "PROFILE",
];

fn spellings(name: &str) -> [String; 3] {
    let mixed = name
        .chars()
        .enumerate()
        .map(|(index, letter)| {
            if index % 2 == 0 {
                letter.to_ascii_lowercase()
            } else {
                letter.to_ascii_uppercase()
            }
        })
        .collect();
    [name.to_string(), name.to_ascii_lowercase(), mixed]
}

#[test]
fn every_reserved_row_has_its_class_and_every_free_name_has_none_in_any_case() {
    for (name, class) in RESERVED {
        for spelled in spellings(name) {
            assert_eq!(reserved_class(&spelled), Some(*class), "{spelled}");
        }
    }
    for name in FREE {
        for spelled in spellings(name) {
            assert_eq!(reserved_class(&spelled), None, "{spelled}");
        }
    }
}

/// CORE-R2-02: a name with a credential word gets no other treatment than any free name.
/// Add, edit, the import preview, the import, the plan of `run` and of `export`, and the
/// hand-over of the values treat each of these names in the same way.
#[test]
fn a_credential_word_gives_a_name_the_same_treatment_as_any_free_name() {
    let names = ["OPENAI_API_KEY", "DB_PASSWORD", "PORT", "DATABASE_URL"];
    let directory = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(directory.path().join("vault.kdbx"), PASSWORD, None).unwrap();

    let mut previews = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let mut input = ApiKeyInput::new(format!("Added {name}"), format!("fake-added-{name}"));
        input.env_var = Some(name.into());
        input.project = Some("added".into());
        input.environment = Some("dev".into());
        assert_eq!(vault.add(input).unwrap().resolved_env_var(), name);

        let mut input = ApiKeyInput::new("Edited key", format!("fake-edited-{name}"));
        input.project = Some(format!("edited-{index}"));
        input.environment = Some("dev".into());
        let edited = vault.add(input).unwrap();
        assert_eq!(edited.resolved_env_var(), "EDITED_KEY");
        let mut edit = vault.edit_input(&edited.id).unwrap();
        edit.env_var = Some(name.into());
        assert_eq!(
            vault.update(&edited.id, edit).unwrap().resolved_env_var(),
            name
        );

        let entries = vec![(name.to_string(), format!("fake-imported-{name}"))];
        let preview = vault
            .preview_import_env(&entries, Some("imported"), Some("dev"))
            .unwrap();
        assert_eq!(preview.changes.len(), 1, "{name}");
        assert_eq!(preview.changes[0].name, name);
        previews.push((
            preview.added,
            preview.updated,
            preview.unchanged,
            preview.changes[0].action.clone(),
        ));
        assert_eq!(
            vault
                .import_env(&entries, Some("imported"), Some("dev"))
                .unwrap(),
            (1, 0),
            "{name}"
        );
    }
    assert!(
        previews.windows(2).all(|pair| pair[0] == pair[1]),
        "{previews:?}"
    );

    for (project, value) in [("added", "fake-added-"), ("imported", "fake-imported-")] {
        for action in [EnvDisclosure::Run, EnvDisclosure::Export] {
            let plan = vault.plan_env(Some(project), Some("dev"), false).unwrap();
            let proof = vault.disclose_env(action, &plan, Some("test")).unwrap();
            let mut handed = vault.env_vars(&plan, &proof).unwrap();
            handed.sort();
            let mut expected: Vec<(String, String)> = names
                .iter()
                .map(|name| (name.to_string(), format!("{value}{name}")))
                .collect();
            expected.sort();
            assert_eq!(handed, expected, "{project} {action:?}");
        }
    }
}
