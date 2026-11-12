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
        if self.title.trim().is_empty() {
            return Err(crate::Error::Invalid("title must not be empty".into()));
        }
        if self.secret.is_empty() {
            return Err(crate::Error::Invalid("secret must not be empty".into()));
        }
        let texts = [
            ("title", Some(&self.title)),
            ("username", self.username.as_ref()),
            ("issuer", self.issuer.as_ref()),
            ("project", self.project.as_ref()),
            ("environment", self.environment.as_ref()),
            ("url", self.url.as_ref()),
            ("notes", self.notes.as_ref()),
        ];
        let lists = [("scope", &self.scopes), ("tag", &self.tags)];
        let nul_field = texts
            .iter()
            .find(|(_, value)| value.is_some_and(|value| value.contains('\0')))
            .map(|(label, _)| *label)
            .or_else(|| {
                lists
                    .iter()
                    .find(|(_, values)| values.iter().any(|value| value.contains('\0')))
                    .map(|(label, _)| *label)
            });
        if let Some(label) = nul_field {
            return Err(crate::Error::Invalid(format!(
                "{label} must not contain a NUL character"
            )));
        }
        if let Some(var) = &self.env_var {
            if !is_valid_env_var(var) {
                return Err(crate::Error::Invalid(format!(
                    "{var:?} is not a valid environment variable name (use A-Z, 0-9, _)"
                )));
            }
        }
        let resolved = self
            .env_var
            .clone()
            .unwrap_or_else(|| derive_env_var(&self.title));
        if let Some(class) = reserved_class(&resolved) {
            return Err(crate::Error::Invalid(format!(
                "{resolved:?} is reserved: {}; set a different env var name for this key",
                class.reason()
            )));
        }
        if let Some(expires) = self.expires_at {
            crate::schedule::require_whole_second(expires)?;
        }
        if let Some(days) = self.rotation_days {
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
        Ok(())
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
}

/// The class of a variable name. [`name_class`] is the one rule that decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameClass {
    Reserved(ReservedClass),
    Credential,
    Other,
}

/// SCAFFOLD: the rule before CORE-R1-02, so that the new tests run against it.
pub fn name_class(name: &str) -> NameClass {
    match reserved_class(name) {
        Some(class) => NameClass::Reserved(class),
        None => NameClass::Other,
    }
}

impl ImportChange {
    pub fn name_class(&self) -> NameClass {
        name_class(&self.name)
    }
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
        }
    }
}

struct Rule {
    class: ReservedClass,
    /// Whole names. An exact match beats a prefix match, so a name can sit in a narrower
    /// class than the prefix family around it (`CARGO_HOME` against `CARGO_`).
    exact: &'static [&'static str],
    prefixes: &'static [&'static str],
}

/// Names that look like part of a reserved family and carry a secret that a person stores
/// on purpose: a registry token. They are not reserved (FR-02).
fn is_credential_name(upper: &str) -> bool {
    const REGISTRIES: &str = "CARGO_REGISTRIES_";
    const TOKEN: &str = "_TOKEN";
    upper == "CARGO_REGISTRY_TOKEN"
        || (upper.len() > REGISTRIES.len() + TOKEN.len()
            && upper.starts_with(REGISTRIES)
            && upper.ends_with(TOKEN))
}

const RESERVED: &[Rule] = &[
    Rule {
        class: ReservedClass::AkmInternal,
        exact: &[],
        prefixes: &["AKM_"],
    },
    Rule {
        class: ReservedClass::ProcessControl,
        exact: &[
            "PATH",
            "PATHEXT",
            "COMSPEC",
            "SYSTEMROOT",
            "WINDIR",
            "PSMODULEPATH",
            // Which toolchain, interpreter or runtime a tool picks.
            "GOROOT",
            "GOTOOLCHAIN",
            "JAVA_HOME",
            "GEM_HOME",
            "GEM_PATH",
            "UV_PYTHON",
        ],
        // Every `RUSTUP_` variable but the few below: rustup picks the toolchain it runs.
        prefixes: &["LD_", "DYLD_", "RUSTUP_"],
    },
    Rule {
        class: ReservedClass::StartupHook,
        exact: &[
            "NODE_OPTIONS",
            "NODE_PATH",
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONSTARTUP",
            "PYTHONINSPECT",
            "PYTHONBREAKPOINT",
            "PYTHONUSERBASE",
            "PYTHONEXECUTABLE",
            "PYTHONPLATLIBDIR",
            "PYTHONPYCACHEPREFIX",
            "PERL5OPT",
            "PERL5LIB",
            "PERLLIB",
            "PERL5DB",
            "RUBYOPT",
            "RUBYLIB",
            "JAVA_TOOL_OPTIONS",
            "_JAVA_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "JAVA_OPTS",
            "MAVEN_OPTS",
            "GRADLE_OPTS",
            "ANT_OPTS",
            "SBT_OPTS",
            "CATALINA_OPTS",
            "CLASSPATH",
            "DOTNET_STARTUP_HOOKS",
            "DOTNET_ADDITIONAL_DEPS",
            "ELECTRON_RUN_AS_NODE",
            "PHPRC",
            "PHP_INI_SCAN_DIR",
            "LUA_INIT",
            "LUA_PATH",
            "LUA_CPATH",
            "R_PROFILE",
            "R_PROFILE_USER",
            "R_ENVIRON",
            "R_ENVIRON_USER",
        ],
        prefixes: &["COR_PROFILER", "COR_ENABLE_PROFILING", "CORECLR_"],
    },
    Rule {
        class: ReservedClass::HelperProgram,
        exact: &[
            "GIT_SSH_COMMAND",
            "GIT_SSH",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "SUDO_ASKPASS",
            "GIT_EXEC_PATH",
            "GIT_PROXY_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_TEMPLATE_DIR",
            // Rust: the compiler, its wrappers and flags.
            "RUSTC",
            "RUSTDOC",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            // Compilers and build tools.
            "CC",
            "CXX",
            "CPP",
            "AS",
            "LD",
            "AR",
            "NM",
            "RANLIB",
            "STRIP",
            "OBJCOPY",
            "OBJDUMP",
            "MAKE",
            "MAKEFLAGS",
            "MFLAGS",
            "CFLAGS",
            "CXXFLAGS",
            "CPPFLAGS",
            "LDFLAGS",
            "GOFLAGS",
            "GOAUTH",
            "MAVEN_ARGS",
            // Editors, pagers and shells that tools run.
            "EDITOR",
            "VISUAL",
            "PAGER",
            "MANPAGER",
            "LESSOPEN",
            "LESSCLOSE",
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
        // `RUSTC_`: RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER, RUSTC_BOOTSTRAP and the rest.
        // `CARGO_`: Cargo reads every such variable as a configuration key. Together they
        // cover CARGO_BUILD_RUSTC*, CARGO_ENCODED_RUSTFLAGS and every
        // CARGO_TARGET_<triple>_RUNNER, _LINKER and _RUSTFLAGS. `is_credential_name` takes
        // the registry tokens out again.
        prefixes: &[
            "GIT_CONFIG_",
            "RUSTC_",
            "CARGO_",
            "CMAKE_",
            "CGO_",
            "TF_CLI_ARGS_",
        ],
    },
    Rule {
        class: ReservedClass::ShellStartup,
        exact: &[
            "BASH_ENV",
            "ENV",
            "PROMPT_COMMAND",
            "PS1",
            "PS4",
            "SHELLOPTS",
            // PowerShell and cmd.
            "PSEXECUTIONPOLICYPREFERENCE",
            "__PSLOCKDOWNPOLICY",
            "PROMPT",
            "DIRCMD",
            "COPYCMD",
        ],
        prefixes: &[],
    },
    Rule {
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
            "REQUESTS_CA_BUNDLE",
            "CURL_CA_BUNDLE",
            "AWS_CA_BUNDLE",
            "GRPC_DEFAULT_SSL_ROOTS_FILE_PATH",
            "NODE_EXTRA_CA_CERTS",
            "NODE_TLS_REJECT_UNAUTHORIZED",
            "GIT_SSL_NO_VERIFY",
            "GIT_SSL_CAINFO",
            "GIT_SSL_CAPATH",
            "AWS_ENDPOINT_URL",
            "DOCKER_HOST",
            // Package registries and indexes.
            "GOPROXY",
            "GONOPROXY",
            "GOSUMDB",
            "GONOSUMDB",
            "GOINSECURE",
            "UV_INDEX_URL",
            "UV_DEFAULT_INDEX",
            "UV_EXTRA_INDEX_URL",
            "UV_INDEX",
            "UV_FIND_LINKS",
            "UV_PUBLISH_URL",
            "UV_PYTHON_INSTALL_MIRROR",
            "UV_INSECURE_HOST",
            "YARN_NPM_REGISTRY_SERVER",
            "YARN_HTTP_PROXY",
            "YARN_HTTPS_PROXY",
            "BUN_CONFIG_REGISTRY",
            "COREPACK_NPM_REGISTRY",
            "RUSTUP_DIST_SERVER",
            "RUSTUP_UPDATE_ROOT",
        ],
        prefixes: &[
            "NPM_CONFIG_",
            "PIP_",
            "OPENSSL_",
            "AWS_ENDPOINT_URL_",
            "CARGO_HTTP_",
            "CARGO_NET_",
            "CARGO_SOURCE_",
        ],
    },
    Rule {
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
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            // Where toolchains and package managers keep their settings and files.
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
            "RUSTUP_HOME",
            "GOPATH",
            "GOBIN",
            "GOENV",
            "GOMODCACHE",
            "GOCACHE",
            "BUN_INSTALL",
            "DENO_DIR",
            "PNPM_HOME",
            "GRADLE_USER_HOME",
        ],
        prefixes: &[],
    },
];

/// The reserved class of a name, or `None`. Windows compares variable names without regard
/// to case, so this does too. A whole-name match decides first. Then the longest matching
/// prefix decides (`CARGO_HTTP_` before `CARGO_`), unless the name is a registry token.
pub fn reserved_class(name: &str) -> Option<ReservedClass> {
    let upper = name.to_ascii_uppercase();
    if let Some(rule) = RESERVED
        .iter()
        .find(|rule| rule.exact.contains(&upper.as_str()))
    {
        return Some(rule.class);
    }
    if is_credential_name(&upper) {
        return None;
    }
    RESERVED
        .iter()
        .flat_map(|rule| rule.prefixes.iter().map(move |prefix| (rule.class, prefix)))
        .filter(|(_, prefix)| upper.starts_with(*prefix))
        .max_by_key(|(_, prefix)| prefix.len())
        .map(|(class, _)| class)
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
