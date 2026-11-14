use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::lifecycle::SecretChange;
use crate::schedule::{rotation_due, Remaining};

/// Custom field names written into the KDBX entry. KeePassXC preserves unknown
/// fields, so the vault stays fully readable outside this app.
/// The longest rotation interval that input accepts: 100 years (FR-15).
pub const MAX_ROTATION_DAYS: u32 = 36_500;

pub const META_ID: &str = "AKM_ID";
pub const META_ISSUER: &str = "AKM_ISSUER";
pub const META_PROJECT: &str = "AKM_PROJECT";
pub const META_ENVIRONMENT: &str = "AKM_ENV";
pub const META_SCOPES: &str = "AKM_SCOPES";
pub const META_ENV_VAR: &str = "AKM_ENV_VAR";
pub const META_ROTATION_DAYS: &str = "AKM_ROTATION_DAYS";
/// When the secret last changed, `YYYY-MM-DDTHH:MM:SSZ` in UTC. Absent means unknown.
pub const META_SECRET_CHANGED: &str = "AKM_SECRET_CHANGED";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub changes: Vec<ImportChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChange {
    pub name: String,
    pub title: String,
    pub action: String,
}

/// A key's metadata. It carries no secret: a secret leaves the vault only through
/// [`crate::Vault::secret`] or [`crate::Vault::env_vars`], each with a [`crate::Disclosure`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub title: String,
    pub username: Option<String>,
    pub issuer: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub scopes: Vec<String>,
    /// Environment variable this key is injected as. Derived from the title if unset.
    pub env_var: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotation_days: Option<u32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// When the secret last changed. `Some` only when `secret_change` is `Known`.
    pub secret_changed_at: Option<DateTime<Utc>>,
    pub secret_change: SecretChange,
    /// The number of earlier versions in the native KDBX history.
    pub history_count: usize,
}

impl ApiKey {
    /// A match across the fields a person would search by, under [`caseless`].
    pub fn matches(&self, needle: &str) -> bool {
        let needle = caseless(needle);
        let haystack = [
            Some(self.title.as_str()),
            self.issuer.as_deref(),
            self.project.as_deref(),
            self.environment.as_deref(),
            self.username.as_deref(),
            self.url.as_deref(),
            self.notes.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.tags.iter().map(String::as_str))
        .chain(self.scopes.iter().map(String::as_str));

        haystack.into_iter().any(|v| caseless(v).contains(&needle))
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| e <= now)
    }

    /// When the rotation interval elapses, counted from the last secret change. It is `None`
    /// when AKM knows no such time: an edit of notes or metadata never counts.
    pub fn rotation_due_at(&self) -> Option<DateTime<Utc>> {
        rotation_due(self.secret_changed_at?, self.rotation_days?)
    }

    /// The distance to [`ApiKey::rotation_due_at`]: exact for scheduling, whole days for display.
    pub fn rotation_remaining(&self, now: DateTime<Utc>) -> Option<Remaining> {
        Some(Remaining::of(self.rotation_due_at()?, now))
    }

    pub fn expiry_remaining(&self, now: DateTime<Utc>) -> Option<Remaining> {
        Some(Remaining::of(self.expires_at?, now))
    }

    /// The environment variable name used when injecting this key.
    pub fn resolved_env_var(&self) -> String {
        self.env_var
            .clone()
            .unwrap_or_else(|| derive_env_var(&self.title))
    }
}

/// A secret value on its way into the vault. `Debug` and `Serialize` never show it, and no
/// accessor outside this crate returns its text.
#[derive(Clone, Default)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn contains_nul(&self) -> bool {
        self.0.contains('\0')
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("***")
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ApiKeyInput {
    pub title: String,
    pub secret: Secret,
    pub username: Option<String>,
    pub issuer: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub scopes: Vec<String>,
    pub env_var: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotation_days: Option<u32>,
}

impl ApiKeyInput {
    pub fn new(title: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            secret: Secret::new(secret),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> crate::Result<()> {
        self.validate_fields()?;
        self.reject_nul_secret()
    }

    /// A NUL character cannot reach a child process or an environment file, so a secret that
    /// has one is refused when it comes in (FR-16). A secret that is already stored with one
    /// stays viewable; `update` asks for this check only when the secret changes.
    pub(crate) fn reject_nul_secret(&self) -> crate::Result<()> {
        if self.secret.contains_nul() {
            return Err(crate::Error::Invalid(
                "secret must not contain a NUL character".into(),
            ));
        }
        Ok(())
    }

    /// Everything but the NUL check of the secret.
    pub(crate) fn validate_fields(&self) -> crate::Result<()> {
        self.validate_changes(None)
    }

    /// The checks of each field whose value differs from the one in `stored`, and of every
    /// field when there is no stored key. An update does not judge a field that it leaves as
    /// it is: another program, or an older rule, can have stored a value that AKM would not
    /// take today, and that value must not stop an edit of another field (D2B-02).
    pub(crate) fn validate_changes(&self, stored: Option<&Self>) -> crate::Result<()> {
        let changed =
            |same: fn(&Self, &Self) -> bool| stored.is_none_or(|stored| !same(self, stored));
        let title = changed(|new, old| new.title == old.title);
        if title && self.title.trim().is_empty() {
            return Err(crate::Error::Invalid("title must not be empty".into()));
        }
        if changed(|new, old| new.secret.as_str() == old.secret.as_str()) && self.secret.is_empty()
        {
            return Err(crate::Error::Invalid("secret must not be empty".into()));
        }
        let texts = [
            ("title", Some(&self.title), title),
            (
                "username",
                self.username.as_ref(),
                changed(|new, old| new.username == old.username),
            ),
            (
                "issuer",
                self.issuer.as_ref(),
                changed(|new, old| new.issuer == old.issuer),
            ),
            (
                "project",
                self.project.as_ref(),
                changed(|new, old| new.project == old.project),
            ),
            (
                "environment",
                self.environment.as_ref(),
                changed(|new, old| new.environment == old.environment),
            ),
            (
                "url",
                self.url.as_ref(),
                changed(|new, old| new.url == old.url),
            ),
            (
                "notes",
                self.notes.as_ref(),
                changed(|new, old| new.notes == old.notes),
            ),
        ];
        let lists = [
            (
                "scope",
                &self.scopes,
                changed(|new, old| new.scopes == old.scopes),
            ),
            ("tag", &self.tags, changed(|new, old| new.tags == old.tags)),
        ];
        let nul_field = texts
            .iter()
            .find(|(_, value, changed)| *changed && value.is_some_and(|value| value.contains('\0')))
            .map(|(label, _, _)| *label)
            .or_else(|| {
                lists
                    .iter()
                    .find(|(_, values, changed)| {
                        *changed && values.iter().any(|value| value.contains('\0'))
                    })
                    .map(|(label, _, _)| *label)
            });
        if let Some(label) = nul_field {
            return Err(crate::Error::Invalid(format!(
                "{label} must not contain a NUL character"
            )));
        }
        if let Some(var) = &self.env_var {
            if changed(|new, old| new.env_var == old.env_var) && !is_valid_env_var(var) {
                return Err(crate::Error::Invalid(format!(
                    "{var:?} is not a valid environment variable name (use A-Z, 0-9, _)"
                )));
            }
        }
        let resolved = self.resolved_env_var();
        if stored.is_none_or(|stored| stored.resolved_env_var() != resolved) {
            if let Some(class) = reserved_class(&resolved) {
                return Err(crate::Error::Invalid(format!(
                    "{resolved:?} is reserved: {}; set a different env var name for this key",
                    class.reason()
                )));
            }
        }
        if let Some(expires) = self.expires_at {
            if changed(|new, old| new.expires_at == old.expires_at) {
                crate::schedule::require_whole_second(expires)?;
            }
        }
        if let Some(days) = self.rotation_days {
            if changed(|new, old| new.rotation_days == old.rotation_days) {
                if days == 0 {
                    return Err(crate::Error::Invalid(
                        "rotation_days must be greater than zero".into(),
                    ));
                }
                if days > MAX_ROTATION_DAYS {
                    return Err(crate::Error::Invalid(format!(
                        "rotation_days must be at most {MAX_ROTATION_DAYS} (100 years)"
                    )));
                }
            }
        }
        Ok(())
    }

    fn resolved_env_var(&self) -> String {
        self.env_var
            .clone()
            .unwrap_or_else(|| derive_env_var(&self.title))
    }
}

/// The one caseless rule of AKM (FR-17, core review 1, finding 10). Every title comparison
/// uses it: selection of a live or a recycled key, search, and the CLI listing order.
///
/// Each character becomes the lowercase of its uppercase when its uppercase is one character,
/// and its plain lowercase otherwise. This is Unicode simple case folding in practice, with no
/// table to keep: `Σ`, `σ` and `ς` are one letter, `ẞ` and `ß` are one letter, and the Kelvin
/// sign is `k`. `ß` is not `ss`, because its uppercase is two letters. The rule maps letters
/// only: it does not normalize, so a precomposed `é` and `e` with a combining accent differ.
pub fn caseless(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for letter in text.chars() {
        let mut upper = letter.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(single), None) => folded.extend(single.to_lowercase()),
            _ => folded.extend(letter.to_lowercase()),
        }
    }
    folded
}

pub fn is_valid_env_var(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Why a variable name is kept out of a child process. Each class is one way that an
/// environment variable makes a program run other code or send its traffic elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedClass {
    AkmInternal,
    ProcessControl,
    StartupHook,
    HelperProgram,
    ShellStartup,
    TrafficRedirect,
    UserLocation,
    OutputCapture,
    SettingsFile,
}

impl ReservedClass {
    pub fn reason(self) -> &'static str {
        match self {
            Self::AkmInternal => "AKM uses it internally",
            Self::ProcessControl => "it changes which programs and libraries a process loads",
            Self::StartupHook => "it makes a runtime load or run code when it starts",
            Self::HelperProgram => "it names a helper program or a setting that a tool runs",
            Self::ShellStartup => "it makes a shell run a script or a command",
            Self::TrafficRedirect => {
                "it redirects network traffic or changes which certificates are trusted"
            }
            Self::UserLocation => "it moves the folders where programs keep settings and files",
            Self::OutputCapture => {
                "it makes a program write what it does or holds in memory to a file"
            }
            Self::SettingsFile => {
                "it names a file that a program reads its settings or its credentials from"
            }
        }
    }
}

struct Rule {
    /// The documentation of the tool that reads these names. The tests check that each
    /// family names one.
    #[cfg_attr(not(test), allow(dead_code, reason = "only the tests read it"))]
    doc: &'static str,
    class: ReservedClass,
    /// Whole names. An exact match beats every other rule, so a name can sit in a narrower
    /// class than the prefix family around it (`CARGO_HOME` against `CARGO_`).
    exact: &'static [&'static str],
    /// Names that start with one of these. The longest matching prefix decides.
    prefixes: &'static [&'static str],
}

/// Names inside a reserved prefix family that the tool itself documents as a credential.
/// They are free: AKM hands them over as stored, as every name that no rule reserves.
fn is_registry_credential(upper: &str) -> bool {
    const REGISTRIES: &str = "CARGO_REGISTRIES_";
    const TOKEN: &str = "_TOKEN";
    // https://doc.rust-lang.org/cargo/reference/environment-variables.html
    upper == "CARGO_REGISTRY_TOKEN"
        || (upper.len() > REGISTRIES.len() + TOKEN.len()
            && upper.starts_with(REGISTRIES)
            && upper.ends_with(TOKEN))
        // https://yarnpkg.com/configuration/yarnrc (npmAuthToken, npmAuthIdent)
        || upper == "YARN_NPM_AUTH_TOKEN"
        || upper == "YARN_NPM_AUTH_IDENT"
}

/// The reserved table, by tool family. Each family was checked against the documentation
/// in its `doc`. The table is finite: a name that no rule and no shape below covers is
/// handed over as stored.
const RESERVED: &[Rule] = &[
    Rule {
        doc: "docs/PROJECT.md",
        class: ReservedClass::AkmInternal,
        exact: &[],
        prefixes: &["AKM_"],
    },
    // Not re-checked in this unit: Windows, cmd, PowerShell, the XDG folders and ld.so.
    Rule {
        doc: "https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd",
        class: ReservedClass::ProcessControl,
        exact: &["PATH", "PATHEXT", "COMSPEC", "SYSTEMROOT", "WINDIR"],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/prompt",
        class: ReservedClass::ShellStartup,
        // `PROMPT` is printed on every line; `DIRCMD` and `COPYCMD` are read by dir and copy.
        exact: &["PROMPT", "DIRCMD", "COPYCMD"],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_environment_variables",
        class: ReservedClass::ShellStartup,
        exact: &["PSEXECUTIONPOLICYPREFERENCE", "__PSLOCKDOWNPOLICY"],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_environment_variables",
        class: ReservedClass::ProcessControl,
        exact: &["PSMODULEPATH"],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/windows/deployment/usmt/usmt-recognized-environment-variables",
        class: ReservedClass::UserLocation,
        exact: &[
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
        prefixes: &[],
    },
    Rule {
        doc: "https://specifications.freedesktop.org/basedir-spec/latest/",
        class: ReservedClass::UserLocation,
        exact: &["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"],
        prefixes: &[],
    },
    Rule {
        doc: "https://man7.org/linux/man-pages/man8/ld.so.8.html",
        class: ReservedClass::ProcessControl,
        exact: &[],
        prefixes: &["LD_", "DYLD_"],
    },
    // Rust and Cargo. `RUSTC_` covers RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER and
    // RUSTC_BOOTSTRAP. Cargo reads every `CARGO_` variable as a configuration key, which
    // covers CARGO_BUILD_RUSTC*, CARGO_ENCODED_RUSTFLAGS and CARGO_TARGET_<triple>_RUNNER,
    // _LINKER and _RUSTFLAGS. `is_registry_credential` takes the registry tokens out again.
    Rule {
        doc: "https://doc.rust-lang.org/cargo/reference/environment-variables.html",
        class: ReservedClass::HelperProgram,
        exact: &[
            "CARGO",
            "RUSTC",
            "RUSTDOC",
            "RUSTFMT",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
        ],
        prefixes: &["RUSTC_", "CARGO_"],
    },
    Rule {
        doc: "https://doc.rust-lang.org/cargo/reference/environment-variables.html",
        class: ReservedClass::TrafficRedirect,
        exact: &[],
        prefixes: &["CARGO_HTTP_", "CARGO_NET_", "CARGO_SOURCE_"],
    },
    Rule {
        doc: "https://doc.rust-lang.org/cargo/reference/environment-variables.html",
        class: ReservedClass::UserLocation,
        exact: &["CARGO_HOME", "CARGO_TARGET_DIR"],
        prefixes: &[],
    },
    // rustup picks the toolchain it runs from every `RUSTUP_` variable but the few below.
    Rule {
        doc: "https://rust-lang.github.io/rustup/environment-variables.html",
        class: ReservedClass::ProcessControl,
        exact: &[],
        prefixes: &["RUSTUP_"],
    },
    Rule {
        doc: "https://rust-lang.github.io/rustup/environment-variables.html",
        class: ReservedClass::TrafficRedirect,
        exact: &["RUSTUP_DIST_SERVER", "RUSTUP_DIST_ROOT", "RUSTUP_UPDATE_ROOT"],
        prefixes: &[],
    },
    Rule {
        doc: "https://rust-lang.github.io/rustup/environment-variables.html",
        class: ReservedClass::UserLocation,
        exact: &["RUSTUP_HOME"],
        prefixes: &[],
    },
    // The cc crate, besides the build-variable shape in `is_build_variable`.
    Rule {
        doc: "https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables",
        class: ReservedClass::HelperProgram,
        exact: &["CROSS_COMPILE"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables",
        class: ReservedClass::ProcessControl,
        // Where the compiler finds its tools, headers and libraries.
        exact: &[
            "SDKROOT",
            "WASI_SYSROOT",
            "WASM_MUSL_SYSROOT",
            "PAUTHTEST_SYSROOT",
            "VCINSTALLDIR",
            "VSINSTALLDIR",
            "CPATH",
        ],
        prefixes: &[],
    },
    // The MSVC compiler and linker, which cc runs on Windows.
    Rule {
        doc: "https://learn.microsoft.com/en-us/cpp/build/reference/cl-environment-variables",
        class: ReservedClass::HelperProgram,
        // Options and files that cl and link add to every command line.
        exact: &["CL", "_CL_", "LINK", "_LINK_"],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/cpp/build/reference/linking",
        class: ReservedClass::ProcessControl,
        exact: &["INCLUDE", "LIB", "LIBPATH"],
        prefixes: &[],
    },
    // Git.
    Rule {
        doc: "https://git-scm.com/docs/git#_environment_variables",
        class: ReservedClass::HelperProgram,
        exact: &[
            "GIT_SSH_COMMAND",
            "GIT_SSH",
            "GIT_SSH_VARIANT",
            "GIT_ASKPASS",
            "GIT_EXEC_PATH",
            "GIT_PROXY_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_TEMPLATE_DIR",
            "GIT_PAGER",
            "GIT_EDITOR",
            "GIT_SEQUENCE_EDITOR",
            "GIT_CONFIG",
            "GIT_ALLOW_PROTOCOL",
            "GIT_PROTOCOL_FROM_USER",
            "GIT_ATTR_SOURCE",
        ],
        // GIT_CONFIG_GLOBAL, _SYSTEM, _NOSYSTEM, _COUNT, _KEY_<n>, _VALUE_<n>, _PARAMETERS.
        prefixes: &["GIT_CONFIG_"],
    },
    Rule {
        doc: "https://git-scm.com/docs/git#_environment_variables",
        class: ReservedClass::UserLocation,
        exact: &[
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_NAMESPACE",
            "GIT_CEILING_DIRECTORIES",
            "GIT_DISCOVERY_ACROSS_FILESYSTEM",
            "GIT_REFERENCE_BACKEND",
        ],
        prefixes: &[],
    },
    Rule {
        // http.ssl* and http.proxySSL* in git-config: GIT_SSL_NO_VERIFY, _CAINFO, _CAPATH,
        // _CERT, _KEY, _VERSION, _CIPHER_LIST and GIT_PROXY_SSL_CERT, _KEY, _CAINFO.
        doc: "https://git-scm.com/docs/git-config",
        class: ReservedClass::TrafficRedirect,
        exact: &[],
        prefixes: &["GIT_SSL_", "GIT_PROXY_"],
    },
    Rule {
        doc: "https://git-scm.com/docs/git#_environment_variables",
        class: ReservedClass::OutputCapture,
        exact: &["GIT_TRACE", "GIT_TRACE2"],
        // Every trace target, and the Windows redirections of the standard streams.
        prefixes: &["GIT_TRACE_", "GIT_TRACE2_", "GIT_REDIRECT_"],
    },
    // Go.
    Rule {
        doc: "https://pkg.go.dev/cmd/go#hdr-Environment_variables",
        class: ReservedClass::ProcessControl,
        exact: &["GOROOT", "GOTOOLCHAIN", "GCCGOTOOLDIR"],
        prefixes: &[],
    },
    Rule {
        doc: "https://pkg.go.dev/cmd/go#hdr-Environment_variables",
        class: ReservedClass::HelperProgram,
        exact: &[
            "GOFLAGS",
            "GOAUTH",
            "GCCGO",
            "GOCACHEPROG",
            "GOVCS",
            "GO_EXTLINK_ENABLED",
        ],
        prefixes: &["CGO_"],
    },
    Rule {
        doc: "https://pkg.go.dev/cmd/go#hdr-Environment_variables",
        class: ReservedClass::UserLocation,
        exact: &[
            "GOPATH",
            "GOBIN",
            "GOENV",
            "GOMODCACHE",
            "GOCACHE",
            "GOWORK",
            "GOTMPDIR",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://pkg.go.dev/cmd/go#hdr-Environment_variables",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "GOPROXY",
            "GONOPROXY",
            "GOSUMDB",
            "GONOSUMDB",
            "GOINSECURE",
            "GOPRIVATE",
            "GODEBUG",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://pkg.go.dev/cmd/go#hdr-Environment_variables",
        class: ReservedClass::OutputCapture,
        exact: &["GOCOVERDIR"],
        prefixes: &[],
    },
    // Node, npm and Yarn.
    Rule {
        doc: "https://nodejs.org/api/cli.html#environment-variables",
        class: ReservedClass::StartupHook,
        exact: &[
            "NODE_OPTIONS",
            "NODE_PATH",
            "NODE_REPL_EXTERNAL_MODULE",
            "NODE_COMPILE_CACHE",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://nodejs.org/api/cli.html#environment-variables",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "NODE_EXTRA_CA_CERTS",
            "NODE_TLS_REJECT_UNAUTHORIZED",
            "NODE_USE_ENV_PROXY",
            "NODE_USE_SYSTEM_CA",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://nodejs.org/api/cli.html#environment-variables",
        class: ReservedClass::OutputCapture,
        exact: &["NODE_REDIRECT_WARNINGS", "NODE_V8_COVERAGE", "NODE_REPL_HISTORY"],
        prefixes: &[],
    },
    Rule {
        doc: "https://www.electronjs.org/docs/latest/api/environment-variables",
        class: ReservedClass::StartupHook,
        exact: &["ELECTRON_RUN_AS_NODE"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.npmjs.com/cli/using-npm/config#environment-variables",
        class: ReservedClass::TrafficRedirect,
        exact: &[],
        prefixes: &["NPM_CONFIG_"],
    },
    // Yarn reads every setting from `YARN_` and its name in snake case. The two credential
    // settings come out again in `is_registry_credential`.
    Rule {
        doc: "https://yarnpkg.com/configuration/yarnrc",
        class: ReservedClass::HelperProgram,
        exact: &["YARN_YARN_PATH"],
        prefixes: &["YARN_"],
    },
    Rule {
        doc: "https://yarnpkg.com/configuration/yarnrc",
        class: ReservedClass::TrafficRedirect,
        exact: &["YARN_NPM_REGISTRY_SERVER", "YARN_HTTP_PROXY", "YARN_HTTPS_PROXY"],
        prefixes: &[],
    },
    // Not re-checked in this unit: Bun, Corepack, Deno and pnpm.
    Rule {
        doc: "https://bun.sh/docs/runtime/env",
        class: ReservedClass::TrafficRedirect,
        exact: &["BUN_CONFIG_REGISTRY", "COREPACK_NPM_REGISTRY"],
        prefixes: &[],
    },
    Rule {
        doc: "https://bun.sh/docs/runtime/env",
        class: ReservedClass::UserLocation,
        exact: &["BUN_INSTALL", "DENO_DIR", "PNPM_HOME"],
        prefixes: &[],
    },
    // Python, pip and uv.
    Rule {
        doc: "https://docs.python.org/3/using/cmdline.html#environment-variables",
        class: ReservedClass::StartupHook,
        exact: &[
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONSTARTUP",
            "PYTHONINSPECT",
            "PYTHONBREAKPOINT",
            "PYTHONUSERBASE",
            "PYTHONEXECUTABLE",
            "PYTHONPLATLIBDIR",
            "PYTHONPYCACHEPREFIX",
            // A warning filter names a category class, and Python imports its module.
            "PYTHONWARNINGS",
            "PYTHON_PRESITE",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.python.org/3/using/cmdline.html#environment-variables",
        class: ReservedClass::OutputCapture,
        exact: &["PYTHON_HISTORY", "PYTHONDUMPREFSFILE"],
        prefixes: &[],
    },
    Rule {
        doc: "https://pip.pypa.io/en/stable/topics/configuration/#environment-variables",
        class: ReservedClass::TrafficRedirect,
        exact: &[],
        prefixes: &["PIP_"],
    },
    Rule {
        doc: "https://docs.astral.sh/uv/reference/environment/",
        class: ReservedClass::ProcessControl,
        exact: &[
            "UV_PYTHON",
            "UV_PROJECT_ENVIRONMENT",
            "UV_SYSTEM_PYTHON",
            "VIRTUAL_ENV",
            "CONDA_PREFIX",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.astral.sh/uv/reference/environment/",
        class: ReservedClass::HelperProgram,
        exact: &["UV", "RUFF", "TY", "UV_CONFIG_FILE", "UV_ENV_FILE"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.astral.sh/uv/reference/environment/",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "UV_INDEX_URL",
            "UV_DEFAULT_INDEX",
            "UV_EXTRA_INDEX_URL",
            "UV_INDEX",
            "UV_FIND_LINKS",
            "UV_PUBLISH_URL",
            "UV_PUBLISH_CHECK_URL",
            "UV_PUBLISH_INDEX",
            "UV_PYTHON_INSTALL_MIRROR",
            "UV_PYPY_INSTALL_MIRROR",
            "UV_PYODIDE_INSTALL_MIRROR",
            "UV_GRAALPY_INSTALL_MIRROR",
            "UV_ASTRAL_MIRROR_URL",
            "UV_PYTHON_DOWNLOADS_JSON_URL",
            "UV_INSTALLER_GITHUB_BASE_URL",
            "UV_INSTALLER_GHE_BASE_URL",
            "UV_DOWNLOAD_URL",
            "UV_S3_ENDPOINT_URL",
            "UV_GCS_ENDPOINT_URL",
            "UV_AZURE_ENDPOINT_URL",
            "UV_INSECURE_HOST",
            "UV_SYSTEM_CERTS",
            "UV_NATIVE_TLS",
            "SSL_CLIENT_CERT",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.astral.sh/uv/reference/environment/",
        class: ReservedClass::UserLocation,
        exact: &["NETRC"],
        prefixes: &[],
    },
    // Perl, Ruby, PHP, Lua and R: re-checked on 2026-11-13 against what each interpreter
    // reads when it starts (FV-08). Left out: names that need a debugging build of Perl
    // (PERLIO_DEBUG, PERL_MEM_LOG), and the files of `R CMD check` and `R CMD build`.
    Rule {
        doc: "https://perldoc.perl.org/perlrun#ENVIRONMENT",
        class: ReservedClass::StartupHook,
        exact: &[
            "PERL5OPT",
            "PERL5LIB",
            "PERLLIB",
            "PERL5DB",
            "PERL_USE_UNSAFE_INC",
            "RUBYOPT",
            "RUBYLIB",
            "PHPRC",
            "PHP_INI_SCAN_DIR",
            "R_PROFILE",
            "R_PROFILE_USER",
            "R_ENVIRON",
            "R_ENVIRON_USER",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://perldoc.perl.org/perlrun#ENVIRONMENT",
        class: ReservedClass::HelperProgram,
        exact: &["PERL5SHELL"],
        prefixes: &[],
    },
    Rule {
        // Lua tries the versioned name first (LUA_INIT_5_4), then LUA_INIT; so for every
        // version these are prefixes (lua.c `handle_luainit`, loadlib.c `setpath`).
        doc: "https://www.lua.org/manual/5.4/manual.html#7",
        class: ReservedClass::StartupHook,
        exact: &[],
        prefixes: &["LUA_INIT", "LUA_PATH", "LUA_CPATH"],
    },
    Rule {
        doc: "https://github.com/ruby/ruby/blob/master/man/ruby.1",
        class: ReservedClass::ProcessControl,
        exact: &["RUBYPATH"],
        prefixes: &[],
    },
    Rule {
        // `RUBY_CRASH_REPORT` that starts with `|` runs the rest as a command.
        doc: "https://github.com/ruby/ruby/blob/master/man/ruby.1",
        class: ReservedClass::HelperProgram,
        exact: &["RUBYSHELL", "RUBY_CRASH_REPORT"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.ruby-lang.org/en/master/Gem.html#method-c-use_gemdeps",
        class: ReservedClass::StartupHook,
        exact: &["RUBYGEMS_GEMDEPS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://stat.ethz.ch/R-manual/R-devel/library/base/html/libPaths.html",
        class: ReservedClass::StartupHook,
        exact: &[
            "R_LIBS",
            "R_LIBS_USER",
            "R_LIBS_SITE",
            "R_DEFAULT_PACKAGES",
            "R_SCRIPT_DEFAULT_PACKAGES",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://stat.ethz.ch/R-manual/R-devel/library/utils/html/Rscript.html",
        class: ReservedClass::ProcessControl,
        exact: &["RHOME"],
        prefixes: &[],
    },
    Rule {
        doc: "https://cran.r-project.org/bin/windows/base/rw-FAQ.html",
        class: ReservedClass::UserLocation,
        exact: &["R_USER"],
        prefixes: &[],
    },
    Rule {
        doc: "https://guides.rubygems.org/command-reference/",
        class: ReservedClass::ProcessControl,
        exact: &["GEM_HOME", "GEM_PATH"],
        prefixes: &[],
    },
    // Java, its build tools and servers.
    Rule {
        doc: "https://docs.oracle.com/en/java/javase/21/docs/specs/man/java.html",
        class: ReservedClass::StartupHook,
        exact: &[
            "JAVA_TOOL_OPTIONS",
            "_JAVA_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "CLASSPATH",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.oracle.com/en/java/javase/21/docs/specs/man/javac.html",
        class: ReservedClass::StartupHook,
        exact: &["JDK_JAVAC_OPTIONS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.oracle.com/en/java/javase/21/docs/specs/man/java.html",
        class: ReservedClass::ProcessControl,
        exact: &["JAVA_HOME"],
        prefixes: &[],
    },
    Rule {
        doc: "https://ant.apache.org/manual/running.html",
        class: ReservedClass::StartupHook,
        exact: &["ANT_OPTS", "ANT_ARGS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://ant.apache.org/manual/running.html",
        class: ReservedClass::ProcessControl,
        exact: &["JAVACMD"],
        prefixes: &[],
    },
    Rule {
        doc: "https://maven.apache.org/configure.html",
        class: ReservedClass::StartupHook,
        exact: &["MAVEN_OPTS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://maven.apache.org/configure.html",
        class: ReservedClass::HelperProgram,
        exact: &["MAVEN_ARGS"],
        prefixes: &[],
    },
    // Not re-checked in this unit: Gradle, sbt and Tomcat.
    Rule {
        doc: "https://docs.gradle.org/current/userguide/build_environment.html",
        class: ReservedClass::StartupHook,
        exact: &["GRADLE_OPTS", "SBT_OPTS", "JAVA_OPTS", "CATALINA_OPTS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.gradle.org/current/userguide/build_environment.html",
        class: ReservedClass::UserLocation,
        exact: &["GRADLE_USER_HOME"],
        prefixes: &[],
    },
    // .NET.
    Rule {
        doc: "https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables",
        class: ReservedClass::ProcessControl,
        exact: &[
            "DOTNET_HOST_PATH",
            "DOTNET_SERVICING",
            "DOTNET_SHARED_STORE",
            "DOTNET_MULTILEVEL_LOOKUP",
        ],
        // DOTNET_ROOT, DOTNET_ROOT_<ARCH> and DOTNET_ROOT(x86).
        prefixes: &["DOTNET_ROOT"],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables",
        class: ReservedClass::StartupHook,
        exact: &[
            "DOTNET_STARTUP_HOOKS",
            "DOTNET_ADDITIONAL_DEPS",
            // An endpoint that diagnostic tools connect to; one can load a profiler.
            "DOTNET_DIAGNOSTICPORTS",
        ],
        prefixes: &[],
    },
    Rule {
        // The runtime reads each setting as DOTNET_<name> and, before .NET 5, COMPlus_<name>.
        doc: "https://learn.microsoft.com/en-us/dotnet/core/runtime-config/debugging-profiling",
        class: ReservedClass::StartupHook,
        exact: &[],
        prefixes: &["COR_PROFILER", "COR_ENABLE_PROFILING", "CORECLR_", "COMPLUS_"],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables",
        class: ReservedClass::HelperProgram,
        exact: &["DOTNET_DBGCREATEDUMPTOOLPATH"],
        prefixes: &["DOTNET_MSBUILD_SDK_RESOLVER_"],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables",
        class: ReservedClass::UserLocation,
        exact: &["NUGET_PACKAGES"],
        prefixes: &[],
    },
    Rule {
        // Host traces, EventPipe traces, and the dumps of process memory on a crash.
        doc: "https://learn.microsoft.com/en-us/dotnet/core/diagnostics/collect-dumps-crash",
        class: ReservedClass::OutputCapture,
        exact: &["DOTNET_ENABLEEVENTPIPE"],
        prefixes: &[
            "DOTNET_HOST_TRACE",
            "COREHOST_",
            "DOTNET_DBG",
            "DOTNET_CREATEDUMP",
            "DOTNET_ENABLECRASHREPORT",
            "DOTNET_EVENTPIPE",
        ],
    },
    // make. GNU make turns every environment variable into a make variable; these are the
    // programs and flags of its implicit rules, and the files and shell it reads.
    Rule {
        doc: "https://www.gnu.org/software/make/manual/html_node/Implicit-Variables.html",
        class: ReservedClass::HelperProgram,
        exact: &[
            "MAKE",
            "MAKEFLAGS",
            "MFLAGS",
            "MAKEFILES",
            "MAKESHELL",
            "AS",
            "CO",
            "CPP",
            "CTANGLE",
            "CWEAVE",
            "FC",
            "GET",
            "LEX",
            "LINT",
            "M2C",
            "MAKEINFO",
            "PC",
            "RM",
            "TANGLE",
            "TEX",
            "TEXI2DVI",
            "WEAVE",
            "YACC",
            "CPPFLAGS",
            "LDFLAGS",
            "LDLIBS",
            "LOADLIBES",
            // binutils that build scripts and makefiles run.
            "LD",
            "NM",
            "STRIP",
            "OBJCOPY",
            "OBJDUMP",
        ],
        prefixes: &[],
    },
    // CMake: the compiler of each language and its flags, ASM<DIALECT> and
    // ASM<DIALECT>FLAGS. CMAKE_* is the build-variable shape; <PackageName>_ROOT is a shape.
    Rule {
        doc: "https://cmake.org/cmake/help/latest/manual/cmake-env-variables.7.html",
        class: ReservedClass::HelperProgram,
        exact: &[
            "OBJC",
            "OBJCXX",
            "CUDACXX",
            "CUDAHOSTCXX",
            "HIPCXX",
            "HIPHOSTCXX",
            "ISPC",
            "RC",
            "SWIFTC",
            "SYCLCXX",
            "ASM",
        ],
        prefixes: &["ASM_"],
    },
    Rule {
        doc: "https://cmake.org/cmake/help/latest/manual/cmake-env-variables.7.html",
        class: ReservedClass::UserLocation,
        exact: &["DESTDIR"],
        prefixes: &[],
    },
    // Shells.
    Rule {
        doc: "https://www.gnu.org/software/bash/manual/html_node/Bash-Variables.html",
        class: ReservedClass::ShellStartup,
        exact: &[
            "BASH_ENV",
            "ENV",
            "PROMPT_COMMAND",
            "PS0",
            "PS1",
            "PS2",
            "PS3",
            "PS4",
            "SHELLOPTS",
            "BASHOPTS",
            "BASH_LOADABLES_PATH",
            "MAILPATH",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://www.gnu.org/software/bash/manual/html_node/Bash-Variables.html",
        class: ReservedClass::ProcessControl,
        // Both change which file a command name or a `cd` reaches.
        exact: &["EXECIGNORE", "CDPATH"],
        prefixes: &[],
    },
    Rule {
        doc: "https://www.gnu.org/software/bash/manual/html_node/Bash-Variables.html",
        class: ReservedClass::OutputCapture,
        exact: &["HISTFILE"],
        prefixes: &[],
    },
    // Pagers, editors, browsers and password helpers that many tools run. The shapes in
    // `ENDINGS` cover the `<TOOL>_PAGER` and `<TOOL>_EDITOR` forms.
    Rule {
        doc: "https://man7.org/linux/man-pages/man1/less.1.html",
        class: ReservedClass::HelperProgram,
        exact: &[
            "EDITOR",
            "VISUAL",
            "PAGER",
            "MANPAGER",
            "BROWSER",
            "SHELL",
            "FCEDIT",
            "SSH_ASKPASS",
            "SUDO_ASKPASS",
            "LESS",
            "LESSOPEN",
            "LESSCLOSE",
            "LESSKEY",
            "LESSKEYIN",
            "LESSKEY_SYSTEM",
            "LESSKEYIN_SYSTEM",
            "LESSEDIT",
            "LESSGLOBALTAGS",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://man7.org/linux/man-pages/man1/less.1.html",
        class: ReservedClass::OutputCapture,
        exact: &["LESSHISTFILE"],
        prefixes: &[],
    },
    // Proxies and trusted certificates. curl documents the `<scheme>_PROXY` form, which the
    // shape in `ENDINGS` covers; OpenSSL reads every `OPENSSL_` variable.
    Rule {
        doc: "https://curl.se/docs/manpage.html#ENVIRONMENT",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "FTP_PROXY",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "SSLKEYLOGFILE",
            "CURL_CA_BUNDLE",
            "REQUESTS_CA_BUNDLE",
            "GRPC_DEFAULT_SSL_ROOTS_FILE_PATH",
        ],
        prefixes: &["OPENSSL_"],
    },
    // Not re-checked in this unit: the AWS SDKs, Docker and Terraform.
    Rule {
        doc: "https://docs.aws.amazon.com/sdkref/latest/guide/settings-reference.html",
        class: ReservedClass::TrafficRedirect,
        exact: &["AWS_CA_BUNDLE", "AWS_ENDPOINT_URL", "DOCKER_HOST"],
        prefixes: &["AWS_ENDPOINT_URL_"],
    },
    Rule {
        doc: "https://developer.hashicorp.com/terraform/cli/config/environment-variables",
        class: ReservedClass::HelperProgram,
        exact: &[
            "AWS_CONFIG_FILE",
            "DOCKER_CONFIG",
            "TF_CLI_ARGS",
            "TF_CLI_CONFIG_FILE",
        ],
        prefixes: &["TF_CLI_ARGS_"],
    },
    // CORE-R2-02: files that a program reads its credentials or its settings from, and the
    // services that hand out credentials. The `FILE` and `CONFIG` shapes in `ENDINGS` cover
    // most of them (AWS_SHARED_CREDENTIALS_FILE, AWS_WEB_IDENTITY_TOKEN_FILE,
    // AZURE_FEDERATED_TOKEN_FILE, GIT_CONFIG); these are the names that the shapes miss.
    // From the documentation of each tool; not re-checked online in this unit.
    Rule {
        doc: "https://googleapis.dev/python/google-auth/latest/reference/google.auth.environment_vars.html",
        class: ReservedClass::SettingsFile,
        exact: &["GOOGLE_APPLICATION_CREDENTIALS"],
        prefixes: &[],
    },
    Rule {
        doc: "https://googleapis.dev/python/google-auth/latest/reference/google.auth.environment_vars.html",
        class: ReservedClass::TrafficRedirect,
        exact: &["GCE_METADATA_HOST", "GCE_METADATA_IP", "GCE_METADATA_ROOT"],
        prefixes: &[],
    },
    Rule {
        doc: "https://cloud.google.com/iam/docs/workload-identity-federation-with-other-providers",
        class: ReservedClass::HelperProgram,
        // It lets a credential file make the library run a program.
        exact: &["GOOGLE_EXTERNAL_ACCOUNT_ALLOW_EXECUTABLES"],
        prefixes: &[],
    },
    Rule {
        doc: "https://cloud.google.com/sdk/docs/properties",
        class: ReservedClass::TrafficRedirect,
        // gcloud reads every property from CLOUDSDK_<SECTION>_<PROPERTY>: endpoints,
        // proxies, certificate files and credential files among them.
        exact: &[],
        prefixes: &["CLOUDSDK_"],
    },
    Rule {
        doc: "https://cloud.google.com/sdk/docs/configurations",
        class: ReservedClass::UserLocation,
        exact: &["CLOUDSDK_CONFIG"],
        prefixes: &[],
    },
    Rule {
        doc: "https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/",
        class: ReservedClass::SettingsFile,
        exact: &["KUBECONFIG"],
        prefixes: &[],
    },
    Rule {
        doc: "https://kubernetes.io/docs/reference/kubectl/generated/kubectl_diff/",
        class: ReservedClass::HelperProgram,
        exact: &["KUBECTL_EXTERNAL_DIFF"],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.aws.amazon.com/sdkref/latest/guide/feature-container-credentials.html",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
            "AWS_EC2_METADATA_SERVICE_ENDPOINT",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://learn.microsoft.com/en-us/python/api/overview/azure/identity-readme",
        class: ReservedClass::TrafficRedirect,
        exact: &[
            "AZURE_AUTHORITY_HOST",
            "IDENTITY_ENDPOINT",
            "IMDS_ENDPOINT",
            "MSI_ENDPOINT",
        ],
        prefixes: &[],
    },
    Rule {
        doc: "https://docs.docker.com/reference/cli/docker/",
        class: ReservedClass::TrafficRedirect,
        exact: &["DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH", "BUILDKIT_HOST"],
        prefixes: &[],
    },
];

const BUILD_VARIABLE_BASES: &[&str] = &[
    // https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables
    // (CC also covers CC_MASM_ASM and the CC_* settings of cc itself).
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
    // https://github.com/rust-lang/cmake-rs/blob/master/src/lib.rs (getenv_target_os);
    // CMAKE also covers CMAKE_TOOLCHAIN_FILE, CMAKE_GENERATOR and every CMake setting.
    "CMAKE",
    "EMCMAKE",
    "EMMAKE",
    // https://docs.rs/pkg-config/latest/pkg_config/
    "PKG_CONFIG",
];

/// The build-variable shape of the cc, cmake and pkg-config crates:
/// `[HOST_|TARGET_]<base>[_<target>]`, where the target is in the hyphen or the underscore
/// spelling (`CC_x86_64-unknown-linux-gnu`, `CC_x86_64_unknown_linux_gnu`). Any text after
/// `<base>_` counts as a target, because a custom target can have any name. The cc crate
/// documents the shape; the cmake and pkg-config crates read their variables the same way.
fn is_build_variable(upper: &str) -> bool {
    let rest = upper
        .strip_prefix("HOST_")
        .or_else(|| upper.strip_prefix("TARGET_"))
        .unwrap_or(upper);
    BUILD_VARIABLE_BASES.iter().any(|base| {
        rest == *base
            || rest
                .strip_prefix(base)
                .and_then(|tail| tail.strip_prefix('_'))
                .is_some_and(|target| !target.is_empty())
    })
}

/// The shapes that the documented names of the families share: the last `_`-separated
/// token of a name says that its value is a program, its options, a location or a proxy.
/// For example GIT_PAGER and GH_PAGER, RUSTC_WRAPPER, CMAKE_<LANG>_COMPILER_LAUNCHER,
/// CARGO_TARGET_<triple>_RUNNER and _LINKER, GIT_SSH_COMMAND, TF_CLI_ARGS, JAVA_OPTS,
/// NODE_OPTIONS, DOTNET_STARTUP_HOOKS, NODE_PATH, NUGET_PLUGIN_PATHS, CARGO_HOME, CMake's
/// <PackageName>_ROOT, DENO_DIR, XDG_CONFIG_DIRS, curl's <scheme>_PROXY, and the settings and
/// credential files AWS_SHARED_CREDENTIALS_FILE, AWS_WEB_IDENTITY_TOKEN_FILE and GIT_CONFIG.
const ENDINGS: &[(&str, ReservedClass)] = &[
    ("PAGER", ReservedClass::HelperProgram),
    ("EDITOR", ReservedClass::HelperProgram),
    ("BROWSER", ReservedClass::HelperProgram),
    ("ASKPASS", ReservedClass::HelperProgram),
    ("COMMAND", ReservedClass::HelperProgram),
    ("WRAPPER", ReservedClass::HelperProgram),
    ("LAUNCHER", ReservedClass::HelperProgram),
    ("RUNNER", ReservedClass::HelperProgram),
    ("LINKER", ReservedClass::HelperProgram),
    ("ARGS", ReservedClass::HelperProgram),
    ("OPTS", ReservedClass::StartupHook),
    ("OPTIONS", ReservedClass::StartupHook),
    ("HOOKS", ReservedClass::StartupHook),
    ("PATH", ReservedClass::ProcessControl),
    ("PATHS", ReservedClass::ProcessControl),
    ("HOME", ReservedClass::UserLocation),
    ("ROOT", ReservedClass::UserLocation),
    ("DIR", ReservedClass::UserLocation),
    ("DIRS", ReservedClass::UserLocation),
    ("PROXY", ReservedClass::TrafficRedirect),
    ("FILE", ReservedClass::SettingsFile),
    ("CONFIG", ReservedClass::SettingsFile),
];

/// The flags of a tool: a last token that ends in FLAGS and is longer, as in CFLAGS,
/// LDFLAGS, RUSTFLAGS, GOFLAGS, CGO_LDFLAGS and every <LANG>FLAGS of make and CMake.
/// `FEATURE_FLAGS` is no tool's flags.
fn is_tool_flags(last: &str) -> bool {
    last.len() > "FLAGS".len() && last.ends_with("FLAGS")
}

fn reserved_rule(upper: &str) -> Option<ReservedClass> {
    if let Some(rule) = RESERVED.iter().find(|rule| rule.exact.contains(&upper)) {
        return Some(rule.class);
    }
    if is_registry_credential(upper) {
        return None;
    }
    if let Some(class) = RESERVED
        .iter()
        .flat_map(|rule| rule.prefixes.iter().map(move |prefix| (rule.class, prefix)))
        .filter(|(_, prefix)| upper.starts_with(*prefix))
        .max_by_key(|(_, prefix)| prefix.len())
        .map(|(class, _)| class)
    {
        return Some(class);
    }
    if is_build_variable(upper) {
        return Some(ReservedClass::HelperProgram);
    }
    let last = upper.rsplit('_').next().unwrap_or(upper);
    if is_tool_flags(last) {
        return Some(ReservedClass::HelperProgram);
    }
    ENDINGS
        .iter()
        .find(|(word, _)| *word == last)
        .map(|(_, class)| *class)
}

/// The reserved class of a variable name, or `None` when the name is free: the one rule for
/// every path that stores, imports, plans or hands over a variable (FR-02, CORE-R1-02,
/// CORE-R2-02). Windows compares variable names without regard to case, so this does too.
/// The rules decide in this order: a whole name of the table, a registry credential (free),
/// the longest prefix of the table, the build-variable shape, the flags shape, and the shape
/// of the last token. The table is finite and is not complete. A free name is handed over as
/// stored, whatever words it holds: AKM has no credential class.
pub fn reserved_class(name: &str) -> Option<ReservedClass> {
    reserved_rule(&name.to_ascii_uppercase())
}

pub(crate) fn validate_process_var(name: &str, value: &str) -> crate::Result<()> {
    if !is_valid_env_var(name) {
        return Err(crate::Error::Invalid(format!(
            "{name:?} is not a valid environment variable name"
        )));
    }
    if let Some(class) = reserved_class(name) {
        return Err(crate::Error::Invalid(format!(
            "{name:?} is reserved: {}",
            class.reason()
        )));
    }
    if value.is_empty() || value.contains('\0') {
        return Err(crate::Error::Invalid(format!(
            "{name:?} must have a non-empty value without NUL characters"
        )));
    }
    Ok(())
}

/// "OpenAI — Personal" -> "OPENAI_PERSONAL"
pub fn derive_env_var(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_was_sep = true;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "API_KEY".to_string()
    } else if out.as_bytes()[0].is_ascii_digit() {
        format!("KEY_{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upper_name_part(text: &str) -> bool {
        !text.is_empty()
            && text.chars().all(|letter| {
                letter.is_ascii_uppercase() || letter.is_ascii_digit() || letter == '_'
            })
    }

    /// Each family names the documentation that its names come from, and holds only names
    /// and prefixes in the upper-case form that `reserved_class` compares against.
    #[test]
    fn every_rule_names_its_documentation_and_holds_upper_case_names() {
        for rule in RESERVED {
            assert!(
                rule.doc.starts_with("https://") || rule.doc == "docs/PROJECT.md",
                "{:?}",
                rule.doc
            );
            assert!(!rule.exact.is_empty() || !rule.prefixes.is_empty());
            for name in rule.exact {
                assert!(is_valid_env_var(name), "{name}");
            }
            for prefix in rule.prefixes {
                assert!(upper_name_part(prefix), "{prefix}");
            }
        }
        for word in BUILD_VARIABLE_BASES
            .iter()
            .chain(ENDINGS.iter().map(|(word, _)| word))
        {
            assert!(upper_name_part(word), "{word}");
        }
    }

    /// A name or a prefix sits in one rule only, so its class has one source.
    #[test]
    fn no_name_or_prefix_sits_in_two_rules() {
        let mut names = std::collections::HashSet::new();
        let mut prefixes = std::collections::HashSet::new();
        for rule in RESERVED {
            for name in rule.exact {
                assert!(names.insert(*name), "{name} is in two rules");
            }
            for prefix in rule.prefixes {
                assert!(prefixes.insert(*prefix), "{prefix} is in two rules");
            }
        }
    }

    /// Every whole name of the table has the class of its own rule: no other rule or shape
    /// takes it away.
    #[test]
    fn every_whole_name_of_the_table_has_the_class_of_its_rule() {
        for rule in RESERVED {
            for name in rule.exact {
                assert_eq!(reserved_class(name), Some(rule.class), "{name}");
            }
        }
    }
}
