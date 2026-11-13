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

/// CORE-R1-02: the cc crate reads each of its variables plain, with a `HOST_` or `TARGET_`
/// prefix, and with a target suffix in the hyphen and the underscore spelling
/// (https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables). The
/// cmake and pkg-config crates read theirs the same way. Every form is reserved, in any case.
#[test]
fn the_build_variable_shape_is_reserved_with_host_and_target_prefixes_and_target_suffixes() {
    let bases = [
        "CC",
        "CXX",
        "AR",
        "RANLIB",
        "NVCC",
        "CXXSTDLIB",
        "CFLAGS",
        "CXXFLAGS",
        "ARFLAGS",
        "RANLIBFLAGS",
        "CMAKE",
        "CMAKE_TOOLCHAIN_FILE",
        "CMAKE_GENERATOR",
        "PKG_CONFIG",
        "PKG_CONFIG_PATH",
        "PKG_CONFIG_SYSROOT_DIR",
    ];
    let suffixes = [
        "",
        "_x86_64-unknown-linux-gnu",
        "_x86_64_unknown_linux_gnu",
        "_aarch64-pc-windows-msvc",
        "_wasm32-wasip1",
        "_thumbv7em_none_eabihf",
    ];
    for base in bases {
        for prefix in ["", "HOST_", "TARGET_"] {
            for suffix in suffixes {
                let name = format!("{prefix}{base}{suffix}");
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
    // The settings of the cc crate itself, which share the CC base.
    for name in [
        "CC_KNOWN_WRAPPER_CUSTOM",
        "CC_MASM_ASM",
        "CC_ENABLE_DEBUG_OUTPUT",
        "CC_SHELL_ESCAPED_FLAGS",
        "CXXSTDLIB_STATIC",
        "CROSS_COMPILE",
    ] {
        assert_eq!(
            reserved_class(name),
            Some(ReservedClass::HelperProgram),
            "{name}"
        );
    }
    // Near misses: the base is only a part of a longer word.
    for name in [
        "CCACHE_TOKEN",
        "ARC_KEY",
        "HOSTING_KEY",
        "TARGETED_ADS_KEY",
        "CMAKER_TOKEN",
        "PKG_CONFIGURATOR_KEY",
        "HOST_TOKEN",
        "TARGET_API_KEY",
    ] {
        assert_eq!(reserved_class(name), None, "{name}");
    }
}

/// CORE-R1-02: at least one name per family that the table holds, newly covered after a
/// check of the documentation of that tool, with its class.
const NEWLY_COVERED: &[(&str, ReservedClass)] = &[
    // Rust and Cargo.
    ("CARGO", ReservedClass::HelperProgram),
    ("RUSTFMT", ReservedClass::HelperProgram),
    // The MSVC compiler and linker that cc runs on Windows.
    ("CL", ReservedClass::HelperProgram),
    ("_CL_", ReservedClass::HelperProgram),
    ("LINK", ReservedClass::HelperProgram),
    ("_LINK_", ReservedClass::HelperProgram),
    ("INCLUDE", ReservedClass::ProcessControl),
    ("LIB", ReservedClass::ProcessControl),
    ("LIBPATH", ReservedClass::ProcessControl),
    // Git.
    ("GIT_DIR", ReservedClass::UserLocation),
    ("GIT_WORK_TREE", ReservedClass::UserLocation),
    ("GIT_INDEX_FILE", ReservedClass::UserLocation),
    ("GIT_OBJECT_DIRECTORY", ReservedClass::UserLocation),
    ("GIT_CONFIG", ReservedClass::HelperProgram),
    ("GIT_SSH_VARIANT", ReservedClass::HelperProgram),
    ("GIT_ALLOW_PROTOCOL", ReservedClass::HelperProgram),
    ("GIT_SSL_KEY", ReservedClass::TrafficRedirect),
    ("GIT_PROXY_SSL_CAINFO", ReservedClass::TrafficRedirect),
    ("GIT_TRACE", ReservedClass::OutputCapture),
    ("GIT_TRACE_CURL", ReservedClass::OutputCapture),
    ("GIT_TRACE2_EVENT", ReservedClass::OutputCapture),
    ("GIT_REDIRECT_STDERR", ReservedClass::OutputCapture),
    // Go.
    ("GCCGO", ReservedClass::HelperProgram),
    ("GOCACHEPROG", ReservedClass::HelperProgram),
    ("GOVCS", ReservedClass::HelperProgram),
    ("GO_EXTLINK_ENABLED", ReservedClass::HelperProgram),
    ("FC", ReservedClass::HelperProgram),
    ("GOWORK", ReservedClass::UserLocation),
    ("GOPRIVATE", ReservedClass::TrafficRedirect),
    ("GODEBUG", ReservedClass::TrafficRedirect),
    // Node and Yarn.
    ("NODE_REPL_EXTERNAL_MODULE", ReservedClass::StartupHook),
    ("NODE_COMPILE_CACHE", ReservedClass::StartupHook),
    ("NODE_REDIRECT_WARNINGS", ReservedClass::OutputCapture),
    ("NODE_V8_COVERAGE", ReservedClass::OutputCapture),
    ("NODE_USE_ENV_PROXY", ReservedClass::TrafficRedirect),
    ("NODE_USE_SYSTEM_CA", ReservedClass::TrafficRedirect),
    ("YARN_ENABLE_SCRIPTS", ReservedClass::HelperProgram),
    ("YARN_HTTPS_CA_FILE_PATH", ReservedClass::HelperProgram),
    // Python and uv.
    ("PYTHONWARNINGS", ReservedClass::StartupHook),
    ("PYTHON_PRESITE", ReservedClass::StartupHook),
    ("PYTHON_HISTORY", ReservedClass::OutputCapture),
    ("UV_PROJECT_ENVIRONMENT", ReservedClass::ProcessControl),
    ("VIRTUAL_ENV", ReservedClass::ProcessControl),
    ("UV_CONFIG_FILE", ReservedClass::HelperProgram),
    ("UV_ENV_FILE", ReservedClass::HelperProgram),
    ("RUFF", ReservedClass::HelperProgram),
    ("UV_DOWNLOAD_URL", ReservedClass::TrafficRedirect),
    ("SSL_CLIENT_CERT", ReservedClass::TrafficRedirect),
    // FV-08: Lua reads a versioned name first (LUA_INIT_5_4, then LUA_INIT), for any version.
    ("LUA_INIT_5_4", ReservedClass::StartupHook),
    ("LUA_PATH_5_4", ReservedClass::StartupHook),
    ("LUA_CPATH_5_4", ReservedClass::StartupHook),
    ("LUA_CPATH_5_5", ReservedClass::StartupHook),
    // FV-08: Perl, Ruby and R, re-checked against what each reads when it starts.
    ("PERL5SHELL", ReservedClass::HelperProgram),
    ("PERL_USE_UNSAFE_INC", ReservedClass::StartupHook),
    ("RUBYPATH", ReservedClass::ProcessControl),
    ("RUBYSHELL", ReservedClass::HelperProgram),
    ("RUBY_CRASH_REPORT", ReservedClass::HelperProgram),
    ("RUBYGEMS_GEMDEPS", ReservedClass::StartupHook),
    ("R_DEFAULT_PACKAGES", ReservedClass::StartupHook),
    ("R_SCRIPT_DEFAULT_PACKAGES", ReservedClass::StartupHook),
    ("R_LIBS", ReservedClass::StartupHook),
    ("R_LIBS_USER", ReservedClass::StartupHook),
    ("R_LIBS_SITE", ReservedClass::StartupHook),
    ("RHOME", ReservedClass::ProcessControl),
    ("R_USER", ReservedClass::UserLocation),
    // Java and Ant.
    ("JDK_JAVAC_OPTIONS", ReservedClass::StartupHook),
    ("JAVACMD", ReservedClass::ProcessControl),
    ("ANT_ARGS", ReservedClass::StartupHook),
    // .NET.
    ("DOTNET_ROOT", ReservedClass::ProcessControl),
    ("DOTNET_ROOT_X64", ReservedClass::ProcessControl),
    ("DOTNET_HOST_PATH", ReservedClass::ProcessControl),
    ("DOTNET_SHARED_STORE", ReservedClass::ProcessControl),
    ("DOTNET_DIAGNOSTICPORTS", ReservedClass::StartupHook),
    ("COMPLUS_GCSERVER", ReservedClass::StartupHook),
    (
        "DOTNET_MSBUILD_SDK_RESOLVER_SDKS_DIR",
        ReservedClass::HelperProgram,
    ),
    ("DOTNET_DBGMINIDUMPNAME", ReservedClass::OutputCapture),
    ("DOTNET_DbgMiniDumpName", ReservedClass::OutputCapture),
    ("DOTNET_HOST_TRACEFILE", ReservedClass::OutputCapture),
    ("COREHOST_TRACEFILE", ReservedClass::OutputCapture),
    ("NUGET_PACKAGES", ReservedClass::UserLocation),
    // make.
    ("MAKEFILES", ReservedClass::HelperProgram),
    ("MAKESHELL", ReservedClass::HelperProgram),
    ("RM", ReservedClass::HelperProgram),
    ("YACC", ReservedClass::HelperProgram),
    ("LDLIBS", ReservedClass::HelperProgram),
    // CMake.
    ("OBJC", ReservedClass::HelperProgram),
    ("CUDACXX", ReservedClass::HelperProgram),
    ("RC", ReservedClass::HelperProgram),
    ("ASM_NASM", ReservedClass::HelperProgram),
    ("CSFLAGS", ReservedClass::HelperProgram),
    ("ZLIB_ROOT", ReservedClass::UserLocation),
    ("DESTDIR", ReservedClass::UserLocation),
    // Shells.
    ("PS0", ReservedClass::ShellStartup),
    ("PS2", ReservedClass::ShellStartup),
    ("BASHOPTS", ReservedClass::ShellStartup),
    ("BASH_LOADABLES_PATH", ReservedClass::ShellStartup),
    ("EXECIGNORE", ReservedClass::ProcessControl),
    ("CDPATH", ReservedClass::ProcessControl),
    ("HISTFILE", ReservedClass::OutputCapture),
    // Pagers and editors.
    ("LESS", ReservedClass::HelperProgram),
    ("LESSKEY", ReservedClass::HelperProgram),
    ("LESSEDIT", ReservedClass::HelperProgram),
    ("LESSGLOBALTAGS", ReservedClass::HelperProgram),
    ("FCEDIT", ReservedClass::HelperProgram),
    ("GH_PAGER", ReservedClass::HelperProgram),
    ("KUBE_EDITOR", ReservedClass::HelperProgram),
    ("GH_BROWSER", ReservedClass::HelperProgram),
    ("MY_TOOL_ASKPASS", ReservedClass::HelperProgram),
    // The shapes that the documented names of the families share.
    ("SOCKS_PROXY", ReservedClass::TrafficRedirect),
    ("ANY_TOOL_HOME", ReservedClass::UserLocation),
    ("ANY_PACKAGE_DIR", ReservedClass::UserLocation),
    ("XDG_CONFIG_DIRS", ReservedClass::UserLocation),
    ("ANY_TOOL_PATH", ReservedClass::ProcessControl),
    ("NUGET_PLUGIN_PATHS", ReservedClass::ProcessControl),
    ("ANY_TOOL_OPTS", ReservedClass::StartupHook),
    ("ANY_TOOL_OPTIONS", ReservedClass::StartupHook),
    ("ANY_TOOL_ARGS", ReservedClass::HelperProgram),
    ("ANY_TOOL_COMMAND", ReservedClass::HelperProgram),
    ("ANY_TOOL_WRAPPER", ReservedClass::HelperProgram),
    ("CMAKE_TEST_LAUNCHER", ReservedClass::HelperProgram),
    ("ANY_TOOL_RUNNER", ReservedClass::HelperProgram),
    ("ANY_TOOL_LINKER", ReservedClass::HelperProgram),
    ("ANY_TOOLFLAGS", ReservedClass::HelperProgram),
];

#[test]
fn one_newly_covered_name_per_family_is_reserved_in_its_class_in_any_case() {
    for (name, class) in NEWLY_COVERED {
        for spelled in [
            (*name).to_string(),
            name.to_ascii_lowercase(),
            mixed_case(name),
        ] {
            assert_eq!(reserved_class(&spelled), Some(*class), "{spelled}");
        }
    }
}

/// A key name that a family shape could catch by mistake. A shape reads the last token
/// only, and a word that only contains the word of a shape is not that word.
#[test]
fn a_shape_reads_the_last_whole_token_only() {
    for name in [
        "PATH_TOKEN",
        "HOME_ASSISTANT_TOKEN",
        "PROXY_TOKEN",
        "EDITOR_KEY",
        "JAVA_HOME_KEY",
        "DIRECTORY_SERVICE_KEY",
        "ROOTLY_API_KEY",
        "LESSON_KEY",
        "FEATURE_FLAGS",
        "SLACK_WEBHOOK",
        "GIT_TOKEN",
        "PYTHONANYWHERE_API_TOKEN",
        "DOTNET_ENVIRONMENT",
        "NODE_ENV",
        "NODE_AUTH_TOKEN",
        "NUGET_API_KEY",
        "UV_PUBLISH_PASSWORD",
        "UV_INDEX_INTERNAL_USERNAME",
        "YARN_NPM_AUTH_IDENT",
    ] {
        assert_eq!(reserved_class(name), None, "{name}");
    }
}

fn vault_bytes(vault: &Vault) -> Vec<u8> {
    std::fs::read(vault.path()).unwrap()
}

/// Names from NEWLY_COVERED that pass the variable-name syntax, and the cc forms.
const REFUSED_ON_EVERY_PATH: &[&str] = &[
    "HOST_CC",
    "TARGET_CC",
    "CC_X86_64_UNKNOWN_LINUX_GNU",
    "TARGET_CFLAGS",
    "CMAKE",
    "PKG_CONFIG_PATH",
    "CARGO",
    "RUSTFMT",
    "CL",
    "LINK",
    "GIT_DIR",
    "GIT_TRACE_CURL",
    "GOCACHEPROG",
    "NODE_REPL_EXTERNAL_MODULE",
    "YARN_ENABLE_SCRIPTS",
    "PYTHONWARNINGS",
    "UV_PROJECT_ENVIRONMENT",
    "JDK_JAVAC_OPTIONS",
    "DOTNET_ROOT_X64",
    "DOTNET_DBGMINIDUMPNAME",
    "MAKEFILES",
    "ZLIB_ROOT",
    "PS0",
    "LESSEDIT",
    "GH_PAGER",
    "SOCKS_PROXY",
    "LUA_INIT_5_4",
    "LUA_PATH_5_4",
    "LUA_CPATH_5_4",
    "PERL5SHELL",
    "PERL_USE_UNSAFE_INC",
    "RUBYPATH",
    "RUBYSHELL",
    "RUBY_CRASH_REPORT",
    "RUBYGEMS_GEMDEPS",
    "R_DEFAULT_PACKAGES",
    "R_SCRIPT_DEFAULT_PACKAGES",
    "R_LIBS",
    "R_LIBS_USER",
    "R_LIBS_SITE",
    "RHOME",
    "R_USER",
];

/// CORE-R1-02, through each core path that refuses a reserved name: add with an explicit
/// and with a derived name, update, the import preview, the import, and the plan that `run`
/// and `export` use for a key that an older vault stores. No refusal changes the vault file.
#[test]
fn newly_reserved_names_are_refused_by_add_update_import_and_planning_with_the_vault_bytes_unchanged(
) {
    let (_directory, mut vault) = vault();
    let kept = vault
        .add(ApiKeyInput::new("Ordinary key", "fake-secret"))
        .unwrap();
    let before = vault_bytes(&vault);
    for name in REFUSED_ON_EVERY_PATH {
        let mut explicit = ApiKeyInput::new("A harmless title", "fake-secret");
        explicit.env_var = Some((*name).into());
        let text = message(vault.add(explicit).unwrap_err());
        assert!(
            text.contains(name) && text.contains("reserved"),
            "{name}: {text}"
        );
        assert!(!text.contains("fake-secret"), "{name}: {text}");

        let derived = message(
            vault
                .add(ApiKeyInput::new(name.replace('_', " "), "fake-secret"))
                .unwrap_err(),
        );
        assert!(
            derived.contains(name) && derived.contains("reserved"),
            "{name}: {derived}"
        );

        let mut edit = vault.edit_input(&kept.id).unwrap();
        edit.env_var = Some((*name).into());
        let text = message(vault.update(&kept.id, edit).unwrap_err());
        assert!(
            text.contains(name) && text.contains("reserved"),
            "{name}: {text}"
        );

        let entries = vec![((*name).to_string(), "fake-hostile-value".to_string())];
        for text in [
            message(
                vault
                    .preview_import_env(&entries, Some("demo"), Some("dev"))
                    .unwrap_err(),
            ),
            message(
                vault
                    .import_env(&entries, Some("demo"), Some("dev"))
                    .unwrap_err(),
            ),
        ] {
            assert!(
                text.contains(name) && text.contains("reserved"),
                "{name}: {text}"
            );
            assert!(!text.contains("fake-hostile-value"), "{name}: {text}");
        }
        assert_eq!(
            vault_bytes(&vault),
            before,
            "{name}: the vault file changed"
        );
    }
    assert_eq!(vault.list().len(), 1);
    assert_eq!(
        vault.get_by_id(&kept.id).unwrap().resolved_env_var(),
        "ORDINARY_KEY"
    );

    for name in REFUSED_ON_EVERY_PATH
        .iter()
        .copied()
        .chain(["CC_x86_64-unknown-linux-gnu", "host_cc"])
    {
        let (_directory, vault) =
            legacy_vault_with(&[("Stored build key", name), ("Safe key", "SAFE_TOKEN")]);
        let before = vault_bytes(&vault);
        for plan in [
            vault.plan_env(Some("demo"), Some("dev"), false),
            vault.plan_env(None, None, true),
        ] {
            let text = message(plan.unwrap_err());
            assert!(text.contains("Stored build key"), "{name}: {text}");
            assert!(text.contains(name), "{name}: {text}");
            assert!(
                text.contains("reserved") || text.contains("not a valid"),
                "{name}: {text}"
            );
            assert!(!text.contains("fake-legacy-secret"), "{name}: {text}");
        }
        assert_eq!(
            vault_bytes(&vault),
            before,
            "{name}: the vault file changed"
        );
    }
}
