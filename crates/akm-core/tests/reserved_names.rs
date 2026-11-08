//! Unit 3, finding 3-SA-01: variable names that make a child run code or redirect traffic.
//! Every test uses a fake vault and literal expected values.

use akm_core::model::{reserved_class, ReservedClass};
use akm_core::{ApiKeyInput, Error, Vault};

const PASSWORD: &str = "correct horse battery staple";

fn vault() -> (tempfile::TempDir, Vault) {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::create(directory.path().join("vault.kdbx"), PASSWORD, None).unwrap();
    (directory, vault)
}

fn message(error: Error) -> String {
    error.to_string()
}

/// Each class with the names that the design lists for it. A name outside this table is
/// not reserved unless a prefix matches.
const CLASSES: &[(ReservedClass, &[&str])] = &[
    (
        ReservedClass::AkmInternal,
        &["AKM_PASSWORD", "AKM_TOKEN", "AKM_JOURNAL_ROOT"],
    ),
    (
        ReservedClass::ProcessControl,
        &[
            "PATH",
            "PATHEXT",
            "COMSPEC",
            "SYSTEMROOT",
            "WINDIR",
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
            "PSModulePath",
        ],
    ),
    (
        ReservedClass::StartupHook,
        &[
            "NODE_OPTIONS",
            "NODE_PATH",
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONSTARTUP",
            "PYTHONINSPECT",
            "PYTHONBREAKPOINT",
            "PYTHONUSERBASE",
            "PERL5OPT",
            "PERL5LIB",
            "PERLLIB",
            "RUBYOPT",
            "RUBYLIB",
            "JAVA_TOOL_OPTIONS",
            "_JAVA_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "CLASSPATH",
            "DOTNET_STARTUP_HOOKS",
            "DOTNET_ADDITIONAL_DEPS",
            "COR_PROFILER",
            "COR_PROFILER_PATH",
            "COR_ENABLE_PROFILING",
            "CORECLR_PROFILER",
            "CORECLR_ENABLE_PROFILING",
            "ELECTRON_RUN_AS_NODE",
        ],
    ),
    (
        ReservedClass::HelperProgram,
        &[
            "GIT_SSH_COMMAND",
            "GIT_SSH",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_GLOBAL",
            "GIT_EXEC_PATH",
            "GIT_PROXY_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_TEMPLATE_DIR",
        ],
    ),
    (
        ReservedClass::ShellStartup,
        &["BASH_ENV", "ENV", "PROMPT_COMMAND", "PS4", "SHELLOPTS"],
    ),
    (
        ReservedClass::TrafficRedirect,
        &[
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "FTP_PROXY",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "REQUESTS_CA_BUNDLE",
            "CURL_CA_BUNDLE",
            "NODE_EXTRA_CA_CERTS",
            "NODE_TLS_REJECT_UNAUTHORIZED",
            "GIT_SSL_NO_VERIFY",
            "GIT_SSL_CAINFO",
            "GIT_SSL_CAPATH",
            "NPM_CONFIG_REGISTRY",
            "NPM_CONFIG_PROXY",
            "PIP_INDEX_URL",
            "PIP_EXTRA_INDEX_URL",
        ],
    ),
    (
        ReservedClass::UserLocation,
        &[
            "TEMP",
            "TMP",
            "TMPDIR",
            "USERPROFILE",
            "HOME",
            "HOMEDRIVE",
            "HOMEPATH",
            "APPDATA",
            "LOCALAPPDATA",
        ],
    ),
];

#[test]
fn every_listed_name_is_reserved_in_its_class_in_any_case() {
    for (class, names) in CLASSES {
        for name in *names {
            assert_eq!(reserved_class(name), Some(*class), "{name}");
            assert_eq!(
                reserved_class(&name.to_ascii_lowercase()),
                Some(*class),
                "{name} in lower case"
            );
            assert_eq!(
                reserved_class(&name.to_ascii_uppercase()),
                Some(*class),
                "{name} in upper case"
            );
        }
    }
}

#[test]
fn ordinary_api_key_names_and_look_alikes_are_not_reserved() {
    for name in [
        "OPENAI_API_KEY",
        "STRIPE_SECRET_KEY",
        "GITHUB_TOKEN",
        "NPM_TOKEN",
        "PATH_TOKEN",
        "HOME_ASSISTANT_TOKEN",
        "HOMEBREW_GITHUB_API_TOKEN",
        "TEMPLATE_KEY",
        "TMPL",
        "ENVIRONMENT_NAME",
        "ENV_NAME",
        "PROXY_TOKEN",
        "NO_PROXY_TOKEN_X",
        "GIT_TOKEN",
        "GITHUB_ASKPASS_NOT",
        "PIPELINE_KEY",
        "CORE_KEY",
        "CORRECT_KEY",
        "LDAP_PASSWORD",
        "DYNAMIC_KEY",
        "AKMS_KEY",
        "JAVA_HOME_KEY",
        "PYTHON_KEY",
    ] {
        assert_eq!(reserved_class(name), None, "{name}");
    }
}

#[test]
fn an_import_with_a_reserved_name_is_refused_for_every_class_and_names_only_the_variable() {
    let (_directory, mut vault) = vault();
    for (_, names) in CLASSES {
        for name in names
            .iter()
            .filter(|name| name.chars().all(|c| !c.is_lowercase()))
        {
            let entries = vec![((*name).to_string(), "fake-hostile-value".to_string())];
            let preview = message(
                vault
                    .preview_import_env(&entries, Some("demo"), Some("dev"))
                    .unwrap_err(),
            );
            let applied = message(
                vault
                    .import_env(&entries, Some("demo"), Some("dev"))
                    .unwrap_err(),
            );
            for text in [preview, applied] {
                assert!(text.contains(name), "{text}");
                assert!(text.contains("reserved"), "{text}");
                assert!(!text.contains("fake-hostile-value"), "{text}");
            }
        }
    }
    assert!(vault.list().is_empty());
}

#[test]
fn add_and_edit_refuse_a_reserved_explicit_variable_name_and_a_reserved_derived_one() {
    let (_directory, mut vault) = vault();

    let mut explicit = ApiKeyInput::new("Proxy for the build", "fake-secret");
    explicit.env_var = Some("HTTPS_PROXY".into());
    let text = message(vault.add(explicit).unwrap_err());
    assert!(
        text.contains("HTTPS_PROXY") && text.contains("reserved"),
        "{text}"
    );
    assert!(!text.contains("fake-secret"));

    let derived = ApiKeyInput::new("Home", "fake-secret");
    let text = message(vault.add(derived).unwrap_err());
    assert!(text.contains("HOME") && text.contains("reserved"), "{text}");
    assert!(text.contains("env var"), "{text}");
    assert!(vault.list().is_empty());

    let mut fine = ApiKeyInput::new("Home", "fake-secret");
    fine.env_var = Some("HOME_TOKEN".into());
    let key = vault.add(fine).unwrap();

    let mut edit = vault.edit_input(&key.id).unwrap();
    edit.env_var = Some("PYTHONSTARTUP".into());
    let text = message(vault.update(&key.id, edit).unwrap_err());
    assert!(text.contains("PYTHONSTARTUP"), "{text}");
    assert_eq!(
        vault.get_by_id(&key.id).unwrap().resolved_env_var(),
        "HOME_TOKEN"
    );
}

fn legacy_vault_with(names: &[(&str, &str)]) -> (tempfile::TempDir, Vault) {
    use akm_core::model::{META_ENVIRONMENT, META_ENV_VAR, META_PROJECT};
    use keepass::db::fields;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = keepass::Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "API Keys".into();
    for (title, var) in names {
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, *title);
        entry.set_protected(fields::PASSWORD, "fake-legacy-secret");
        entry.set_unprotected(META_ENV_VAR, *var);
        entry.set_unprotected(META_PROJECT, "demo");
        entry.set_unprotected(META_ENVIRONMENT, "dev");
    }
    db.save(
        &mut std::fs::File::create(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let vault = Vault::open(&path, PASSWORD).unwrap();
    (directory, vault)
}

#[test]
fn a_legacy_key_with_a_reserved_name_is_refused_by_the_plan_with_its_title_and_never_its_value() {
    let (_directory, vault) =
        legacy_vault_with(&[("Build proxy", "HTTP_PROXY"), ("Safe key", "SAFE_TOKEN")]);
    let text = message(
        vault
            .plan_env(Some("demo"), Some("dev"), false)
            .unwrap_err(),
    );
    assert!(text.contains("Build proxy"), "{text}");
    assert!(text.contains("HTTP_PROXY"), "{text}");
    assert!(text.contains("reserved"), "{text}");
    assert!(!text.contains("fake-legacy-secret"), "{text}");
    let text = message(vault.plan_env(None, None, true).unwrap_err());
    assert!(text.contains("Build proxy"), "{text}");
}

#[test]
fn renaming_a_legacy_key_makes_it_usable_again() {
    let (_directory, mut vault) = legacy_vault_with(&[("Build proxy", "HTTP_PROXY")]);
    let key = vault.list().remove(0);
    let mut edit = vault.edit_input(&key.id).unwrap();
    edit.env_var = Some("BUILD_PROXY_TOKEN".into());
    vault.update(&key.id, edit).unwrap();
    let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
    assert_eq!(plan.names().collect::<Vec<_>>(), ["BUILD_PROXY_TOKEN"]);
}
