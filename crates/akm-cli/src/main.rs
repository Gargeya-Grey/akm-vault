use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use akm_core::audit::AuditLog;
use akm_core::{ApiKey, ApiKeyInput, Error, Result, Vault};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Args, Parser, Subcommand};
use zeroize::Zeroizing;

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
            eprintln!("error: this vault needs an explicit format upgrade before writing");
            eprintln!(
                "Run `akm upgrade` with the same vault, or `akm upgrade --yes` for automation."
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("error: {error}");
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

            println!("Created {}", vault.path().display());
            println!();
            println!("Back up this file and remember the master password.");
            println!("There is no recovery path. The file also opens in KeePassXC.");
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
            println!("Added {:?} ({})", key.title, key.id);
            println!("  env var: {}", key.resolved_env_var());
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
                println!("{}", serde_json::to_string_pretty(&redacted)?);
                return Ok(0);
            }

            print_table(&keys);
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
            if secret_field || (field.is_none() && reveal) {
                vault.log("reveal-requested", Some(&key), Some("disclosure requested"))?;
            }

            match field.as_deref() {
                Some(field) => print_field(&key, field)?,
                None => print_key(&key, reveal),
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
                print!("Delete {:?} ({})? [y/N] ", key.title, key.id);
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("Cancelled.");
                    return Ok(0);
                }
            }

            vault.remove(&key.id)?;
            print_warnings(&mut vault);
            println!("Deleted {:?}", key.title);
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

            vault.log(
                "export-requested",
                None,
                Some(&format!(
                    "disclosure requested; {} keys selected",
                    envs.len()
                )),
            )?;
            drop(vault);

            match output {
                Some(path) => export::write_new(&path, &envs)?,
                None => export::write(&mut std::io::stdout().lock(), &envs)?,
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
                println!("Vault is already KDBX 4.1; no upgrade needed.");
                return Ok(0);
            }
            if !yes {
                print!("Upgrade this vault to KDBX 4.1? Older clients may not open it. [y/N] ");
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                    println!("Cancelled.");
                    return Ok(0);
                }
            }
            vault.upgrade()?;
            print_warnings(&mut vault);
            println!("Upgraded to KDBX 4.1.");
        }

        Commands::Audit { limit, verify } => {
            let log = AuditLog::for_vault(&path);
            let snapshot = log.snapshot(limit)?;
            use akm_core::audit::AuditState;
            match snapshot.state {
                AuditState::Absent => println!(
                    "Journal unavailable: no journal file at {}",
                    log.path().display()
                ),
                AuditState::Empty => println!("No records available at {}", log.path().display()),
                AuditState::Consistent => {
                    println!("Recorded chain is consistent, not authenticated.")
                }
                AuditState::Corrupt => {
                    let sequence = snapshot
                        .broken_at
                        .map(|seq| format!(", sequence {seq}"))
                        .unwrap_or_default();
                    println!("Journal data is damaged at physical line {}{sequence}. Original bytes were preserved.", snapshot.problem_line.unwrap_or(0));
                }
            }
            println!("Completeness unknown. Missing records, truncation and rewritten history cannot be ruled out. Requested events do not prove completion; legacy action outcomes are unknown.");
            let bad_status = snapshot.state == AuditState::Corrupt
                || (verify && snapshot.state != AuditState::Consistent);
            let entries = snapshot.entries;
            if entries.is_empty() {
                println!("No records displayed. This is not proof of inactivity.");
                return Ok(if bad_status { 2 } else { 0 });
            }
            for entry in entries {
                println!(
                    "#{:<4} {} {:<8} {}{}",
                    entry.seq,
                    entry.ts.format("%Y-%m-%d %H:%M:%S"),
                    entry.action,
                    entry.key_title.unwrap_or_else(|| "-".to_string()),
                    entry
                        .detail
                        .map(|detail| format!("  ({detail})"))
                        .unwrap_or_default(),
                );
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
    for warning in vault.take_warnings() {
        eprintln!("warning: {warning}");
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

fn print_table(keys: &[ApiKey]) {
    if keys.is_empty() {
        println!("no keys");
        return;
    }

    let title_width = keys
        .iter()
        .map(|key| key.title.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);

    println!(
        "{:<title_width$}  {:<12}  {:<10}  {:<24}  {:<10}  ROTATION",
        "TITLE", "PROJECT", "ENV", "ENV VAR", "EXPIRES"
    );

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

        println!(
            "{:<title_width$}  {:<12}  {:<10}  {:<24}  {:<10}  {}",
            key.title,
            key.project.as_deref().unwrap_or("-"),
            key.environment.as_deref().unwrap_or("-"),
            key.resolved_env_var(),
            expires,
            rotation,
        );
    }

    println!("\n{} key(s)", keys.len());
}

fn print_key(key: &ApiKey, reveal: bool) {
    println!("Title:       {}", key.title);
    println!("ID:          {}", key.id);
    println!("Project:     {}", key.project.as_deref().unwrap_or("-"));
    println!("Environment: {}", key.environment.as_deref().unwrap_or("-"));
    println!("Env var:     {}", key.resolved_env_var());
    println!("Issuer:      {}", key.issuer.as_deref().unwrap_or("-"));
    println!(
        "Scopes:      {}",
        if key.scopes.is_empty() {
            "-".to_string()
        } else {
            key.scopes.join(", ")
        }
    );
    println!("URL:         {}", key.url.as_deref().unwrap_or("-"));

    let now = Utc::now();
    match key.expires_at {
        Some(date) if date <= now => println!("Expires:     {} (EXPIRED)", date.format("%Y-%m-%d")),
        Some(date) => println!("Expires:     {}", date.format("%Y-%m-%d")),
        None => println!("Expires:     -"),
    }

    if let Some(days) = key.rotation_due_in(now) {
        println!(
            "Rotation:    {} days ({days}d)",
            key.rotation_days.unwrap_or(0)
        );
    } else {
        println!("Rotation:    -");
    }

    if let Some(notes) = &key.notes {
        println!("Notes:       {notes}");
    }

    if reveal {
        println!("Secret:      {}", key.secret);
    } else {
        println!("Secret:      {}", mask(&key.secret));
    }
}

fn print_field(key: &ApiKey, field: &str) -> Result<()> {
    match field.trim().to_lowercase().as_str() {
        "id" => println!("{}", key.id),
        "title" => println!("{}", key.title),
        "secret" => println!("{}", key.secret),
        "project" => println!("{}", key.project.as_deref().unwrap_or("")),
        "env" | "environment" => println!("{}", key.environment.as_deref().unwrap_or("")),
        "issuer" => println!("{}", key.issuer.as_deref().unwrap_or("")),
        "url" => println!("{}", key.url.as_deref().unwrap_or("")),
        "notes" => println!("{}", key.notes.as_deref().unwrap_or("")),
        "scopes" => println!("{}", key.scopes.join(",")),
        "env_var" | "env-var" => println!("{}", key.resolved_env_var()),
        "expires" => println!(
            "{}",
            key.expires_at
                .map(|date| date.to_rfc3339())
                .unwrap_or_default()
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

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
