//! Unit 3, finding 3-SA-01: variable names that make a child run code or redirect traffic.
//! Every test uses a fake vault and literal expected values.

use akm_core::model::{reserved_class, ReservedClass};
use akm_core::{dotenv, ApiKeyInput, Error, Vault};

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
            // FR-02: which toolchain, interpreter or runtime a tool picks.
            "RUSTUP_TOOLCHAIN",
            "GOROOT",
            "GOTOOLCHAIN",
            "JAVA_HOME",
            "GEM_HOME",
            "GEM_PATH",
            "UV_PYTHON",
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
            // FR-02: JVM build tools and other runtimes that load code or flags at start.
            "MAVEN_OPTS",
            "GRADLE_OPTS",
            "ANT_OPTS",
            "SBT_OPTS",
            "JAVA_OPTS",
            "CATALINA_OPTS",
            "PYTHONEXECUTABLE",
            "PERL5DB",
            "PHPRC",
            "PHP_INI_SCAN_DIR",
            "LUA_INIT",
            "LUA_PATH",
            "R_PROFILE_USER",
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
            // FR-02: the Rust and Cargo execution controls.
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "RUSTC",
            "RUSTDOC",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "RUSTC_BOOTSTRAP",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_ENCODED_RUSTDOCFLAGS",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTDOC",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTDOCFLAGS",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS",
            "CARGO_REGISTRIES_MINE_CREDENTIAL_PROVIDER",
            "CARGO_REGISTRIES_MINE_INDEX",
            "CARGO_ALIAS_B",
            "CARGO_PROFILE_DEV_RUSTFLAGS",
            "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS",
            // Compilers and build tools.
            "CC",
            "CXX",
            "LD",
            "AR",
            "MAKE",
            "MAKEFLAGS",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "CMAKE_TOOLCHAIN_FILE",
            "CGO_LDFLAGS",
            "GOFLAGS",
            "GOAUTH",
            // Editors, pagers and shells that tools run.
            "EDITOR",
            "VISUAL",
            "PAGER",
            "GIT_PAGER",
            "GIT_EDITOR",
            "GIT_SEQUENCE_EDITOR",
            "BROWSER",
            "SHELL",
            // Other tools that run a configured program.
            "AWS_CONFIG_FILE",
            "DOCKER_CONFIG",
            "TF_CLI_ARGS",
            "TF_CLI_CONFIG_FILE",
            "YARN_YARN_PATH",
        ],
    ),
    (
        ReservedClass::ShellStartup,
        &[
            "BASH_ENV",
            "ENV",
            "PROMPT_COMMAND",
            "PS4",
            "SHELLOPTS",
            "PSEXECUTIONPOLICYPREFERENCE",
            "__PSLOCKDOWNPOLICY",
            "PROMPT",
            "DIRCMD",
            "COPYCMD",
        ],
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
            // FR-02: TLS and crypto controls, and registry or index redirects.
            "SSLKEYLOGFILE",
            "OPENSSL_CONF",
            "OPENSSL_ENGINES",
            "OPENSSL_MODULES",
            "AWS_CA_BUNDLE",
            "AWS_ENDPOINT_URL",
            "AWS_ENDPOINT_URL_S3",
            "GRPC_DEFAULT_SSL_ROOTS_FILE_PATH",
            "GOPROXY",
            "GOSUMDB",
            "UV_INDEX_URL",
            "UV_DEFAULT_INDEX",
            "UV_EXTRA_INDEX_URL",
            "UV_PUBLISH_URL",
            "YARN_NPM_REGISTRY_SERVER",
            "BUN_CONFIG_REGISTRY",
            "RUSTUP_DIST_SERVER",
            "DOCKER_HOST",
            "CARGO_HTTP_PROXY",
            "CARGO_NET_GIT_FETCH_WITH_CLI",
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
            // FR-02: where toolchains and package managers keep their settings and files.
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
            "RUSTUP_HOME",
            "GOPATH",
            "GOBIN",
            "GOENV",
            "BUN_INSTALL",
            "GRADLE_USER_HOME",
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
        "CARGOTOKEN",
        "RUSTCRATES_TOKEN",
        "CCACHE_TOKEN",
        "MAKER_KEY",
        "EDITOR_KEY",
        "SHELL_TOKEN",
        "GOPRO_TOKEN",
        "OPENSSLX_KEY",
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

/// FR-02: the Cargo and Rust controls are reserved in any case, and so is every Cargo target
/// setting that runs a program or passes flags, for any target triple.
#[test]
fn every_cargo_target_runner_linker_and_flags_setting_is_reserved_for_any_triple() {
    for triple in [
        "X86_64_PC_WINDOWS_MSVC",
        "AARCH64_UNKNOWN_LINUX_GNU",
        "I686_PC_WINDOWS_GNU",
        "THUMBV7EM_NONE_EABIHF",
        "WASM32_UNKNOWN_UNKNOWN",
    ] {
        for suffix in ["RUNNER", "LINKER", "RUSTFLAGS", "AR"] {
            let name = format!("CARGO_TARGET_{triple}_{suffix}");
            for spelled in [name.clone(), name.to_ascii_lowercase(), mixed_case(&name)] {
                assert_eq!(
                    reserved_class(&spelled),
                    Some(ReservedClass::HelperProgram),
                    "{spelled}"
                );
            }
        }
    }
}

fn mixed_case(name: &str) -> String {
    name.chars()
        .enumerate()
        .map(|(index, ch)| {
            if index % 2 == 0 {
                ch.to_ascii_lowercase()
            } else {
                ch.to_ascii_uppercase()
            }
        })
        .collect()
}

/// FR-02: variables that carry a secret for a registry or a cloud stay usable. A fix that
/// reserved a whole prefix without this list would break the keys that AKM is for.
#[test]
fn registry_and_cloud_credential_names_stay_allowed() {
    for name in [
        "CARGO_REGISTRY_TOKEN",
        "CARGO_REGISTRIES_MY_REGISTRY_TOKEN",
        "CARGO_REGISTRIES_CRATES_IO_TOKEN",
        "cargo_registry_token",
        "Cargo_Registries_Mine_Token",
        "UV_PUBLISH_TOKEN",
        "UV_INDEX_INTERNAL_PASSWORD",
        "YARN_NPM_AUTH_TOKEN",
        "DENO_AUTH_TOKENS",
        "NPM_TOKEN",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "GOOGLE_API_KEY",
        "OPENAI_API_KEY",
    ] {
        assert_eq!(reserved_class(name), None, "{name}");
    }
    // The allowance is for the token itself, not for the rest of the family.
    for name in [
        "CARGO_REGISTRIES_MINE_INDEX",
        "CARGO_REGISTRIES__TOKEN_X",
        "CARGO_REGISTRIES_TOKEN",
        "CARGO_REGISTRY_TOKEN_FILE",
        "CARGO_REGISTRY_DEFAULT",
    ] {
        assert!(reserved_class(name).is_some(), "{name}");
    }

    // And the real paths accept them: add, and an import into a scope.
    let (_directory, mut vault) = vault();
    let mut input = ApiKeyInput::new("Crates token", "fake-crates-token");
    input.env_var = Some("CARGO_REGISTRY_TOKEN".into());
    vault.add(input).unwrap();
    let entries = vec![
        ("UV_PUBLISH_TOKEN".to_string(), "fake-uv".to_string()),
        ("YARN_NPM_AUTH_TOKEN".to_string(), "fake-yarn".to_string()),
        ("DENO_AUTH_TOKENS".to_string(), "fake-deno".to_string()),
        ("NPM_TOKEN".to_string(), "fake-npm".to_string()),
        (
            "CARGO_REGISTRIES_MINE_TOKEN".to_string(),
            "fake-mine".to_string(),
        ),
    ];
    assert_eq!(
        vault
            .import_env(&entries, Some("demo"), Some("dev"))
            .unwrap(),
        (5, 0)
    );
}

/// FR-02, through the real paths: `add` (a derived and an explicit name), a `.env` import
/// and an edit. Lower and mixed case spellings are either reserved (a derived name is
/// upper-cased) or refused as invalid names; none is stored.
#[test]
fn rustc_wrapper_is_refused_by_add_import_and_edit_in_any_case() {
    let (_directory, mut vault) = vault();

    for title in [
        "RUSTC_WRAPPER",
        "rustc_wrapper",
        "RustC_Wrapper",
        "rustc wrapper",
    ] {
        let text = message(
            vault
                .add(ApiKeyInput::new(title, "fake-secret"))
                .unwrap_err(),
        );
        assert!(
            text.contains("RUSTC_WRAPPER") && text.contains("reserved"),
            "{title}: {text}"
        );
        assert!(!text.contains("fake-secret"));
    }
    for env_var in ["RUSTC_WRAPPER", "rustc_wrapper", "RustC_Wrapper"] {
        let mut input = ApiKeyInput::new("A harmless title", "fake-secret");
        input.env_var = Some(env_var.into());
        let text = message(vault.add(input).unwrap_err());
        assert!(
            text.contains(env_var) && (text.contains("reserved") || text.contains("not a valid")),
            "{env_var}: {text}"
        );
    }

    for document in [
        "RUSTC_WRAPPER=fake-evil\n",
        "export RUSTC_WRAPPER=fake-evil\n",
        "RUSTC_WORKSPACE_WRAPPER='fake-evil'\n",
        "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER=fake-evil\n",
        "RUSTFLAGS=fake-evil\n",
        // Lower and mixed case do not pass the dotenv profile.
        "rustc_wrapper=fake-evil\n",
        "RustC_Wrapper=fake-evil\n",
    ] {
        let refused = dotenv::parse(document)
            .and_then(|entries| {
                vault
                    .import_env(&entries, Some("demo"), Some("dev"))
                    .map(|_| ())
            })
            .unwrap_err();
        let text = message(refused);
        assert!(!text.contains("fake-evil"), "{document}: {text}");
    }
    // The preview names the variable and the reason, for the names that pass the profile.
    let entries = dotenv::parse("RUSTC_WRAPPER=fake-evil\n").unwrap();
    let text = message(
        vault
            .preview_import_env(&entries, Some("demo"), Some("dev"))
            .unwrap_err(),
    );
    assert!(
        text.contains("RUSTC_WRAPPER") && text.contains("reserved"),
        "{text}"
    );

    let key = vault
        .add(ApiKeyInput::new("Ordinary key", "fake-secret"))
        .unwrap();
    for env_var in ["RUSTC_WRAPPER", "CARGO_HOME", "RUSTUP_TOOLCHAIN"] {
        let mut edit = vault.edit_input(&key.id).unwrap();
        edit.env_var = Some(env_var.into());
        let text = message(vault.update(&key.id, edit).unwrap_err());
        assert!(
            text.contains(env_var) && text.contains("reserved"),
            "{text}"
        );
    }
    assert_eq!(vault.list().len(), 1, "only the ordinary key was stored");
}

/// FR-02: a key that was stored before a name became reserved. `plan_env` (so `run` and
/// `export`) refuses it with the title, the variable and what to do; the key stays listed,
/// its secret can be read and rotated, and a rename makes it usable.
#[test]
fn a_key_stored_under_a_newly_reserved_name_is_refused_for_use_but_can_be_viewed_and_renamed() {
    for name in [
        "RUSTC_WRAPPER",
        "RustC_Wrapper",
        "rustc_wrapper",
        "CARGO_HOME",
    ] {
        let (_directory, mut vault) =
            legacy_vault_with(&[("Build wrapper", name), ("Safe key", "SAFE_TOKEN")]);
        let text = message(
            vault
                .plan_env(Some("demo"), Some("dev"), false)
                .unwrap_err(),
        );
        assert!(text.contains("Build wrapper"), "{name}: {text}");
        assert!(text.contains(name), "{name}: {text}");
        assert!(
            text.contains("reserved") || text.contains("not a valid"),
            "{name}: {text}"
        );
        assert!(!text.contains("fake-legacy-secret"), "{name}: {text}");
        assert!(
            text.contains("env var"),
            "the message says what to change: {text}"
        );

        let key = vault
            .list()
            .into_iter()
            .find(|key| key.title == "Build wrapper")
            .expect("the key stays listed");
        assert_eq!(
            vault.get_by_id(&key.id).unwrap().env_var.as_deref(),
            Some(name)
        );
        let proof = vault
            .disclose_key(akm_core::KeyDisclosure::Reveal, &key, Some("test"))
            .unwrap();
        assert_eq!(
            *vault.secret(&key.id, &proof).unwrap(),
            "fake-legacy-secret",
            "{name}: the secret can be viewed"
        );
        vault
            .rotate(&key.id, akm_core::Secret::new("fake-rotated-secret"))
            .expect("the secret can be rotated");

        let mut edit = vault.edit_input(&key.id).unwrap();
        edit.env_var = Some("BUILD_WRAPPER_TOKEN".into());
        vault.update(&key.id, edit).unwrap();
        let plan = vault.plan_env(Some("demo"), Some("dev"), false).unwrap();
        assert_eq!(
            plan.names().collect::<Vec<_>>(),
            ["BUILD_WRAPPER_TOKEN", "SAFE_TOKEN"],
            "{name}"
        );
    }
}
