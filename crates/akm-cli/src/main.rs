use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use akm_core::audit::AuditLog;
use akm_core::schedule::{parse_expiry, Remaining};
use akm_core::{ApiKey, ApiKeyInput, EnvDisclosure, EnvPlan, Error, KeyDisclosure, Result, Vault};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use zeroize::Zeroizing;

// Output rules (contract 12). The std print macros panic when a pipe is closed, so
// `clippy.toml` bans them in this crate. Every print goes to a writer in `Io` and takes
// one of two forms:
// - primary output (a listing, a field, an export) returns its error with `?`, so a
//   failed write is an ordinary error, never a panic;
// - text after the result is decided (a saved change, a warning) uses `status!` or
//   `warning!`. Both are best effort: a closed pipe never changes the exit.

/// Status text after the result is decided. A failed write is ignored.
macro_rules! status {
    ($io:expr, $($arg:tt)*) => {{
        let _ = writeln!($io.out, $($arg)*);
    }};
}

/// A warning on stderr. A failed write is ignored.
macro_rules! warning {
    ($io:expr, $($arg:tt)*) => {{
        let _ = writeln!($io.err, "warning: {}", format_args!($($arg)*));
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

    /// Expiry: a date YYYY-MM-DD (it ends at 23:59:59 UTC that day) or a full RFC 3339 time
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
        /// Id or exact title of a key (any letter case). A partial word is refused
        /// and the refusal lists candidates.
        query: String,

        /// Print a single field: id, title, secret, project, env, issuer, url,
        /// notes, scopes, env_var, expires
        #[arg(long)]
        field: Option<String>,

        /// Reveal the full secret in the human-readable output
        #[arg(long)]
        reveal: bool,
    },

    /// Replace the secret of a key and record the time (the new secret comes from the
    /// prompt or from stdin, never from an argument)
    Rotate {
        /// Id or exact title of a key (any letter case). A partial word is refused
        /// and the refusal lists candidates.
        query: String,

        /// Read the new secret from stdin (single line, trailing newline stripped)
        #[arg(long)]
        secret_stdin: bool,
    },

    /// List the earlier versions of a key: times and field names, never values
    History {
        /// Id or exact title of a key, live or in the Recycle Bin
        query: String,
    },

    /// Move a key to the Recycle Bin. Restore it with `restore`; delete it for good with `purge`
    Rm {
        /// Id or exact title of a key (any letter case)
        query: String,

        #[arg(long)]
        yes: bool,
    },

    /// List the keys in the Recycle Bin
    Bin,

    /// Move a key from the Recycle Bin back to the key list
    Restore {
        /// Id or exact title of a key in the Recycle Bin
        query: String,
    },

    /// Delete for good: one key from the Recycle Bin, or every recycled key older than N days
    Purge {
        /// Id or exact title of a key in the Recycle Bin (with --live, a live key)
        #[arg(required_unless_present = "older_than_days")]
        query: Option<String>,

        /// Delete every recycled key that has been in the Recycle Bin for more than N days
        #[arg(long, conflicts_with = "query")]
        older_than_days: Option<u32>,

        /// Delete a live key for good instead of moving it to the Recycle Bin
        #[arg(long, requires = "query")]
        live: bool,

        /// Confirm without an interactive prompt
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

/// Where a file export is published. `main` builds the real one; a test can watch the
/// file path (unit 1h, design D10).
trait FilePublisher {
    fn write_new(&mut self, path: &Path, envs: &[(String, String)]) -> Result<()>;
}

struct SystemFiles;
impl FilePublisher for SystemFiles {
    fn write_new(&mut self, path: &Path, envs: &[(String, String)]) -> Result<()> {
        export::write_new(path, envs)
    }
}

/// The process outputs, the file publisher, and the journal append for the export-completion
/// record. `main` builds the real ones (design `unit1a-repair.md`, section 2.6).
struct Io<'a> {
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
    files: &'a mut dyn FilePublisher,
    complete: fn(&AuditLog, &str) -> Result<()>,
}

fn main() {
    let cli = Cli::parse();
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    let code = run_with(
        cli,
        &mut Io {
            out: &mut out,
            err: &mut err,
            files: &mut SystemFiles,
            complete: append_completion,
        },
    );
    std::process::exit(code);
}

/// Runs one command and returns the process exit code.
fn run_with(cli: Cli, io: &mut Io) -> i32 {
    match run(cli, io) {
        Ok(code) => code,
        Err(Error::UpgradeRequired) => {
            let _ = writeln!(
                io.err,
                "error: this vault needs an explicit format upgrade before writing\n\
                 Run `akm upgrade` with the same vault, or `akm upgrade --yes` for automation."
            );
            1
        }
        Err(error) => {
            let _ = writeln!(io.err, "error: {error}");
            1
        }
    }
}

fn run(cli: Cli, io: &mut Io) -> Result<i32> {
    let Cli { vault, command } = cli;

    let path = vault.unwrap_or_else(default_vault_path);

    match command {
        Commands::Init { name } => {
            let password = master_password(true)?;
            let mut vault = Vault::create(&path, &password, name.as_deref())?;
            drop(password);
            print_warnings(io, &mut vault);

            status!(
                io,
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
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            let secret = secret_value(secret, secret_stdin)?;

            let input = ApiKeyInput {
                title,
                secret: secret.into(),
                username,
                issuer,
                project,
                environment,
                scopes,
                env_var,
                url,
                notes,
                tags,
                expires_at: expires.as_deref().map(parse_expiry).transpose()?,
                rotation_days,
            };

            let key = vault.add(input)?;
            print_warnings(io, &mut vault);
            status!(
                io,
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
            let vault = open_vault(&path, &password)?;
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
                writeln!(io.out, "{}", listing_json(&keys)?)?;
                return Ok(0);
            }

            print_table(&mut io.out, &keys, vault.now())?;
        }

        Commands::Get {
            query,
            field,
            reveal,
        } => {
            let password = master_password(false)?;
            let vault = open_vault(&path, &password)?;
            drop(password);
            let key = select(&vault, &query)?;

            let secret_field = field
                .as_deref()
                .is_some_and(|field| field.trim().eq_ignore_ascii_case("secret"));
            let disclosure = secret_field || (field.is_none() && reveal);
            // 1H-04: only the two disclosures read the secret, and `Vault::secret` needs the
            // proof that `disclose_key` returns after the journal append. A failed append
            // returns here, before any secret is read.
            let secret = if disclosure {
                let proof = vault.disclose_key(
                    KeyDisclosure::Reveal,
                    &key,
                    Some("disclosure requested"),
                )?;
                Some(vault.secret(&key.id, &proof)?)
            } else {
                None
            };

            // 1A-R8-01: the default view and every other field receive `KeyMetadata`.
            let metadata = KeyMetadata::new(&key, vault.now());
            let out = &mut *io.out;
            let shown = match (
                field.as_deref(),
                secret.as_ref().map(|secret| secret.as_str()),
            ) {
                (Some(_), Some(secret)) if secret_field => {
                    writeln!(out, "{secret}").map_err(Error::from)
                }
                (Some(field), _) => print_field(out, &metadata, field),
                (None, revealed) => print_key(out, &metadata, revealed).map_err(Error::from),
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
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            let key = select(&vault, &query)?;

            if !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(Error::Invalid(
                        "refusing to delete without confirmation; pass --yes".into(),
                    ));
                }
                write!(io.out, "Delete {:?} ({})? [y/N] ", key.title, key.id)?;
                io.out.flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    status!(io, "Cancelled.");
                    return Ok(0);
                }
            }

            vault.remove(&key.id)?;
            print_warnings(io, &mut vault);
            status!(
                io,
                "Moved {:?} to the Recycle Bin.\nRestore it with `akm restore {id}`. Delete it for good with `akm purge {id}`.",
                key.title,
                id = key.id
            );
        }

        Commands::Rotate {
            query,
            secret_stdin,
        } => {
            let password = master_password(false)?;
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            let key = select(&vault, &query)?;
            let secret = secret_value(None, secret_stdin)?;
            let rotation = vault.rotate(&key.id, secret.into())?;
            print_warnings(io, &mut vault);
            status!(
                io,
                "Rotated {:?} ({})\n  Secret changed:          {}\n  Previous secret change:  {}\nThe old secret stays in the vault history. Rotating here does not change the secret at the provider.",
                rotation.key.title,
                rotation.key.id,
                show_time(rotation.changed_at),
                rotation
                    .previous_changed_at
                    .map_or_else(|| "unknown".to_string(), show_time)
            );
        }

        Commands::History { query } => {
            let password = master_password(false)?;
            let vault = open_vault(&path, &password)?;
            drop(password);
            let (id, title) = match vault.find(&query) {
                Ok(key) => (key.id, key.title),
                Err(Error::EntryNotFound(_)) => {
                    let recycled = select_recycled(&vault, &query, "")?;
                    (recycled.key.id, recycled.key.title)
                }
                Err(error) => {
                    return Err(selection_error(
                        &query,
                        error,
                        &vault.candidates(&query),
                        "",
                    ))
                }
            };
            let versions = vault.history(&id)?;
            if versions.is_empty() {
                writeln!(io.out, "no earlier versions of {title:?} ({id})")?;
                return Ok(0);
            }
            writeln!(
                io.out,
                "{} earlier version(s) of {title:?} ({id}), newest first. Values are never shown.",
                versions.len()
            )?;
            for version in versions {
                writeln!(
                    io.out,
                    "#{:<3} {}  {}  {}",
                    version.index,
                    version
                        .modified_at
                        .map_or_else(|| "unknown time".to_string(), show_time),
                    if version.secret_changed {
                        "secret changed"
                    } else {
                        "secret kept   "
                    },
                    version.changed.join(", ")
                )?;
            }
        }

        Commands::Bin => {
            let password = master_password(false)?;
            let vault = open_vault(&path, &password)?;
            drop(password);
            let recycled = vault.recycled();
            if recycled.is_empty() {
                writeln!(io.out, "The Recycle Bin is empty.")?;
                return Ok(0);
            }
            for item in &recycled {
                writeln!(
                    io.out,
                    "{}  {}  {}",
                    item.key.id,
                    item.deleted_at
                        .map_or_else(|| "unknown time".to_string(), show_time),
                    item.key.title
                )?;
            }
            writeln!(
                io.out,
                "\n{} recycled key(s). Nothing is deleted for good until you run `akm purge`.",
                recycled.len()
            )?;
        }

        Commands::Restore { query } => {
            let password = master_password(false)?;
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            let recycled = select_recycled(&vault, &query, "")?;
            let key = vault.restore(&recycled.key.id)?;
            print_warnings(io, &mut vault);
            status!(io, "Restored {:?} ({})", key.title, key.id);
        }

        Commands::Purge {
            query,
            older_than_days,
            live,
            yes,
        } => {
            let password = master_password(false)?;
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            match (query, older_than_days) {
                (Some(query), None) => {
                    let (id, title) = if live {
                        let key = select(&vault, &query)?;
                        (key.id, key.title)
                    } else {
                        let recycled = select_recycled(
                            &vault,
                            &query,
                            " in the Recycle Bin (to delete a live key for good, pass --live)",
                        )?;
                        (recycled.key.id, recycled.key.title)
                    };
                    if !confirmed(
                        io,
                        yes,
                        "delete for good",
                        &format!("Delete {title:?} ({id}) for good? This cannot be undone."),
                    )? {
                        status!(io, "Cancelled.");
                        return Ok(0);
                    }
                    vault.purge(&id)?;
                    print_warnings(io, &mut vault);
                    status!(
                        io,
                        "Deleted {title:?} for good. A deletion record was stored. Older copies and backups of this vault can still hold the key, and a KeePassXC merge with an older copy can bring it back."
                    );
                }
                (None, Some(days)) => {
                    let due = vault
                        .recycled()
                        .iter()
                        .filter(|item| {
                            item.deleted_at.is_some_and(|at| {
                                at < vault.now() - chrono::Duration::days(i64::from(days))
                            })
                        })
                        .count();
                    if due == 0 {
                        status!(
                            io,
                            "Nothing to purge: no recycled key is older than {days} days."
                        );
                        return Ok(0);
                    }
                    if !confirmed(
                        io,
                        yes,
                        "delete for good",
                        &format!("Delete {due} recycled key(s) older than {days} days for good? This cannot be undone."),
                    )? {
                        status!(io, "Cancelled.");
                        return Ok(0);
                    }
                    let purged = vault.purge_recycled_older_than(days)?;
                    print_warnings(io, &mut vault);
                    status!(
                        io,
                        "Deleted {} key{} for good.",
                        purged.len(),
                        if purged.len() == 1 { "" } else { "s" }
                    );
                }
                _ => unreachable!("clap requires exactly one of a key or --older-than-days"),
            }
        }

        Commands::Run { selection, command } => {
            let program = command
                .first()
                .ok_or_else(|| Error::Invalid("no command given".into()))?;
            let mut command = child::prepare(program, &command[1..])?;
            {
                let password = master_password(false)?;
                let vault = open_vault(&path, &password)?;
                drop(password);
                let plan = planned_envs(&vault, &selection)?;
                let envs = disclosed_envs(
                    &vault,
                    EnvDisclosure::Run,
                    &plan,
                    &format!("launch requested; {} keys selected", plan.len()),
                )?;
                child::inject(&mut command, &envs)?;
            }

            let mut child = command.spawn()?;
            drop(command);
            return Ok(child.wait()?.code().unwrap_or(1));
        }

        Commands::Export { selection, output } => {
            let output = output.as_deref().map(export::destination).transpose()?;
            let password = master_password(false)?;
            let vault = open_vault(&path, &password)?;
            drop(password);
            let plan = planned_envs(&vault, &selection)?;
            // Pairs the two records; the destination stays out of the journal.
            let operation = uuid::Uuid::new_v4();
            let journal = vault.audit_log();
            let envs = disclosed_envs(
                &vault,
                EnvDisclosure::Export,
                &plan,
                &format!(
                    "operation {operation}; disclosure requested; {} keys selected",
                    plan.len()
                ),
            )?;
            drop(vault);

            match output {
                Some(path) => io.files.write_new(&path, &envs)?,
                None => write_stdout_export(&mut io.out, &envs)?,
            }
            if let Some(warning) =
                record_export_completed(io.complete, &journal, &operation, envs.len())
            {
                warning!(io, "{warning}");
            }
        }

        Commands::Upgrade { yes } => {
            if !yes && !std::io::stdin().is_terminal() {
                return Err(Error::Invalid(
                    "refusing to upgrade without confirmation; pass --yes".into(),
                ));
            }
            let password = master_password(false)?;
            let mut vault = open_vault(&path, &password)?;
            drop(password);
            if !vault.needs_upgrade() {
                status!(io, "Vault is already KDBX 4.1; no upgrade needed.");
                return Ok(0);
            }
            if !yes {
                write!(
                    io.out,
                    "Upgrade this vault to KDBX 4.1? Older clients may not open it. [y/N] "
                )?;
                io.out.flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                    status!(io, "Cancelled.");
                    return Ok(0);
                }
            }
            vault.upgrade()?;
            print_warnings(io, &mut vault);
            status!(io, "Upgraded to KDBX 4.1.");
        }

        Commands::Audit { limit, verify } => {
            let snapshot = AuditLog::for_vault(&path).snapshot(limit)?;
            use akm_core::audit::AuditState;
            let out = &mut *io.out;
            match snapshot.state {
                AuditState::Absent => writeln!(
                    out,
                    "Journal unavailable: no journal exists for this vault on this device yet."
                )?,
                AuditState::Empty => writeln!(
                    out,
                    "No records available in {}",
                    snapshot.location.display()
                )?,
                AuditState::Consistent => {
                    writeln!(out, "Recorded chain is consistent, not authenticated.")?
                }
                AuditState::Corrupt => {
                    let sequence = snapshot
                        .broken_at
                        .map(|seq| format!(", declared sequence {seq} (not verified)"))
                        .unwrap_or_default();
                    let what = snapshot
                        .damage
                        .map(|damage| format!(": {}", damage.describe()))
                        .unwrap_or_default();
                    writeln!(out, "Journal data is damaged at physical line {}{what}{sequence}. Original bytes were preserved.", snapshot.problem_line.unwrap_or(0))?;
                }
            }
            for notice in &snapshot.notices {
                writeln!(out, "Notice: {}", notice.message)?;
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
                    entry
                        .key_id
                        .map(|id| format!("key {}", id.chars().take(8).collect::<String>()))
                        .unwrap_or_else(|| "-".to_string()),
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

/// Opens the vault. A debug build reads `AKM_TEST_CLOCK` (an RFC 3339 time) so that a test
/// can fix every time that the vault writes. A release build never reads it.
const MAX_CANDIDATES: usize = 12;

fn candidate_line(key: &ApiKey) -> String {
    format!(
        "  {}  {:?}  project {:?}, env {:?}",
        key.id,
        key.title,
        key.project.as_deref().unwrap_or("-"),
        key.environment.as_deref().unwrap_or("-")
    )
}

/// L2: a refusal names candidates by id, title, project and environment. It never shows a
/// secret, a note or a URL.
fn selection_error(query: &str, error: Error, candidates: &[ApiKey], place: &str) -> Error {
    let mut list = String::new();
    if !candidates.is_empty() {
        list.push_str("\nCandidates (select one by its id):");
        for key in candidates.iter().take(MAX_CANDIDATES) {
            list.push('\n');
            list.push_str(&candidate_line(key));
        }
        if candidates.len() > MAX_CANDIDATES {
            list.push_str(&format!(
                "\n  and {} more",
                candidates.len() - MAX_CANDIDATES
            ));
        }
    }
    match error {
        Error::EntryNotFound(_) => Error::Invalid(format!(
            "no key has the id or exact title {query:?}{place}; select a key by its exact title or id{list}"
        )),
        Error::Ambiguous(_, count) => Error::Invalid(format!(
            "{query:?} is the exact title of {count} keys; select one by its id{list}"
        )),
        other => other,
    }
}

fn select(vault: &Vault, query: &str) -> Result<ApiKey> {
    vault
        .find(query)
        .map_err(|error| selection_error(query, error, &vault.candidates(query), ""))
}

fn select_recycled(vault: &Vault, query: &str, place: &str) -> Result<akm_core::RecycledKey> {
    vault.find_recycled(query).map_err(|error| {
        let candidates: Vec<ApiKey> = vault
            .recycled_candidates(query)
            .into_iter()
            .map(|item| item.key)
            .collect();
        selection_error(query, error, &candidates, place)
    })
}

fn open_vault(path: &Path, password: &str) -> Result<Vault> {
    let vault = Vault::open(path, password)?;
    with_test_clock(vault)
}

#[cfg(debug_assertions)]
fn with_test_clock(vault: Vault) -> Result<Vault> {
    struct Fixed(DateTime<Utc>);
    impl akm_core::Clock for Fixed {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }
    match std::env::var("AKM_TEST_CLOCK") {
        Ok(text) => {
            let time = DateTime::parse_from_rfc3339(&text)
                .map_err(|_| Error::Invalid("AKM_TEST_CLOCK must be an RFC 3339 time".into()))?;
            Ok(vault.with_clock(std::sync::Arc::new(Fixed(time.with_timezone(&Utc)))))
        }
        Err(_) => Ok(vault),
    }
}

#[cfg(not(debug_assertions))]
fn with_test_clock(vault: Vault) -> Result<Vault> {
    Ok(vault)
}

/// A time as the CLI prints it.
fn show_time(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// A line-based yes/no on the terminal, or a refusal when there is none and `--yes` is absent.
fn confirmed(io: &mut Io, yes: bool, what: &str, question: &str) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(Error::Invalid(format!(
            "refusing to {what} without confirmation; pass --yes"
        )));
    }
    write!(io.out, "{question} [y/N] ")?;
    io.out.flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// The selection's names, checked before anything is journaled or read.
fn planned_envs(vault: &Vault, selection: &Selection) -> Result<EnvPlan> {
    let plan = vault.plan_env(
        selection.project.as_deref(),
        selection.environment.as_deref(),
        selection.all,
    )?;
    if plan.is_empty() {
        return Err(Error::Invalid(
            "no keys matched the explicit selection".into(),
        ));
    }
    child::validate_names(plan.names())?;
    Ok(plan)
}

/// Records the request, then reads the values. A failed record returns before any value
/// is read (contract 12).
fn disclosed_envs(
    vault: &Vault,
    kind: EnvDisclosure,
    plan: &EnvPlan,
    detail: &str,
) -> Result<child::Secrets> {
    let proof = vault.disclose_env(kind, plan, Some(detail))?;
    Ok(child::Secrets(vault.env_vars(plan, &proof)?))
}

/// `list --json`: the key fields, and the legacy `"secret": "***"` placeholder that
/// scripts read. A key carries no secret to leak here.
fn listing_json(keys: &[ApiKey]) -> Result<String> {
    let mut value = serde_json::to_value(keys)?;
    for key in value.as_array_mut().into_iter().flatten() {
        if let Some(object) = key.as_object_mut() {
            object.insert("secret".into(), "***".into());
        }
    }
    Ok(serde_json::to_string_pretty(&value)?)
}

fn print_warnings(io: &mut Io, vault: &mut Vault) {
    for text in vault.take_warnings() {
        warning!(io, "{text}");
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
        let secret = secret
            .strip_prefix('\u{feff}')
            .unwrap_or(&secret)
            .trim_end_matches(['\r', '\n'])
            .to_string();
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

/// Whole days round up while time is left, and count full days once it is late. A key that is
/// late by less than a day reads "overdue", never "in 0d".
fn remaining_text(remaining: Remaining) -> String {
    match remaining {
        Remaining::In { days } => format!("in {days}d"),
        Remaining::Now => "due now".to_string(),
        Remaining::Overdue { days: 0 } => "overdue".to_string(),
        Remaining::Overdue { days } => format!("{days}d overdue"),
    }
}

fn print_table(out: &mut impl Write, keys: &[ApiKey], now: DateTime<Utc>) -> std::io::Result<()> {
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

        let rotation = match (key.rotation_days, key.rotation_remaining(now)) {
            (_, Some(remaining)) => remaining_text(remaining),
            (Some(_), None) => "unknown".to_string(),
            (None, None) => "-".to_string(),
        };

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

/// The default `get` line for the secret: the same for every value and every length.
const MASK: &str = "********";

/// The key fields that the `get` view and `--field` print. It has no secret field, so
/// those outputs cannot contain the secret or anything derived from it (1A-R8-01).
struct KeyMetadata {
    id: String,
    title: String,
    project: Option<String>,
    environment: Option<String>,
    env_var: String,
    issuer: Option<String>,
    scopes: Vec<String>,
    url: Option<String>,
    notes: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    rotation_days: Option<u32>,
    rotation_remaining: Option<Remaining>,
    secret_changed_at: Option<DateTime<Utc>>,
    history_count: usize,
    now: DateTime<Utc>,
}

impl KeyMetadata {
    fn new(key: &ApiKey, now: DateTime<Utc>) -> Self {
        Self {
            id: key.id.clone(),
            title: key.title.clone(),
            project: key.project.clone(),
            environment: key.environment.clone(),
            env_var: key.resolved_env_var(),
            issuer: key.issuer.clone(),
            scopes: key.scopes.clone(),
            url: key.url.clone(),
            notes: key.notes.clone(),
            expires_at: key.expires_at,
            rotation_days: key.rotation_days,
            rotation_remaining: key.rotation_remaining(now),
            secret_changed_at: key.secret_changed_at,
            history_count: key.history_count,
            now,
        }
    }
}

/// The `get` view. `revealed` is `Some` only for `--reveal`, after its record.
fn print_key(
    out: &mut dyn Write,
    key: &KeyMetadata,
    revealed: Option<&str>,
) -> std::io::Result<()> {
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
    writeln!(out, "Env var:     {}", key.env_var)?;
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

    let now = key.now;
    match key.expires_at {
        Some(date) if date <= now => {
            writeln!(out, "Expires:     {} (EXPIRED)", date.format("%Y-%m-%d"))?
        }
        Some(date) => writeln!(out, "Expires:     {}", date.format("%Y-%m-%d"))?,
        None => writeln!(out, "Expires:     -")?,
    }

    match (key.rotation_days, key.rotation_remaining) {
        (Some(interval), Some(remaining)) => writeln!(
            out,
            "Rotation:    {interval} days ({})",
            remaining_text(remaining)
        )?,
        (Some(interval), None) => writeln!(
            out,
            "Rotation:    {interval} days (due date unknown: no secret-change time is recorded)"
        )?,
        (None, _) => writeln!(out, "Rotation:    -")?,
    }
    writeln!(
        out,
        "Secret changed: {}",
        key.secret_changed_at
            .map_or_else(|| "unknown".to_string(), show_time)
    )?;
    writeln!(
        out,
        "History: {}",
        match key.history_count {
            0 => "none".to_string(),
            1 => "1 earlier version".to_string(),
            n => format!("{n} earlier versions"),
        }
    )?;

    if let Some(notes) = &key.notes {
        writeln!(out, "Notes:       {notes}")?;
    }

    writeln!(out, "Secret:      {}", revealed.unwrap_or(MASK))
}

/// A non-secret field. `--field secret` never reaches this function.
fn print_field(out: &mut dyn Write, key: &KeyMetadata, field: &str) -> Result<()> {
    match field.trim().to_lowercase().as_str() {
        "id" => writeln!(out, "{}", key.id)?,
        "title" => writeln!(out, "{}", key.title)?,
        "project" => writeln!(out, "{}", key.project.as_deref().unwrap_or(""))?,
        "env" | "environment" => writeln!(out, "{}", key.environment.as_deref().unwrap_or(""))?,
        "issuer" => writeln!(out, "{}", key.issuer.as_deref().unwrap_or(""))?,
        "url" => writeln!(out, "{}", key.url.as_deref().unwrap_or(""))?,
        "notes" => writeln!(out, "{}", key.notes.as_deref().unwrap_or(""))?,
        "scopes" => writeln!(out, "{}", key.scopes.join(","))?,
        "env_var" | "env-var" => writeln!(out, "{}", key.env_var)?,
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

/// The real `Io::complete`: writes the export-completion record and confirms it.
fn append_completion(log: &AuditLog, detail: &str) -> Result<()> {
    log.append("export-completed", None, Some(detail))
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
fn record_export_completed(
    complete: fn(&AuditLog, &str) -> Result<()>,
    log: &AuditLog,
    operation: &uuid::Uuid,
    keys: usize,
) -> Option<String> {
    complete(log, &format!("operation {operation}; export completed; {keys} keys"))
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
    fn a_failed_real_completion_append_is_a_non_retryable_warning() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, b"fake").unwrap();
        let log = AuditLog::new(
            akm_core::journal::JournalRoot::at(blocker.join("journal")),
            akm_core::journal::VaultEvidence::from_file(&blocker.join("vault.kdbx")),
        );
        let operation = uuid::Uuid::new_v4();
        let warning = record_export_completed(append_completion, &log, &operation, 2).unwrap();
        assert!(warning.contains("could not be confirmed"), "{warning}");
        assert!(warning.contains("Do not repeat the export"), "{warning}");
    }

    // 1A-R8-04: the export path through `run_with`, the function `main` calls.

    const PASSWORD: &str = "fake-cli-unit-password";
    const SECRET: &str = "fake-export-secret";

    struct Exported {
        code: i32,
        out: String,
        err: String,
        /// The export records, in journal order: (action, operation).
        journal: Vec<(String, String)>,
    }

    fn export_through_run_with(complete: fn(&AuditLog, &str) -> Result<()>) -> Exported {
        let (_directory, path) = fake_export_vault();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(
            export_cli(&path),
            &mut Io {
                out: &mut out,
                err: &mut err,
                files: &mut SystemFiles,
                complete,
            },
        );
        Exported {
            code,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
            journal: export_journal(&path),
        }
    }

    /// A fake vault with one key in demo/dev, and `AKM_PASSWORD` set for it.
    fn fake_export_vault() -> (tempfile::TempDir, std::path::PathBuf) {
        // Every caller sets the same fake value, so tests that run in parallel agree.
        std::env::set_var("AKM_PASSWORD", PASSWORD);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        let mut vault = Vault::create(&path, PASSWORD, Some("Fake")).unwrap();
        vault
            .add(ApiKeyInput {
                title: "Demo".into(),
                secret: SECRET.into(),
                project: Some("demo".into()),
                environment: Some("dev".into()),
                env_var: Some("DEMO_SECRET".into()),
                ..Default::default()
            })
            .unwrap();
        drop(vault);
        (directory, path)
    }

    /// `akm export --project demo --env dev`, to stdout.
    fn export_cli(path: &std::path::Path) -> Cli {
        Cli::try_parse_from([
            "akm",
            "--vault",
            path.to_str().unwrap(),
            "export",
            "--project",
            "demo",
            "--env",
            "dev",
        ])
        .unwrap()
    }

    /// The export records in journal order, each with its operation ID.
    fn export_journal(path: &std::path::Path) -> Vec<(String, String)> {
        let mut entries = AuditLog::for_vault(path).read(100).unwrap();
        entries.sort_by_key(|entry| entry.seq);
        entries
            .into_iter()
            .filter(|entry| entry.action.starts_with("export-"))
            .map(|entry| {
                let detail = entry.detail.unwrap_or_default();
                let operation = detail.split(';').next().unwrap_or_default().to_string();
                (entry.action, operation)
            })
            .collect()
    }

    /// An export sink that reads the journal at every write (1A-A1-04).
    struct JournalAtEachWrite {
        path: std::path::PathBuf,
        written: Vec<u8>,
        seen: Vec<Vec<String>>,
    }

    impl Write for JournalAtEachWrite {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let actions = export_journal(&self.path).into_iter().map(|(a, _)| a);
            self.seen.push(actions.collect());
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn an_export_records_its_request_before_the_first_byte_and_its_completion_after_the_last() {
        let (_directory, path) = fake_export_vault();
        let mut out = JournalAtEachWrite {
            path: path.clone(),
            written: Vec::new(),
            seen: Vec::new(),
        };
        let mut err = Vec::new();
        let code = run_with(
            export_cli(&path),
            &mut Io {
                out: &mut out,
                err: &mut err,
                files: &mut SystemFiles,
                complete: append_completion,
            },
        );
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        assert!(String::from_utf8_lossy(&out.written).contains(SECRET));
        assert!(!out.seen.is_empty(), "the export wrote through the sink");
        for (write, seen) in out.seen.iter().enumerate() {
            assert_eq!(seen, &["export-requested"], "journal at write {write}");
        }
        let after: Vec<String> = export_journal(&path).into_iter().map(|(a, _)| a).collect();
        assert_eq!(after, ["export-requested", "export-completed"]);
    }

    /// What a file export showed at each step that can disclose or finish it (1A-A2-03).
    #[derive(Default)]
    struct FileObservations {
        /// The journal actions when the first byte reached the temporary file.
        first_byte: Option<Vec<String>>,
        /// The journal actions, and whether the destination existed, just before publication.
        at_publication: Option<(Vec<String>, bool)>,
        bytes_before_publication: Vec<u8>,
    }

    struct ObservedFiles {
        vault: std::path::PathBuf,
        seen: FileObservations,
    }

    /// The real temporary file, real write and real publication, with a look at the journal
    /// before the first byte and before the rename.
    struct Probe<'a> {
        vault: &'a Path,
        seen: &'a mut FileObservations,
    }

    struct FirstByte<'a> {
        file: &'a mut std::fs::File,
        vault: &'a Path,
        seen: &'a mut FileObservations,
    }

    fn journal_actions(vault: &Path) -> Vec<String> {
        export_journal(vault).into_iter().map(|(a, _)| a).collect()
    }

    impl Write for FirstByte<'_> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.seen.first_byte.is_none() {
                self.seen.first_byte = Some(journal_actions(self.vault));
            }
            self.seen.bytes_before_publication.extend_from_slice(buf);
            self.file.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }

    impl export::ExportIo for Probe<'_> {
        fn write(&mut self, file: &mut std::fs::File, envs: &[(String, String)]) -> Result<()> {
            let mut sink = FirstByte {
                file,
                vault: self.vault,
                seen: &mut *self.seen,
            };
            export::write(&mut sink, envs)
        }

        fn publish(&mut self, file: &std::fs::File, destination: &Path) -> Result<()> {
            self.seen.at_publication = Some((journal_actions(self.vault), destination.exists()));
            export::publish_new(file, destination)
        }
    }

    impl FilePublisher for ObservedFiles {
        fn write_new(&mut self, path: &Path, envs: &[(String, String)]) -> Result<()> {
            export::write_new_with(
                path,
                envs,
                &mut Probe {
                    vault: &self.vault,
                    seen: &mut self.seen,
                },
            )
        }
    }

    fn export_file_cli(path: &Path, output: &Path) -> Cli {
        Cli::try_parse_from([
            "akm",
            "--vault",
            path.to_str().unwrap(),
            "export",
            "--project",
            "demo",
            "--env",
            "dev",
            "--output",
            output.to_str().unwrap(),
        ])
        .unwrap()
    }

    // 1A-A2-03: the request is in the journal before the first byte reaches the temporary
    // file, and the completion record follows publication.
    #[test]
    fn a_file_export_records_its_request_before_its_first_byte_and_its_completion_after_publication(
    ) {
        let (directory, path) = fake_export_vault();
        let target = directory.path().join("fake-export.env");
        let mut files = ObservedFiles {
            vault: path.clone(),
            seen: FileObservations::default(),
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(
            export_file_cli(&path, &target),
            &mut Io {
                out: &mut out,
                err: &mut err,
                files: &mut files,
                complete: append_completion,
            },
        );
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        assert!(out.is_empty(), "a file export prints no secret to stdout");
        assert_eq!(
            files.seen.first_byte.as_deref(),
            Some(&["export-requested".to_string()][..]),
            "the request is journaled before the first temporary-file byte"
        );
        assert_eq!(
            files.seen.at_publication,
            Some((vec!["export-requested".to_string()], false)),
            "no completion record before publication, and no destination yet"
        );
        assert!(String::from_utf8_lossy(&files.seen.bytes_before_publication).contains(SECRET));
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "DEMO_SECRET=fake-export-secret\n"
        );
        assert_eq!(
            journal_actions(&path),
            ["export-requested", "export-completed"]
        );
    }

    struct RefusingFiles;
    impl FilePublisher for RefusingFiles {
        fn write_new(&mut self, _: &Path, _: &[(String, String)]) -> Result<()> {
            Err(std::io::Error::other("injected publication failure").into())
        }
    }

    #[test]
    fn a_file_export_that_failed_to_publish_leaves_no_completion_record() {
        let (directory, path) = fake_export_vault();
        let target = directory.path().join("fake-export.env");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(
            export_file_cli(&path, &target),
            &mut Io {
                out: &mut out,
                err: &mut err,
                files: &mut RefusingFiles,
                complete: append_completion,
            },
        );
        assert_ne!(code, 0);
        assert_eq!(journal_actions(&path), ["export-requested"]);
        assert!(!target.exists());
    }

    struct UndeletableTemp;
    impl export::ExportIo for UndeletableTemp {
        fn publish(&mut self, _file: &std::fs::File, _destination: &Path) -> Result<()> {
            Err(std::io::Error::other("injected publication failure").into())
        }

        fn discard(&mut self, _file: &std::fs::File) -> bool {
            false
        }
    }

    struct UndeletableFiles;
    impl FilePublisher for UndeletableFiles {
        fn write_new(&mut self, path: &Path, envs: &[(String, String)]) -> Result<()> {
            export::write_new_with(path, envs, &mut UndeletableTemp)
        }
    }

    // 1A-D2-09: a failed file export that cannot confirm the removal of its plaintext temporary
    // file says so on stderr, exits non-zero, and records no completion.
    #[test]
    fn a_failed_file_export_that_cannot_confirm_cleanup_says_so_and_exits_non_zero() {
        let (directory, path) = fake_export_vault();
        let target = directory.path().join("fake-export.env");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(
            export_file_cli(&path, &target),
            &mut Io {
                out: &mut out,
                err: &mut err,
                files: &mut UndeletableFiles,
                complete: append_completion,
            },
        );
        let err = String::from_utf8_lossy(&err);
        assert_eq!(code, 1, "{err}");
        assert!(err.contains("could not confirm cleanup"), "{err}");
        assert!(err.contains(".akm-export-"), "{err}");
        assert!(!err.contains(SECRET), "{err}");
        assert!(out.is_empty());
        assert!(!target.exists());
        assert_eq!(journal_actions(&path), ["export-requested"]);
    }

    fn actions(run: &Exported) -> Vec<&str> {
        run.journal
            .iter()
            .map(|(action, _)| action.as_str())
            .collect()
    }

    fn written_then_unconfirmed(log: &AuditLog, detail: &str) -> Result<()> {
        append_completion(log, detail)?;
        Err(std::io::Error::other("injected journal sync failure").into())
    }

    fn refused(_: &AuditLog, _: &str) -> Result<()> {
        Err(std::io::Error::other("injected journal failure").into())
    }

    fn assert_one_completion_warning(run: &Exported, cause: &str) {
        assert_eq!(run.code, 0, "the export happened: {}", run.err);
        assert!(run.out.contains(SECRET), "the export reached stdout");
        assert_eq!(run.err.lines().count(), 1, "{}", run.err);
        assert!(
            run.err.starts_with(
                "warning: The export was written, but its completion record could not be confirmed"
            ),
            "{}",
            run.err
        );
        assert!(run.err.contains(cause), "{}", run.err);
        assert!(run.err.contains("Do not repeat the export."), "{}", run.err);
        assert!(!run.err.contains(SECRET));
    }

    #[test]
    fn an_export_whose_completion_record_was_written_but_not_confirmed_warns_and_exits_zero() {
        let run = export_through_run_with(written_then_unconfirmed);
        assert_one_completion_warning(&run, "injected journal sync failure");
        // The line can remain: the warning says "could not be confirmed", not "was not written".
        assert_eq!(actions(&run), ["export-requested", "export-completed"]);
        assert_eq!(run.journal[0].1, run.journal[1].1, "one operation");
    }

    #[test]
    fn an_export_whose_completion_record_failed_warns_and_exits_zero() {
        let run = export_through_run_with(refused);
        assert_one_completion_warning(&run, "injected journal failure");
        assert_eq!(actions(&run), ["export-requested"]);
    }

    #[test]
    fn a_confirmed_export_completion_pairs_with_its_request_and_warns_nothing() {
        let run = export_through_run_with(append_completion);
        assert_eq!(run.code, 0, "{}", run.err);
        assert!(run.out.contains(SECRET));
        assert_eq!(run.err, "");
        assert_eq!(actions(&run), ["export-requested", "export-completed"]);
        assert_eq!(run.journal[0].1, run.journal[1].1, "one operation");
        assert!(
            run.journal[0].1.starts_with("operation "),
            "{:?}",
            run.journal
        );
    }

    #[test]
    fn parses_date_and_rfc3339() {
        assert!(parse_expiry("2027-01-31").is_ok());
        assert!(parse_expiry("2027-01-31T10:00:00Z").is_ok());
        assert!(parse_expiry("soon").is_err());
    }
}
