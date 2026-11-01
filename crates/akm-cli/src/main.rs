use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use akm_core::audit::AuditLog;
use akm_core::{ApiKey, ApiKeyInput, Error, Result, Vault};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Args, Parser, Subcommand};
use zeroize::Zeroizing;

// Output rules (contract 12). The std print macros panic when a pipe is closed, so
// `clippy.toml` bans them in this crate. Every print takes one of two forms:
// - primary output (a listing, a field, an export) writes to an explicit writer and
//   returns its error with `?`, so a failed write is an ordinary error, never a panic;
// - text after the result is decided (a saved change, a warning) uses `status!` or
//   `warning!`. Both are best effort: a closed pipe never changes the exit.

/// Status text after the result is decided. A failed write is ignored.
macro_rules! status {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// A warning on stderr. A failed write is ignored.
macro_rules! warning {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stderr(), "warning: {}", format_args!($($arg)*));
    }};
}

mod child;
mod export;

#[derive(Parser)]
#[command(
    name = "akm",
    version,
    about = "Personal API key manager backed by a KeePass KDBX vault"
)]
struct Cli {
    /// Path to the .kdbx vault (defaults to the platform data directory)
    #[arg(long, global = true, env = "AKM_VAULT", hide_env_values = true)]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct Selection {
    #[arg(long, required_unless_present = "all", value_parser = non_empty_scope)]
    project: Option<String>,

    #[arg(long = "env", required_unless_present = "all", value_parser = non_empty_scope)]
    environment: Option<String>,

    /// Explicitly select every key, without project or environment filters
    #[arg(long, conflicts_with_all = ["project", "environment"])]
    all: bool,
}

fn non_empty_scope(value: &str) -> std::result::Result<String, String> {
    if value.trim().is_empty() {
        return Err("scope must not be empty".into());
    }
    Ok(value.to_string())
}

#[derive(Args)]
struct AddArgs {
    title: String,

    #[arg(long)]
    project: Option<String>,

    #[arg(long = "env")]
    environment: Option<String>,

    #[arg(long)]
    issuer: Option<String>,

    #[arg(long, value_delimiter = ',')]
    scopes: Vec<String>,

    #[arg(long = "env-var")]
    env_var: Option<String>,

    #[arg(long)]
    url: Option<String>,

    #[arg(long)]
    notes: Option<String>,

    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,

    /// Expiry date, YYYY-MM-DD
    #[arg(long)]
    expires: Option<String>,

    /// Rotation interval in days
    #[arg(long)]
    rotation_days: Option<u32>,

    #[arg(long)]
    username: Option<String>,

    /// Read the secret from stdin (single line, trailing newline stripped)
    #[arg(long)]
    secret_stdin: bool,

    /// Pass the secret as an argument. Visible in the process list; avoid.
    #[arg(long)]
    secret: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new vault
    Init {
        #[arg(long)]
        name: Option<String>,
    },

    /// Add a key (prompts for the secret unless --secret-stdin is used)
    Add(Box<AddArgs>),

    /// List keys (secrets are never printed)
    List {
        #[arg(long)]
        project: Option<String>,

        #[arg(long = "env")]
        environment: Option<String>,

        #[arg(long)]
        json: bool,

        /// Only show keys expiring within N days
        #[arg(long)]
        expiring: Option<i64>,
    },

    /// Show one key, masked by default
    Get {
        /// Case-insensitive substring of title, project, tag, issuer or URL
        query: String,

        /// Print a single field: id, title, secret, project, env, issuer, url,
        /// notes, scopes, env_var, expires
        #[arg(long)]
        field: Option<String>,

        /// Reveal the full secret in the human-readable output
        #[arg(long)]
        reveal: bool,
    },

    /// Delete a key
    Rm {
        query: String,

        #[arg(long)]
        yes: bool,
    },

    /// Run a command with matching keys injected as environment variables
    Run {
        #[command(flatten)]
        selection: Selection,

        #[arg(last = true, required = true)]
        command: Vec<String>,
    },

    /// Print matching keys as a dotenv file
    Export {
        #[command(flatten)]
        selection: Selection,

        /// Write a new UTF-8 file instead of stdout. Never overwrites a file.
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Explicitly upgrade an older vault to KDBX 4.1 before writing
    Upgrade {
        /// Confirm the format upgrade without an interactive prompt
        #[arg(long)]
        yes: bool,
    },

    /// Show or verify the access log
    Audit {
        #[arg(long, default_value_t = 20)]
        limit: usize,

        #[arg(long)]
        verify: bool,
    },
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(Error::UpgradeRequired) => {
            let _ = writeln!(
                std::io::stderr(),
                "error: this vault needs an explicit format upgrade before writing\n\
                 Run `akm upgrade` with the same vault, or `akm upgrade --yes` for automation."
            );
            std::process::exit(1);
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "error: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    let Cli { vault, command } = Cli::parse();

    let path = vault.unwrap_or_else(default_vault_path);

    match command {
        Commands::Init { name } => {
            let password = master_password(true)?;
            let mut vault = Vault::create(&path, &password, name.as_deref())?;
            drop(password);
            print_warnings(&mut vault);

            status!(
                "Created {}\n\nBack up this file and remember the master password.\n\
                 There is no recovery path. The file also opens in KeePassXC.",
                vault.path().display()
            );
        }

        Commands::Add(args) => {
            let AddArgs {
                title,
                project,
                environment,
                issuer,
                scopes,
                env_var,
                url,
                notes,
                tags,
                expires,
                rotation_days,
                username,
                secret_stdin,
                secret,
            } = *args;
            let password = master_password(false)?;
            let mut vault = Vault::open(&path, &password)?;
            drop(password);
            let secret = secret_value(secret, secret_stdin)?;

            let input = ApiKeyInput {
                title,
                secret,
                username,
                issuer,
                project,
                environment,
                scopes,
                env_var,
                url,
                notes,
                tags,
                expires_at: parse_expiry(expires.as_deref())?,
                rotation_days,
            };

            let key = vault.add(input)?;
            print_warnings(&mut vault);
            status!(
                "Added {:?} ({})\n  env var: {}",
                key.title,
                key.id,
                key.resolved_env_var()
            );
        }

        Commands::List {
            project,
            environment,
            json,
            expiring,
        } => {
            let password = master_password(false)?;
            let vault = Vault::open(&path, &password)?;
            drop(password);

            let mut keys = match expiring {
                Some(days) => vault.expiring(days),
                None => vault.list(),
            };
            keys.retain(|key| {
                project
                    .as_deref()
                    .is_none_or(|project| key.project.as_deref() == Some(project))
                    && environment
                        .as_deref()
                        .is_none_or(|env| key.environment.as_deref() == Some(env))
            });
            keys.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

            if json {
                let mut redacted = keys;
                for key in &mut redacted {
                    key.secret = "***".to_string();
                }
                writeln!(
                    std::io::stdout(),
                    "{}",
                    serde_json::to_string_pretty(&redacted)?
                )?;
                return Ok(0);
            }

            print_table(&mut std::io::stdout().lock(), &keys)?;
        }

        Commands::Get {
            query,
            field,
            reveal,
        } => {
            let password = master_password(false)?;
            let vault = Vault::open(&path, &password)?;
            drop(password);
            let key = vault.find(&query)?;

            let secret_field = field
                .as_deref()
                .is_some_and(|field| field.trim().eq_ignore_ascii_case("secret"));
            let disclosure = secret_field || (field.is_none() && reveal);
            if disclosure {
                vault.log("reveal-requested", Some(&key), Some("disclosure requested"))?;
            }

            let mut out = std::io::stdout().lock();
            let shown = match field.as_deref() {
                Some(field) => print_field(&mut out, &key, field),
                None => print_key(&mut out, &key, reveal).map_err(Error::from),
            };
            match shown {
                Err(Error::Io(error)) if disclosure => {
                    return Err(partial_output(error, "show the secret"))
                }
                shown => shown?,
            }
        }

        Commands::Rm { query, yes } => {
            let password = master_password(false)?;
            let mut vault = Vault::open(&path, &password)?;
            drop(password);
            let key = vault.find(&query)?;

            if !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(Error::Invalid(
                        "refusing to delete without confirmation; pass --yes".into(),
                    ));
                }
                write!(
                    std::io::stdout(),
                    "Delete {:?} ({})? [y/N] ",
                    key.title,
                    key.id
                )?;
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    status!("Cancelled.");
                    return Ok(0);
                }
            }

            vault.remove(&key.id)?;
            print_warnings(&mut vault);
            status!("Deleted {:?}", key.title);
        }

        Commands::Run { selection, command } => {
            let program = command
                .first()
                .ok_or_else(|| Error::Invalid("no command given".into()))?;
            let mut command = child::prepare(program, &command[1..])?;
            {
                let password = master_password(false)?;
                let vault = Vault::open(&path, &password)?;
                drop(password);
                let envs = selected_envs(&vault, &selection)?;
                child::inject(&mut command, &envs)?;
                vault.log(
                    "run-requested",
                    None,
                    Some(&format!("launch requested; {} keys selected", envs.len())),
                )?;
            }

            let mut child = command.spawn()?;
            drop(command);
            return Ok(child.wait()?.code().unwrap_or(1));
        }

        Commands::Export { selection, output } => {
            let output = output.as_deref().map(export::destination).transpose()?;
            let password = master_password(false)?;
            let vault = Vault::open(&path, &password)?;
            drop(password);
            let envs = selected_envs(&vault, &selection)?;
            // Pairs the two records; the destination stays out of the plaintext journal.
            let operation = uuid::Uuid::new_v4();

            vault.log(
                "export-requested",
                None,
                Some(&format!(
                    "operation {operation}; disclosure requested; {} keys selected",
                    envs.len()
                )),
            )?;
            drop(vault);

            match output {
                Some(path) => export::write_new(&path, &envs)?,
                None => write_stdout_export(&mut std::io::stdout().lock(), &envs)?,
            }
            if let Some(warning) =
                record_export_completed(&AuditLog::for_vault(&path), &operation, envs.len())
            {
                warning!("{warning}");
            }
        }

        Commands::Upgrade { yes } => {
            if !yes && !std::io::stdin().is_terminal() {
                return Err(Error::Invalid(
                    "refusing to upgrade without confirmation; pass --yes".into(),
                ));
            }
            let password = master_password(false)?;
            let mut vault = Vault::open(&path, &password)?;
            drop(password);
            if !vault.needs_upgrade() {
                status!("Vault is already KDBX 4.1; no upgrade needed.");
                return Ok(0);
            }
            if !yes {
                write!(
                    std::io::stdout(),
                    "Upgrade this vault to KDBX 4.1? Older clients may not open it. [y/N] "
                )?;
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                    status!("Cancelled.");
                    return Ok(0);
                }
            }
            vault.upgrade()?;
            print_warnings(&mut vault);
            status!("Upgraded to KDBX 4.1.");
        }

        Commands::Audit { limit, verify } => {
            let log = AuditLog::for_vault(&path);
            let snapshot = log.snapshot(limit)?;
            use akm_core::audit::AuditState;
            let mut out = std::io::stdout().lock();
            match snapshot.state {
                AuditState::Absent => writeln!(
                    out,
                    "Journal unavailable: no journal file at {}",
                    log.path().display()
                )?,
                AuditState::Empty => {
                    writeln!(out, "No records available at {}", log.path().display())?
                }
                AuditState::Consistent => {
                    writeln!(out, "Recorded chain is consistent, not authenticated.")?
                }
                AuditState::Corrupt => {
                    let sequence = snapshot
                        .broken_at
                        .map(|seq| format!(", declared sequence {seq} (not verified)"))
                        .unwrap_or_default();
                    writeln!(out, "Journal data is damaged at physical line {}{sequence}. Original bytes were preserved.", snapshot.problem_line.unwrap_or(0))?;
                }
            }
            writeln!(out, "Completeness unknown. Missing records, truncation and rewritten history cannot be ruled out. Requested events do not prove completion; legacy action outcomes are unknown.")?;
            let bad_status = snapshot.state == AuditState::Corrupt
                || (verify && snapshot.state != AuditState::Consistent);
            let entries = snapshot.entries;
            if entries.is_empty() {
                writeln!(
                    out,
                    "No records displayed. This is not proof of inactivity."
                )?;
                return Ok(if bad_status { 2 } else { 0 });
            }
            for entry in entries {
                writeln!(
                    out,
                    "#{:<4} {} {:<8} {}{}",
                    entry.seq,
                    entry.ts.format("%Y-%m-%d %H:%M:%S"),
                    entry.action,
                    entry.key_title.unwrap_or_else(|| "-".to_string()),
                    entry
                        .detail
                        .map(|detail| format!("  ({detail})"))
                        .unwrap_or_default(),
                )?;
            }
            return Ok(if bad_status { 2 } else { 0 });
        }
    }

    Ok(0)
}

fn selected_envs(vault: &Vault, selection: &Selection) -> Result<child::Secrets> {
    let envs = child::Secrets(vault.env_vars(
        selection.project.as_deref(),
        selection.environment.as_deref(),
        selection.all,
    )?);
    if envs.is_empty() {
        return Err(Error::Invalid(
            "no keys matched the explicit selection".into(),
        ));
    }
    child::validate(&envs)?;
    Ok(envs)
}

fn print_warnings(vault: &mut Vault) {
    for text in vault.take_warnings() {
        warning!("{text}");
    }
}

fn default_vault_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("akm")
        .join("vault.kdbx")
}

fn master_password(confirm: bool) -> Result<Zeroizing<String>> {
    match std::env::var("AKM_PASSWORD") {
        Ok(password) => return Ok(Zeroizing::new(password)),
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Error::Invalid("AKM_PASSWORD must be valid Unicode".into()));
        }
    }

    if !std::io::stdin().is_terminal() {
        return Err(Error::Invalid(
            "no terminal available; set AKM_PASSWORD for automation".into(),
        ));
    }

    let password = Zeroizing::new(rpassword::prompt_password("Master password: ")?);
    if password.is_empty() {
        return Err(Error::Invalid("master password must not be empty".into()));
    }

    if confirm {
        let again = Zeroizing::new(rpassword::prompt_password("Confirm master password: ")?);
        if password != again {
            return Err(Error::Invalid("passwords do not match".into()));
        }
    }

    Ok(password)
}

fn secret_value(given: Option<String>, from_stdin: bool) -> Result<String> {
    if let Some(secret) = given {
        return Ok(secret);
    }

    if from_stdin {
        let mut secret = String::new();
        std::io::stdin().read_to_string(&mut secret)?;
        let secret = secret.trim_end_matches(['\r', '\n']).to_string();
        if secret.is_empty() {
            return Err(Error::Invalid("secret read from stdin was empty".into()));
        }
        return Ok(secret);
    }

    let secret = rpassword::prompt_password("Secret value: ")?;
    if secret.is_empty() {
        return Err(Error::Invalid("secret must not be empty".into()));
    }
    Ok(secret)
}

fn parse_expiry(raw: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| Error::Invalid(format!("invalid date {raw:?}")))?;
        return Ok(Some(midnight.and_utc()));
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(raw) {
        return Ok(Some(datetime.with_timezone(&Utc)));
    }

    Err(Error::Invalid(format!(
        "could not parse expiry {raw:?}; use YYYY-MM-DD"
    )))
}

fn print_table(out: &mut impl Write, keys: &[ApiKey]) -> std::io::Result<()> {
    if keys.is_empty() {
        return writeln!(out, "no keys");
    }

    let title_width = keys
        .iter()
        .map(|key| key.title.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);

    writeln!(
        out,
        "{:<title_width$}  {:<12}  {:<10}  {:<24}  {:<10}  ROTATION",
        "TITLE", "PROJECT", "ENV", "ENV VAR", "EXPIRES"
    )?;

    let now = Utc::now();
    for key in keys {
        let expires = key
            .expires_at
            .map(|date| {
                if date <= now {
                    format!("{} EXPIRED", date.format("%Y-%m-%d"))
                } else {
                    date.format("%Y-%m-%d").to_string()
                }
            })
            .unwrap_or_else(|| "-".to_string());

        let rotation = key
            .rotation_due_in(now)
            .map(|days| {
                if days < 0 {
                    format!("{}d overdue", -days)
                } else {
                    format!("in {days}d")
                }
            })
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            out,
            "{:<title_width$}  {:<12}  {:<10}  {:<24}  {:<10}  {}",
            key.title,
            key.project.as_deref().unwrap_or("-"),
            key.environment.as_deref().unwrap_or("-"),
            key.resolved_env_var(),
            expires,
            rotation,
        )?;
    }

    writeln!(out, "\n{} key(s)", keys.len())
}

fn print_key(out: &mut impl Write, key: &ApiKey, reveal: bool) -> std::io::Result<()> {
    writeln!(out, "Title:       {}", key.title)?;
    writeln!(out, "ID:          {}", key.id)?;
    writeln!(
        out,
        "Project:     {}",
        key.project.as_deref().unwrap_or("-")
    )?;
    writeln!(
        out,
        "Environment: {}",
        key.environment.as_deref().unwrap_or("-")
    )?;
    writeln!(out, "Env var:     {}", key.resolved_env_var())?;
    writeln!(out, "Issuer:      {}", key.issuer.as_deref().unwrap_or("-"))?;
    writeln!(
        out,
        "Scopes:      {}",
        if key.scopes.is_empty() {
            "-".to_string()
        } else {
            key.scopes.join(", ")
        }
    )?;
    writeln!(out, "URL:         {}", key.url.as_deref().unwrap_or("-"))?;

    let now = Utc::now();
    match key.expires_at {
        Some(date) if date <= now => {
            writeln!(out, "Expires:     {} (EXPIRED)", date.format("%Y-%m-%d"))?
        }
        Some(date) => writeln!(out, "Expires:     {}", date.format("%Y-%m-%d"))?,
        None => writeln!(out, "Expires:     -")?,
    }

    if let Some(days) = key.rotation_due_in(now) {
        writeln!(
            out,
            "Rotation:    {} days ({days}d)",
            key.rotation_days.unwrap_or(0)
        )?;
    } else {
        writeln!(out, "Rotation:    -")?;
    }

    if let Some(notes) = &key.notes {
        writeln!(out, "Notes:       {notes}")?;
    }

    if reveal {
        writeln!(out, "Secret:      {}", key.secret)?;
    } else {
        writeln!(out, "Secret:      {}", mask(&key.secret))?;
    }
    Ok(())
}

fn print_field(out: &mut impl Write, key: &ApiKey, field: &str) -> Result<()> {
    match field.trim().to_lowercase().as_str() {
        "id" => writeln!(out, "{}", key.id)?,
        "title" => writeln!(out, "{}", key.title)?,
        "secret" => writeln!(out, "{}", key.secret)?,
        "project" => writeln!(out, "{}", key.project.as_deref().unwrap_or(""))?,
        "env" | "environment" => writeln!(out, "{}", key.environment.as_deref().unwrap_or(""))?,
        "issuer" => writeln!(out, "{}", key.issuer.as_deref().unwrap_or(""))?,
        "url" => writeln!(out, "{}", key.url.as_deref().unwrap_or(""))?,
        "notes" => writeln!(out, "{}", key.notes.as_deref().unwrap_or(""))?,
        "scopes" => writeln!(out, "{}", key.scopes.join(","))?,
        "env_var" | "env-var" => writeln!(out, "{}", key.resolved_env_var())?,
        "expires" => writeln!(out, 
            "{}",
            key.expires_at
                .map(|date| date.to_rfc3339())
                .unwrap_or_default()
        )?,
        other => {
            return Err(Error::Invalid(format!(
                "unknown field {other:?}; try id, title, secret, project, env, issuer, url, notes, scopes, env_var, expires"
            )))
        }
    }
    Ok(())
}

fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "********".to_string();
    }

    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}...{tail}")
}

/// A disclosure write can deliver bytes before a later write or flush fails.
fn partial_output(error: impl std::fmt::Display, again: &str) -> Error {
    Error::Invalid(format!(
        "{error}. Some output may already have been written. Check it before you {again} again."
    ))
}

fn write_stdout_export(output: &mut impl Write, envs: &[(String, String)]) -> Result<()> {
    export::write(output, envs).map_err(|error| partial_output(error, "export"))
}

/// Runs only after the export is known to be written. A failure is a warning, never a
/// retryable error: the disclosure happened. The record may exist if only its sync failed.
fn record_export_completed(log: &AuditLog, operation: &uuid::Uuid, keys: usize) -> Option<String> {
    record_completed_with(operation, keys, |detail| {
        log.append("export-completed", None, None, None, None, Some(detail))
    })
}

/// The seam for `record_export_completed`: `append` writes the record and confirms it.
fn record_completed_with(
    operation: &uuid::Uuid,
    keys: usize,
    append: impl FnOnce(&str) -> Result<()>,
) -> Option<String> {
    append(&format!("operation {operation}; export completed; {keys} keys"))
        .err()
        .map(|error| {
            format!(
                "The export was written, but its completion record could not be confirmed in the activity log ({error}). Do not repeat the export."
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter {
        accept: usize,
        fail_flush: bool,
        written: Vec<u8>,
    }
    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.written.len() >= self.accept {
                return Err(std::io::Error::other("injected pipe failure"));
            }
            let count = buf.len().min(self.accept - self.written.len());
            self.written.extend_from_slice(&buf[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                return Err(std::io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }

    #[test]
    fn a_failed_stdout_export_warns_that_output_may_be_partial() {
        let envs = vec![("FAKE_NAME".to_string(), "fake-partial-secret".to_string())];
        for (accept, fail_flush) in [(4, false), (usize::MAX, true)] {
            let mut writer = FailingWriter {
                accept,
                fail_flush,
                written: Vec::new(),
            };
            let error = write_stdout_export(&mut writer, &envs)
                .unwrap_err()
                .to_string();
            assert!(
                !writer.written.is_empty(),
                "the writer accepted bytes first"
            );
            assert!(
                error.contains("Some output may already have been written"),
                "{error}"
            );
            assert!(!error.contains("fake-partial-secret"));
        }
    }

    #[test]
    fn an_unconfirmed_completion_record_is_a_non_retryable_warning() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, b"fake").unwrap();
        let log = AuditLog::for_vault(&blocker.join("vault.kdbx"));
        let operation = uuid::Uuid::new_v4();
        let warning = record_export_completed(&log, &operation, 2).unwrap();
        assert!(warning.contains("could not be confirmed"), "{warning}");
        assert!(warning.contains("Do not repeat the export"), "{warning}");

        let log = AuditLog::for_vault(&directory.path().join("vault.kdbx"));
        assert!(record_export_completed(&log, &operation, 2).is_none());
        let entry = &log.read(1).unwrap()[0];
        assert_eq!(entry.action, "export-completed");
        assert_eq!(
            entry.detail.as_deref(),
            Some(format!("operation {operation}; export completed; 2 keys").as_str())
        );
    }

    #[test]
    fn a_completion_record_written_before_a_failed_sync_is_still_a_warning() {
        let directory = tempfile::tempdir().unwrap();
        let log = AuditLog::for_vault(&directory.path().join("vault.kdbx"));
        let operation = uuid::Uuid::new_v4();
        let warning = record_completed_with(&operation, 3, |detail| {
            log.append("export-completed", None, None, None, None, Some(detail))?;
            Err(std::io::Error::other("injected journal sync failure").into())
        })
        .unwrap();
        assert!(warning.contains("could not be confirmed"), "{warning}");
        assert!(warning.contains("Do not repeat the export"), "{warning}");
        // The line can remain: the warning says "could not be confirmed", not "was not written".
        let entry = &log.read(1).unwrap()[0];
        assert_eq!(entry.action, "export-completed");
        assert_eq!(
            entry.detail.as_deref(),
            Some(format!("operation {operation}; export completed; 3 keys").as_str())
        );
    }

    #[test]
    fn masks_short_and_long_secrets() {
        assert_eq!(mask("short"), "********");
        assert_eq!(mask("sk-1234567890abcdef"), "sk-1...cdef");
    }

    #[test]
    fn parses_date_and_rfc3339() {
        assert!(parse_expiry(Some("2027-01-31")).unwrap().is_some());
        assert!(parse_expiry(Some("2027-01-31T10:00:00Z"))
            .unwrap()
            .is_some());
        assert!(parse_expiry(Some("soon")).is_err());
    }
}
