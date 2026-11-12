//! CORE-R1-02 and FR-02: one function gives every variable name one of three classes.
//! Reserved names are refused everywhere; credential names have a narrow shape; every other
//! name is stored and handed over as before, and the import confirmation can show it apart.
//! Every test uses fake data and literal expected values.

use akm_core::model::{name_class, reserved_class, NameClass, ReservedClass};
use akm_core::Vault;

const PASSWORD: &str = "correct horse battery staple";

/// Each row: a name and its class. The table holds the cc forms of the review, a credential
/// shape on each side of every near miss, and names that are neither.
const TABLE: &[(&str, NameClass)] = &[
    // Reserved: the cc forms of core review 1, finding 2.
    ("HOST_CC", NameClass::Reserved(ReservedClass::HelperProgram)),
    ("TARGET_CC", NameClass::Reserved(ReservedClass::HelperProgram)),
    (
        "CC_x86_64-unknown-linux-gnu",
        NameClass::Reserved(ReservedClass::HelperProgram),
    ),
    (
        "CC_x86_64_unknown_linux_gnu",
        NameClass::Reserved(ReservedClass::HelperProgram),
    ),
    (
        "TARGET_CXXFLAGS_aarch64-apple-darwin",
        NameClass::Reserved(ReservedClass::HelperProgram),
    ),
    // Reserved first: a credential word does not take a reserved name out of its class.
    ("CC_API_KEY", NameClass::Reserved(ReservedClass::HelperProgram)),
    ("GIT_SSL_KEY", NameClass::Reserved(ReservedClass::TrafficRedirect)),
    ("SSH_KEY_PATH", NameClass::Reserved(ReservedClass::ProcessControl)),
    ("PRIVATE_KEY_DIR", NameClass::Reserved(ReservedClass::UserLocation)),
    ("AKM_TOKEN", NameClass::Reserved(ReservedClass::AkmInternal)),
    (
        "CARGO_REGISTRY_TOKEN_FILE",
        NameClass::Reserved(ReservedClass::HelperProgram),
    ),
    ("GIT_ASKPASS", NameClass::Reserved(ReservedClass::HelperProgram)),
    // Credential: a whole token is a credential word.
    ("OPENAI_API_KEY", NameClass::Credential),
    ("STRIPE_SECRET_KEY", NameClass::Credential),
    ("GITHUB_TOKEN", NameClass::Credential),
    ("NPM_TOKEN", NameClass::Credential),
    ("AWS_ACCESS_KEY_ID", NameClass::Credential),
    ("AWS_SECRET_ACCESS_KEY", NameClass::Credential),
    ("DB_PASSWORD", NameClass::Credential),
    ("SMTP_PASSWD", NameClass::Credential),
    ("SSH_PASSPHRASE", NameClass::Credential),
    ("REDIS_PASS", NameClass::Credential),
    ("GOOGLE_APPLICATION_CREDENTIALS", NameClass::Credential),
    ("SERVICE_CREDENTIAL", NameClass::Credential),
    ("AZURE_CLIENT_SECRET", NameClass::Credential),
    ("API_SECRETS", NameClass::Credential),
    ("DENO_AUTH_TOKENS", NameClass::Credential),
    ("SIGNING_KEYS", NameClass::Credential),
    ("OPENAI_APIKEY", NameClass::Credential),
    ("TOKEN", NameClass::Credential),
    ("openai_api_key", NameClass::Credential),
    // Credential: the documented registry tokens inside a reserved family.
    ("CARGO_REGISTRY_TOKEN", NameClass::Credential),
    ("CARGO_REGISTRIES_MINE_TOKEN", NameClass::Credential),
    ("YARN_NPM_AUTH_TOKEN", NameClass::Credential),
    // Credential: a reserved word that is not the last token, or only part of a word.
    ("PATH_TOKEN", NameClass::Credential),
    ("HOME_ASSISTANT_TOKEN", NameClass::Credential),
    ("CCACHE_TOKEN", NameClass::Credential),
    ("HOST_TOKEN", NameClass::Credential),
    ("TARGET_API_KEY", NameClass::Credential),
    ("EDITOR_KEY", NameClass::Credential),
    ("LESSON_KEY", NameClass::Credential),
    ("GIT_TOKEN", NameClass::Credential),
    // Other: no credential token, no reserved rule.
    ("PORT", NameClass::Other),
    ("NODE_ENV", NameClass::Other),
    ("DATABASE_URL", NameClass::Other),
    ("LOG_LEVEL", NameClass::Other),
    ("APP_NAME", NameClass::Other),
    ("DEBUG", NameClass::Other),
    ("TZ", NameClass::Other),
    ("RUST_LOG", NameClass::Other),
    ("AWS_REGION", NameClass::Other),
    ("DOTNET_ENVIRONMENT", NameClass::Other),
    ("YARN_NPM_AUTH_IDENT", NameClass::Other),
    ("FEATURE_FLAGS", NameClass::Other),
    ("SLACK_WEBHOOK", NameClass::Other),
    // Other: a credential word only inside a longer token is not a credential word.
    ("MONKEY_ID", NameClass::Other),
    ("KEYBOARD_LAYOUT", NameClass::Other),
    ("TOKENIZER_MODEL", NameClass::Other),
    ("SECRETARY_NAME", NameClass::Other),
    ("PASSPORT_NUMBER", NameClass::Other),
    ("PASSWORDLESS_LOGIN", NameClass::Other),
    ("BYPASS_CACHE", NameClass::Other),
    ("MY_ASKPASS_HELPER", NameClass::Other),
    ("GITHUB_ASKPASS_NOT", NameClass::Other),
];

#[test]
fn every_name_in_the_table_has_its_class_in_any_case() {
    for (name, class) in TABLE {
        for spelled in [
            (*name).to_string(),
            name.to_ascii_lowercase(),
            name.to_ascii_uppercase(),
        ] {
            assert_eq!(name_class(&spelled), *class, "{spelled}");
        }
    }
}

/// `reserved_class`, which the desktop calls, is the reserved part of `name_class`.
#[test]
fn reserved_class_is_the_reserved_part_of_name_class() {
    for (name, class) in TABLE {
        let expected = match class {
            NameClass::Reserved(reserved) => Some(*reserved),
            NameClass::Credential | NameClass::Other => None,
        };
        assert_eq!(reserved_class(name), expected, "{name}");
    }
}

/// The credential words are whole tokens. A name with none of them is never a credential,
/// whatever else it holds.
#[test]
fn the_credential_shape_is_a_whole_token_and_nothing_else() {
    for word in [
        "KEY",
        "KEYS",
        "APIKEY",
        "TOKEN",
        "TOKENS",
        "SECRET",
        "SECRETS",
        "PASSWORD",
        "PASSWD",
        "PASSPHRASE",
        "PASS",
        "CREDENTIAL",
        "CREDENTIALS",
    ] {
        for name in [
            word.to_string(),
            format!("MY_{word}"),
            format!("{word}_ONE"),
            format!("MY_{word}_ONE"),
        ] {
            assert_eq!(name_class(&name), NameClass::Credential, "{name}");
        }
        for name in [
            format!("MY{word}"),
            format!("{word}X"),
            format!("MY_X{word}_ONE"),
        ] {
            assert_ne!(name_class(&name), NameClass::Credential, "{name}");
        }
    }
}

/// The desktop import confirmation marks the names that are not credential names. Each
/// change of a preview gives its class, and a reserved name never reaches a preview.
#[test]
fn each_change_of_an_import_preview_gives_the_class_of_its_name() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::create(directory.path().join("vault.kdbx"), PASSWORD, None).unwrap();
    let entries: Vec<(String, String)> = [
        ("OPENAI_API_KEY", "fake-openai"),
        ("PORT", "8080"),
        ("DATABASE_URL", "postgres://fake"),
        ("GITHUB_TOKEN", "fake-github"),
    ]
    .iter()
    .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
    .collect();
    let preview = vault
        .preview_import_env(&entries, Some("demo"), Some("dev"))
        .unwrap();
    let classes: Vec<(&str, NameClass)> = preview
        .changes
        .iter()
        .map(|change| (change.name.as_str(), change.name_class()))
        .collect();
    assert_eq!(
        classes,
        [
            ("OPENAI_API_KEY", NameClass::Credential),
            ("PORT", NameClass::Other),
            ("DATABASE_URL", NameClass::Other),
            ("GITHUB_TOKEN", NameClass::Credential),
        ]
    );

    let mut hostile = entries.clone();
    hostile.push(("HOST_CC".to_string(), "fake-compiler".to_string()));
    let text = vault
        .preview_import_env(&hostile, Some("demo"), Some("dev"))
        .unwrap_err()
        .to_string();
    assert!(text.contains("HOST_CC") && text.contains("reserved"), "{text}");
}
