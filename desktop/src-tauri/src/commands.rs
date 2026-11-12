use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use akm_core::{
    audit::AuditLog, dotenv, ApiKey, EnvDisclosure, HistoryVersion, KeyDisclosure, Secret, Vault,
};
use chrono::{DateTime, Utc};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    clipboard::CLEAR_AFTER,
    disclosure::{self, Approval, BulkUse, DisclosureReview},
    error::CommandError,
    import_review::{ConfirmState, ImportConfirm, ImportReview},
    launch::{self, TerminalState},
    outcome::{self, Operation},
    private_export,
    reminders::{ReminderState, ReminderStatus},
    review_window::ReviewAnswer,
    session::{
        Access, Admission, Binding, OperationWarningKind, Session, Snapshot, Stamped, VaultState,
        WarningReport,
    },
    settings::{current_path, settings_file, PreferenceState, PreferenceStore},
    views::*,
};

/// The one list of commands that the app registers. The IPC dispatch test expands the
/// same macro, so a command cannot be missing there and present in the app.
macro_rules! production_handlers {
    () => {
        tauri::generate_handler![
            vault_status,
            vault_prepare_open,
            vault_unlock,
            vault_create,
            vault_lock,
            vault_activity,
            vault_warnings,
            vault_dismiss_warnings,
            vault_upgrade,
            vault_reload,
            keys_list,
            key_reveal,
            key_copy,
            key_add,
            key_update,
            key_remove,
            key_rotate,
            key_history,
            recycled_list,
            key_restore,
            key_purge,
            audit_read,
            audit_recover,
            env_export,
            env_copy,
            env_save,
            env_launch_terminal,
            env_import,
            view_discard,
            backup_info,
            reminders_status,
            reminders_set,
            reminders_notify
        ]
    };
}
pub(crate) use production_handlers;

#[tauri::command]
pub async fn vault_status(
    state: State<'_, VaultState>,
    preferences: State<'_, PreferenceState>,
) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let preferences = preferences.0.clone();
    let expected = session.snapshot();
    blocking(move || match expected {
        Ok(expected) => {
            let status =
                session.access(Access::Read, Some(expected), |vault| Ok(status_for(vault)))?;
            finish_status(&session, Access::Read, status)
        }
        Err(error) if error.kind == "locked" => {
            let path = current_path(preferences.as_ref(), &session)?;
            Ok(Status {
                session_id: None,
                path: path.display().to_string(),
                exists: path.exists(),
                unlocked: false,
                key_count: 0,
                vault_name: None,
                needs_upgrade: false,
            })
        }
        Err(error) => Err(error),
    })
    .await
}

#[tauri::command]
pub fn vault_prepare_open(state: State<'_, VaultState>) -> Result<String, CommandError> {
    state.0.prepare_open()
}

fn finish_status(
    session: &Session,
    access: Access,
    mut status: Stamped<Status>,
) -> Result<Status, CommandError> {
    status.value.session_id = Some(
        session
            .active_owner(status.snapshot.ticket)
            .map_err(|_| access.cancelled_after_completion())?,
    );
    session.finish(access, status)
}

#[tauri::command]
pub async fn vault_unlock(
    state: State<'_, VaultState>,
    preferences: State<'_, PreferenceState>,
    path: String,
    password: String,
    session_id: String,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    open_vault(
        state.0.clone(),
        preferences.0.clone(),
        path.into(),
        password,
        None,
        false,
        &session_id,
    )
    .await
}

#[tauri::command]
pub async fn vault_create(
    state: State<'_, VaultState>,
    preferences: State<'_, PreferenceState>,
    path: String,
    password: String,
    name: Option<String>,
    session_id: String,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    open_vault(
        state.0.clone(),
        preferences.0.clone(),
        path.into(),
        password,
        clean(name),
        true,
        &session_id,
    )
    .await
}

async fn open_vault(
    session: Arc<Session>,
    preferences: Arc<dyn PreferenceStore>,
    target: PathBuf,
    password: Zeroizing<String>,
    name: Option<String>,
    create: bool,
    session_id: &str,
) -> Result<Status, CommandError> {
    let ticket = session.claim_open(session_id)?;
    let completion = if create { Access::Write } else { Access::Read };
    let worker = session.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let load = || load_vault(&target, &password, name.as_deref(), create);
        if create {
            worker.open_created(ticket, load)?;
        } else {
            worker.open(ticket, load)?;
        }
        worker.check_completed(completion, ticket)?;
        worker
            .access(Access::Read, None, |vault| Ok(status_for(vault)))
            .map_err(|error| {
                if create {
                    completion.cancelled_after_completion()
                } else {
                    error
                }
            })
    })
    .await
    .map_err(CommandError::internal)??;
    if opened.snapshot.ticket != ticket {
        return Err(completion.cancelled_after_completion());
    }
    session.check_completed(completion, ticket)?;
    if preferences
        .save_vault_path(
            ticket,
            &|effect| session.commit(Binding::Ticket(ticket), effect),
            &opened.value.path,
        )
        .is_err()
    {
        session.warn_for(ticket, OperationWarningKind::Preference);
    }
    finish_status(&session, completion, opened)
}

fn load_vault(
    target: &Path,
    password: &str,
    name: Option<&str>,
    create: bool,
) -> Result<Vault, CommandError> {
    // Validation is part of the claimed opening, not an early return that leaves
    // its single-use reservation live after a terminal credential failure.
    if create && password.chars().count() < 12 {
        return Err(CommandError::new(
            "invalid",
            "Use at least 12 characters for the master password.",
        ));
    }
    if !create && password.is_empty() {
        return Err(CommandError::new("invalid", "Enter the master password."));
    }
    if create {
        Vault::create(target, password, name).map_err(Into::into)
    } else {
        Vault::open(target, password).map_err(Into::into)
    }
}

#[tauri::command]
pub fn vault_lock(
    state: State<'_, VaultState>,
    session_id: Option<String>,
) -> Result<(), CommandError> {
    state.0.lock_owned(session_id.as_deref())
}

#[tauri::command]
pub fn vault_activity(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<(), CommandError> {
    state.0.activity(&session_id)
}

#[tauri::command]
pub fn vault_warnings(state: State<'_, VaultState>) -> Result<WarningReport, CommandError> {
    state.0.warnings()
}

#[tauri::command]
pub fn vault_dismiss_warnings(
    state: State<'_, VaultState>,
    ids: Vec<String>,
) -> Result<(), CommandError> {
    state.0.dismiss_warnings(&ids)
}

#[tauri::command]
pub async fn vault_upgrade(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    let status = run_session(session.clone(), expected, Access::Upgrade, |vault| {
        vault.upgrade()?;
        Ok(status_for(vault))
    })
    .await?;
    finish_status(
        &session,
        Access::Upgrade,
        Stamped {
            snapshot: expected,
            value: status,
        },
    )
}

#[tauri::command]
pub async fn vault_reload(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    let status = run_session(session.clone(), expected, Access::Reload, |vault| {
        vault.reload()?;
        Ok(status_for(vault))
    })
    .await?;
    finish_status(
        &session,
        Access::Reload,
        Stamped {
            snapshot: expected,
            value: status,
        },
    )
}

async fn run_session<T: Send + 'static>(
    session: Arc<Session>,
    expected: Snapshot,
    access: Access,
    operation: impl FnOnce(&mut Vault) -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    let worker = session.clone();
    let completed = tauri::async_runtime::spawn_blocking(move || {
        worker.access(access, Some(expected), operation)
    })
    .await
    .map_err(CommandError::internal)??;
    session.finish(access, completed)
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(CommandError::internal)?
}

#[tauri::command]
pub async fn keys_list(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<Vec<KeyView>, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Read, |vault| {
        Ok(vault.list().iter().map(KeyView::from).collect())
    })
    .await
}

/// Shows one key's value to the page. `None` means that the user declined in the native window
/// that a request over the budget needs (`session::FREE_SINGLE_DISCLOSURES`).
#[tauri::command]
pub async fn key_reveal(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
    id: String,
) -> Result<Option<String>, CommandError> {
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || {
        single_disclosure(
            &session,
            confirm.as_ref(),
            &session_id,
            &id,
            KeyDisclosure::Reveal,
        )?
        .map(|secret| session.reveal(secret))
        .transpose()
    })
    .await
}

/// One key's secret, if the budget allows it or the user approves it. The budget counts the
/// request before anything is read. Over it, a native window names the key and nothing else,
/// no gate is held while it is open, and the read that follows is bound to the session, the
/// revision and the presentation of the request, so a lock, a change or a hide during the
/// window refuses it.
fn single_disclosure(
    session: &Session,
    host: &dyn ImportConfirm,
    session_id: &str,
    id: &str,
    action: KeyDisclosure,
) -> Result<Option<Stamped<Zeroizing<String>>>, CommandError> {
    let expected = session.snapshot_for(session_id)?;
    if session.admit_single_disclosure(expected.ticket)? == Admission::NeedsApproval {
        let title = session
            .access_reviewed(Access::Read, expected, |vault| {
                Ok(vault.get_by_id(id)?.title)
            })?
            .value;
        let review = DisclosureReview::single(action, &title)?;
        match disclosure::ask(session, host, expected.ticket, &review)? {
            Approval::Declined => return Ok(None),
            Approval::Approved => session.count_approved_disclosure(expected.ticket)?,
        }
    }
    requested_key_at(session, expected, id, action).map(Some)
}

#[cfg(test)]
fn requested_key(
    session: &Session,
    id: &str,
    action: KeyDisclosure,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    requested_key_at(session, session.snapshot()?, id, action)
}

fn requested_key_at(
    session: &Session,
    expected: Snapshot,
    id: &str,
    action: KeyDisclosure,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    session.access(Access::Read, Some(expected), |vault| {
        let key = vault.get_by_id(id)?;
        let proof = vault.disclose_key(action, &key, None)?;
        Ok(vault.secret(id, &proof)?)
    })
}

/// Copies one key's value. `None` means that the user declined (see `key_reveal`). Nothing
/// reaches the clipboard then.
#[tauri::command]
pub async fn key_copy(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
    id: String,
) -> Result<Option<u64>, CommandError> {
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || {
        let Some(secret) = single_disclosure(
            &session,
            confirm.as_ref(),
            &session_id,
            &id,
            KeyDisclosure::Copy,
        )?
        else {
            return Ok(None);
        };
        session.copy_stamped(secret)?;
        Ok(Some(CLEAR_AFTER.as_secs()))
    })
    .await
}

#[tauri::command]
pub async fn key_add(
    state: State<'_, VaultState>,
    session_id: String,
    draft: KeyDraft,
) -> Result<KeyView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        let key = vault.add(draft.into_input(None)?)?;
        Ok(KeyView::from(&key))
    })
    .await
}

#[tauri::command]
pub async fn key_update(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
    draft: KeyDraft,
) -> Result<KeyView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        let existing = vault.edit_input(&id)?;
        let key = vault.update(&id, draft.into_input(Some(existing))?)?;
        Ok(KeyView::from(&key))
    })
    .await
}

#[tauri::command]
pub async fn key_remove(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
) -> Result<(), CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        vault.remove(&id)?;
        Ok(())
    })
    .await
}

/// Replaces the secret of a key. The reply carries times and the key's metadata, never a secret.
/// A repeat with the value already stored is refused by the core, so a lost reply cannot rotate twice.
#[tauri::command]
pub async fn key_rotate(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
    secret: String,
) -> Result<RotationView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        rotate_key(vault, &id, secret)
    })
    .await
}

fn rotate_key(vault: &mut Vault, id: &str, secret: String) -> Result<RotationView, CommandError> {
    let rotation = vault.rotate(id, Secret::from(secret))?;
    Ok(RotationView::from(&rotation))
}

/// One earlier version of a key. It holds a time and the names of the fields that changed,
/// never a value.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    index: usize,
    modified_at: Option<DateTime<Utc>>,
    changed: Vec<String>,
    secret_changed: bool,
}

/// The fields that AKM and KDBX name themselves. A custom field in a foreign vault can have
/// any name, even a secret pasted there, so the page gets only that some other field changed.
const KNOWN_HISTORY_FIELDS: [&str; 15] = [
    "Title",
    "UserName",
    "URL",
    "Notes",
    "Password",
    "Tags",
    "Expiry",
    akm_core::model::META_ID,
    akm_core::model::META_ISSUER,
    akm_core::model::META_PROJECT,
    akm_core::model::META_ENVIRONMENT,
    akm_core::model::META_SCOPES,
    akm_core::model::META_ENV_VAR,
    akm_core::model::META_ROTATION_DAYS,
    akm_core::model::META_SECRET_CHANGED,
];
const OTHER_HISTORY_FIELD: &str = "Other";

fn shown_history_fields(changed: &[String]) -> Vec<String> {
    let mut shown: Vec<String> = changed
        .iter()
        .filter(|name| KNOWN_HISTORY_FIELDS.contains(&name.as_str()))
        .cloned()
        .collect();
    if shown.len() < changed.len() {
        shown.push(OTHER_HISTORY_FIELD.to_string());
    }
    shown
}

impl From<&HistoryVersion> for HistoryView {
    fn from(version: &HistoryVersion) -> Self {
        Self {
            index: version.index,
            modified_at: version.modified_at,
            changed: shown_history_fields(&version.changed),
            secret_changed: version.secret_changed,
        }
    }
}

fn history_views(vault: &Vault, id: &str) -> Result<Vec<HistoryView>, CommandError> {
    Ok(vault.history(id)?.iter().map(HistoryView::from).collect())
}

/// The earlier versions of one key, newest first. A read: the page may show it, and it has
/// no secret in it.
#[tauri::command]
pub async fn key_history(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
) -> Result<Vec<HistoryView>, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Read, move |vault| {
        history_views(vault, &id)
    })
    .await
}

#[tauri::command]
pub async fn recycled_list(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<Vec<RecycledView>, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Read, |vault| {
        Ok(vault.recycled().iter().map(RecycledView::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn key_restore(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
) -> Result<KeyView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        let key = vault.restore(&id)?;
        Ok(KeyView::from(&key))
    })
    .await
}

/// Deletes a key for good. It accepts only a key that is in the Recycle Bin, so a live key
/// can never be purged from here.
#[tauri::command]
pub async fn key_purge(
    state: State<'_, VaultState>,
    session_id: String,
    id: String,
) -> Result<(), CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Write, move |vault| {
        purge_recycled(vault, &id)
    })
    .await
}

fn purge_recycled(vault: &mut Vault, id: &str) -> Result<(), CommandError> {
    let recycled = vault.find_recycled(id)?;
    vault.purge(&recycled.key.id)?;
    Ok(())
}

/// What a recovery kept, for the page. The folder is the one that the page already shows as the
/// journal path, so only the file name goes with it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalRecoveryView {
    kept_name: String,
    sha256: String,
    bytes: u64,
}

/// Starts a new journal after the old one was found damaged (finding FR-14). `None` means that
/// the user declined in the native window. The page only asks: it cannot approve for the user.
#[tauri::command]
pub async fn audit_recover(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
) -> Result<Option<JournalRecoveryView>, CommandError> {
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || run_recovery(&session, confirm.as_ref(), &session_id)).await
}

/// The recovery, in the order that keeps the old journal usable as evidence after every
/// failure: a journal that is not damaged is refused before any window; the user approves in a
/// native window that holds no gate; the recovery runs bound to the session, the revision and
/// the presentation of the request, so a lock, a change or a hide during the window refuses it
/// with nothing moved. The core call writes the new journal first, keeps the old bytes under a
/// dated name and puts the old journal back if a step fails (`AuditLog::recover_damaged`).
fn run_recovery(
    session: &Session,
    host: &dyn ImportConfirm,
    session_id: &str,
) -> Result<Option<JournalRecoveryView>, CommandError> {
    let expected = session.snapshot_for(session_id)?;
    let file = session
        .access_reviewed(Access::Read, expected, |vault| {
            let log = vault.audit_log();
            if log.snapshot(0)?.state != akm_core::audit::AuditState::Corrupt {
                return Err(CommandError::new(
                    "journal",
                    "The journal is not damaged, so nothing was changed.",
                ));
            }
            let path = log.journal_path()?.ok_or_else(|| {
                CommandError::new("journal", "There is no journal to start over.")
            })?;
            Ok(path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default())
        })?
        .value;
    let review = DisclosureReview::recovery(&file)?;
    if disclosure::ask(session, host, expected.ticket, &review)? == Approval::Declined {
        return Ok(None);
    }
    // The recovery moves the journal outside the session gate (limit 4 of
    // `docs/design/commit-point.md`). Once it moved, the answer must not say that nothing moved:
    // an access that then refuses means that the session ended during the move.
    let mut recovered = None;
    let finished = session.access_reviewed(Access::Read, expected, |vault| {
        recovered = Some(vault.audit_log().recover_damaged()?);
        host.journal_moved();
        Ok(())
    });
    let Some(recovery) = recovered else {
        return Err(finished
            .err()
            .unwrap_or_else(|| CommandError::internal("The recovery gave no result.")));
    };
    if finished.is_err() {
        return Err(CommandError::completed_while_locked());
    }
    Ok(Some(JournalRecoveryView {
        kept_name: recovery
            .kept
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        sha256: recovery.sha256,
        bytes: recovery.bytes,
    }))
}

#[tauri::command]
pub async fn audit_read(
    state: State<'_, VaultState>,
    session_id: String,
    limit: Option<usize>,
) -> Result<AuditReport, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Read, move |vault| {
        let snapshot = vault.audit_log().snapshot(limit.unwrap_or(500))?;
        // The journal holds key IDs only. Titles, projects and environments are read from
        // the unlocked vault now, so a renamed key shows its current name (unit 1b, D3).
        let keys: HashMap<String, ApiKey> = vault
            .list()
            .into_iter()
            .map(|key| (key.id.clone(), key))
            .collect();
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| {
                let key = entry.key_id.as_ref().and_then(|id| keys.get(id));
                AuditView {
                    seq: entry.seq,
                    ts: entry.ts,
                    action: entry.action,
                    key_title: key.map(|key| key.title.clone()),
                    project: key.and_then(|key| key.project.clone()),
                    environment: key.and_then(|key| key.environment.clone()),
                    key_id: entry.key_id,
                    detail: entry.detail,
                }
            })
            .collect();
        Ok(AuditReport {
            path: snapshot.location.display().to_string(),
            entries,
            broken_at: snapshot.broken_at,
            state: snapshot.state,
            damage: snapshot.damage,
            identity: snapshot.identity,
            notices: snapshot.notices,
            problem_line: snapshot.problem_line,
            total_entries: snapshot.total_entries,
        })
    })
    .await
}

struct ExportScope {
    project: Option<String>,
    environment: Option<String>,
    all: bool,
}
impl ExportScope {
    fn new(
        project: Option<String>,
        environment: Option<String>,
        all: bool,
    ) -> Result<Self, CommandError> {
        let project = clean(project);
        let environment = clean(environment);
        if (all && (project.is_some() || environment.is_some()))
            || (!all && (project.is_none() || environment.is_none()))
        {
            return Err(CommandError::new(
                "invalid",
                "Choose both project and environment, or explicitly export all without filters.",
            ));
        }
        Ok(Self {
            project,
            environment,
            all,
        })
    }
}

struct SecretEntries(Vec<(String, String)>);
impl Drop for SecretEntries {
    fn drop(&mut self) {
        for (_, value) in &mut self.0 {
            value.zeroize();
        }
    }
}

fn export_body(
    vault: &Vault,
    scope: &ExportScope,
    kind: EnvDisclosure,
    detail: impl FnOnce(usize) -> String,
    refuse_empty: Option<&str>,
) -> Result<(Zeroizing<String>, usize), CommandError> {
    let plan = vault.plan_env(
        scope.project.as_deref(),
        scope.environment.as_deref(),
        scope.all,
    )?;
    if let (true, Some(message)) = (plan.is_empty(), refuse_empty) {
        return Err(CommandError::new("invalid", message));
    }
    let proof = vault.disclose_env(kind, &plan, Some(&detail(plan.len())))?;
    let vars = SecretEntries(vault.env_vars(&plan, &proof)?);
    let mut body = Zeroizing::new(String::new());
    for (name, value) in &vars.0 {
        let line = Zeroizing::new(dotenv::line(name, value));
        body.push_str(&line);
        body.push('\n');
    }
    Ok((body, vars.0.len()))
}

#[cfg(test)]
fn extract_export(
    session: &Session,
    scope: &ExportScope,
    target: &str,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    extract_export_at(session, session.snapshot()?, scope, target)
}

fn extract_export_at(
    session: &Session,
    expected: Snapshot,
    scope: &ExportScope,
    target: &str,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    session.access(Access::Read, Some(expected), |vault| {
        let (body, _) = export_body(
            vault,
            scope,
            EnvDisclosure::Export,
            |count| format!("{count} keys to {target}"),
            None,
        )?;
        Ok(body)
    })
}

/// The values of a scope, for the preview. Every request needs the user's click in a native
/// window that lists the scope and the names: a page can read its session ID, so the ID is no
/// gesture (FR-03). `None` means that the user declined, and no value left the vault.
#[tauri::command]
pub async fn env_export(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<Option<String>, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || {
        bulk_disclosure(
            &session,
            confirm.as_ref(),
            &session_id,
            &scope,
            BulkUse::Preview,
            |expected| {
                let exported = extract_export_at(&session, expected, &scope, ".env preview")?;
                session.reveal(exported)
            },
        )
    })
    .await
}

/// The values of a scope, for the clipboard. Approved like `env_export`. `None` means that the
/// user declined, and nothing reached the clipboard.
#[tauri::command]
pub async fn env_copy(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<Option<u64>, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || {
        bulk_disclosure(
            &session,
            confirm.as_ref(),
            &session_id,
            &scope,
            BulkUse::Clipboard,
            |expected| {
                let exported = extract_export_at(&session, expected, &scope, "clipboard")?;
                if exported.value.is_empty() {
                    return Err(CommandError::new(
                        "invalid",
                        "No keys match that export scope.",
                    ));
                }
                session.copy_stamped(exported)?;
                Ok(CLEAR_AFTER.as_secs())
            },
        )
    })
    .await
}

/// A bulk disclosure after the user approved it. The names come from a plan that holds no value.
/// An empty scope shows no window, because no value can leave. `disclose` reads the vault bound
/// to the session, the revision and the presentation of the request, so the plan that the user
/// approved is the plan that is read: a change in between refuses it.
fn bulk_disclosure<T>(
    session: &Session,
    host: &dyn ImportConfirm,
    session_id: &str,
    scope: &ExportScope,
    purpose: BulkUse,
    disclose: impl FnOnce(Snapshot) -> Result<T, CommandError>,
) -> Result<Option<T>, CommandError> {
    let expected = session.snapshot_for(session_id)?;
    let names = session
        .access_reviewed(Access::Read, expected, |vault| {
            let plan = vault.plan_env(
                scope.project.as_deref(),
                scope.environment.as_deref(),
                scope.all,
            )?;
            Ok(plan.names().map(str::to_owned).collect::<Vec<_>>())
        })?
        .value;
    if !names.is_empty() {
        let review = DisclosureReview::bulk(
            purpose,
            scope.project.as_deref().zip(scope.environment.as_deref()),
            names,
        )?;
        if disclosure::ask(session, host, expected.ticket, &review)? == Approval::Declined {
            return Ok(None);
        }
    }
    disclose(expected).map(Some)
}

#[tauri::command]
pub async fn env_save<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, VaultState>,
    session_id: String,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<Option<String>, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        // FileSaveDialog keeps its default overwrite-approval prompt. No session mutex spans the dialog.
        let selected = app
            .dialog()
            .file()
            .set_title("Export plaintext environment file")
            .set_file_name(".env")
            .add_filter("Environment files", &["env"])
            .blocking_save_file();
        session.ensure(expected.ticket)?;
        let Some(selected) = selected else {
            return Ok(None);
        };
        let target = selected
            .into_path()
            .map_err(|error| CommandError::new("invalidPath", error.to_string()))?;
        let FileExport {
            body,
            count,
            vault_path,
            journal,
        } = requested_file_export_at(&session, expected, &scope)?;
        let settings = settings_file(&app)?;
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(CommandError::internal)?;
        let remnants = private_export::Remnants::default();
        let saved = session.publish(
            expected,
            || {
                private_export::prepare_private(
                    &target,
                    &body,
                    &[vault_path, settings],
                    &[config_dir],
                    &remnants,
                )
            },
            |prepared| {
                publish_and_record(&session, expected, prepared, |_| {
                    journal.append(
                        "export-completed",
                        None,
                        Some(&format!("file export completed; {count} keys")),
                    )
                })
            },
        );
        let saved = report_remnants(&session, &remnants, saved)?;
        Ok(Some(saved.display().to_string()))
    })
    .await
    .map_err(CommandError::internal)?
}

struct FileExport {
    body: Zeroizing<String>,
    count: usize,
    vault_path: PathBuf,
    journal: AuditLog,
}

fn requested_file_export_at(
    session: &Session,
    expected: Snapshot,
    scope: &ExportScope,
) -> Result<FileExport, CommandError> {
    session
        .read_for_publication(expected, |vault| {
            let (body, count) = export_body(
                vault,
                scope,
                EnvDisclosure::Export,
                |count| format!("file export requested; {count} keys"),
                Some("No keys match that export scope."),
            )?;
            Ok(FileExport {
                body,
                count,
                vault_path: vault.path().to_path_buf(),
                journal: vault.audit_log(),
            })
        })
        .map(|exported| exported.value)
}

/// A temporary file whose deletion could not be confirmed may still hold the plaintext keys, so
/// the user must hear of it however the export ended. The text goes to the error of a failed
/// export, and to the application warnings too: a lock can overtake the reply, and a warning
/// outlives the session.
fn report_remnants<T>(
    session: &Session,
    remnants: &private_export::Remnants,
    result: Result<T, CommandError>,
) -> Result<T, CommandError> {
    let Some(warning) = remnants.warning() else {
        return result;
    };
    session.warn(warning.clone());
    result.map_err(|error| CommandError {
        message: format!("{} {warning}", error.message),
        ..error
    })
}

fn publish_and_record(
    session: &Session,
    expected: Snapshot,
    prepared: private_export::PreparedExport,
    record: impl FnOnce(&Path) -> akm_core::Result<()>,
) -> Result<PathBuf, CommandError> {
    let saved = prepared.publish()?;
    if record(&saved).is_err() {
        session.warn_for(expected.ticket, OperationWarningKind::ExportJournal);
    }
    Ok(saved)
}

/// Opens a terminal that holds one project's keys. The page names the session, the scope and
/// an operation ID that it made. It cannot name a program, a folder or an argument: native
/// code shows the folder picker and the review, and starts the system command prompt. `None`
/// means that the user cancelled in one of those windows. The number is how many keys the
/// terminal holds. The outcome record under the operation ID tells a page that lost this
/// reply what happened (`outcome.rs`).
#[tauri::command]
pub async fn env_launch_terminal(
    state: State<'_, VaultState>,
    terminal: State<'_, TerminalState>,
    session_id: String,
    operation_id: String,
    project: Option<String>,
    environment: Option<String>,
) -> Result<Option<usize>, CommandError> {
    let session = state.0.clone();
    let host = terminal.0.clone();
    blocking(move || {
        launch::run_launch(
            &session,
            host.as_ref(),
            &session_id,
            &operation_id,
            project,
            environment,
        )
    })
    .await
}

fn ensure_window_shown(confirm: &dyn ImportConfirm) -> Result<(), CommandError> {
    if confirm.window_live() {
        Ok(())
    } else {
        Err(CommandError::new(
            "viewChanged",
            "The AKM window is not shown, so no confirmation window was opened. Nothing was imported. Start it again.",
        ))
    }
}

/// The whole import in one command. The webview names the session and the data. A
/// native window shows the names and the scope, and only the user's click there lets
/// the import run: that dialog is the review. No review state is kept between the request
/// and the answer, so there is nothing for the webview to cancel, edit or replay. `None`
/// means that the user declined.
///
/// After the dialog returns, the approval is refused only by a lock, another session, or a
/// change of the vault since the preview was read. A hide, a page load, a discard from the
/// page or a hidden window do not refuse it: the user answered in the dialog, and the page
/// learns the result from the reply or, when it discarded its form, from the outcome record.
#[tauri::command]
pub async fn env_import(
    state: State<'_, VaultState>,
    confirm: State<'_, ConfirmState>,
    session_id: String,
    operation_id: String,
    text: String,
    project: Option<String>,
    environment: Option<String>,
) -> Result<Option<ImportReport>, CommandError> {
    let text = Zeroizing::new(text);
    let session = state.0.clone();
    let confirm = confirm.0.clone();
    blocking(move || {
        run_import(
            &session,
            confirm.as_ref(),
            &session_id,
            &operation_id,
            text,
            project,
            environment,
        )
    })
    .await
}

fn run_import(
    session: &Session,
    confirm: &dyn ImportConfirm,
    session_id: &str,
    operation_id: &str,
    text: Zeroizing<String>,
    project: Option<String>,
    environment: Option<String>,
) -> Result<Option<ImportReport>, CommandError> {
    // Every way out after the session check leaves one outcome record (contract 7). The page can
    // lose this reply, and the record is then the only truth about the request.
    outcome::dispatched(
        session,
        Operation::Import,
        operation_id,
        session_id,
        |_, expected| {
            let (project, environment) = (clean(project), clean(environment));
            let entries = SecretEntries(dotenv::parse(&text)?);
            let reviewed = session.access_reviewed(Access::Read, expected, |vault| {
                vault
                    .preview_import_env(&entries.0, project.as_deref(), environment.as_deref())
                    .map_err(Into::into)
            })?;
            let review = ImportReview::new(
                project.clone().unwrap_or_default(),
                environment.clone().unwrap_or_default(),
                reviewed.value,
            )?;
            if review.changes_anything() {
                // No dialog for a window nobody sees, or for a vault that is no longer what was
                // read. A modal dialog cannot be shown under the session gate, so the write below
                // checks the session and the revision again, under the gate.
                ensure_window_shown(confirm)?;
                if !session.snapshot()?.same_vault(&reviewed.snapshot) {
                    return Err(CommandError::new(
                        "stalePreview",
                        "The vault changed before the confirmation could be shown. Start it again.",
                    ));
                }
                let dialog = confirm.review_dialog();
                session.ensure(reviewed.snapshot.ticket)?;
                match review.ask(dialog.as_ref()) {
                    ReviewAnswer::Approved => {}
                    ReviewAnswer::Declined => return Ok(None),
                    ReviewAnswer::Failed(failure) => return Err(failure.into()),
                }
            }
            // The write is bound to the session and the revision of the request: a lock, another
            // session or another write since then makes it stale. The presentation is not part of
            // the binding. The plan must also still equal what the user was shown.
            session
                .access_reviewed(Access::Write, reviewed.snapshot, |vault| {
                    let current = vault.preview_import_env(
                        &entries.0,
                        project.as_deref(),
                        environment.as_deref(),
                    )?;
                    if &current != review.preview() {
                        return Err(CommandError::new(
                            "stalePreview",
                            "The vault changed while the import waited for confirmation. Start it again.",
                        ));
                    }
                    let (added, updated) =
                        vault.import_env(&entries.0, project.as_deref(), environment.as_deref())?;
                    Ok(ImportReport { added, updated })
                })
                .and_then(|stamped| session.finish(Access::Write, stamped))
                .map(Some)
        },
    )
}

/// The page tells native that it discarded its protected views (the document was hidden), and
/// names the session it was in when it handled the hide. Native then does what a minimize does
/// to that session: it expires the presentation, so a reveal, a preview or an export result of
/// the discarded view can no longer apply. The request can arrive late. A session that is not
/// the current one has nothing to expire, so the answer is success and nothing else happens.
/// The discard does not decide an import: the native dialog does.
#[tauri::command]
pub fn view_discard<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<(), CommandError> {
    discard_view(&state.0, &session_id, |notice| {
        tauri::Emitter::emit(&app, "vault-view-expired", notice).map_err(|_| ())
    })
}

fn discard_view(
    session: &Session,
    session_id: &str,
    emit: impl FnOnce(&crate::session::NativeViewNotice) -> Result<(), ()>,
) -> Result<(), CommandError> {
    crate::visibility::expire_and_notify_for(session, session_id, emit).map(|_| ())
}

#[tauri::command]
pub fn reminders_status(state: State<'_, ReminderState>) -> Result<ReminderStatus, CommandError> {
    state.0.status()
}

#[tauri::command]
pub fn reminders_set(
    state: State<'_, ReminderState>,
    enabled: bool,
) -> Result<ReminderStatus, CommandError> {
    state.0.set_enabled(enabled)
}

/// The page sends a count and the session it counted in, and nothing else. The text of the
/// notification is built natively, and only while that session is still the current one.
#[tauri::command]
pub fn reminders_notify(
    vault: State<'_, VaultState>,
    state: State<'_, ReminderState>,
    session_id: String,
    count: u32,
) -> Result<ReminderStatus, CommandError> {
    state.0.notify(&vault.0, &session_id, count)
}

#[tauri::command]
pub async fn backup_info(
    state: State<'_, VaultState>,
    session_id: String,
) -> Result<BackupInfo, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot_for(&session_id)?;
    run_session(session, expected, Access::Read, |vault| {
        let mut backup = vault.path().as_os_str().to_os_string();
        backup.push(".bak");
        let path = PathBuf::from(backup);
        let metadata = fs::metadata(&path).ok();
        Ok(BackupInfo {
            path: path.display().to_string(),
            exists: metadata.is_some(),
            bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use std::sync::Mutex;

    /// The commands that do not name a session, each with its reason. Every other command takes
    /// `session_id`, so a request cannot act on a session other than the one it was made in
    /// (fault class B of AGENTS.md). A new command lands in one list or the other, or this
    /// test fails.
    const NO_SESSION: &[(&str, &str)] = &[
        ("vault_status", "asks which session is current"),
        ("vault_prepare_open", "starts a session"),
        ("vault_warnings", "application-wide, carries no vault data"),
        (
            "vault_dismiss_warnings",
            "application-wide, takes record ids",
        ),
        ("reminders_status", "application-wide preference"),
        ("reminders_set", "application-wide preference"),
    ];

    /// Commands that take `session_id` and that the page sets by hand, not through the
    /// `sessionScoped` list of `api.ts`.
    const SESSION_SET_BY_HAND: &[&str] = &[
        "vault_unlock",
        "vault_create",
        "vault_lock",
        "env_import",
        "view_discard",
        "reminders_notify",
    ];

    /// How many times the identifier `session_id` appears in the text, as a whole word.
    fn session_id_words(text: &str) -> usize {
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| *word == "session_id")
            .count()
    }

    /// Each command: its name, the number of times `session_id` appears in its header, and the
    /// number of times it appears in its body. A command that takes the id and never uses it
    /// has a body count of zero.
    fn command_headers() -> Vec<(String, usize, usize)> {
        let source = include_str!("commands.rs");
        let code = &source[..source.find("\nmod tests {").unwrap()];
        let mut found = Vec::new();
        for part in code.split("#[tauri::command]").skip(1) {
            let start = part.find(" fn ").expect("a command function") + 4;
            let name_end = part[start..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap();
            let name = part[start..start + name_end].to_string();
            let body_start = part.find('{').unwrap();
            found.push((
                name,
                session_id_words(&part[start..body_start]),
                session_id_words(&part[body_start..]),
            ));
        }
        found
    }

    #[test]
    fn p2r2_03_every_command_that_acts_on_the_vault_names_its_session() {
        let headers = command_headers();
        assert!(headers.len() >= 30, "the scan finds the commands");
        for (name, in_header, in_body) in &headers {
            let exempt = NO_SESSION.iter().any(|(exempt, _)| exempt == name);
            assert_eq!(
                *in_header > 0,
                !exempt,
                "{name}: takes session_id unless it is on NO_SESSION"
            );
            assert!(
                exempt || *in_body > 0,
                "{name}: takes session_id and uses it"
            );
        }
        for (exempt, _) in NO_SESSION {
            assert!(
                headers.iter().any(|(name, _, _)| name == exempt),
                "{exempt} is a command"
            );
        }

        // The page side: api.ts adds the session to exactly the commands that Rust expects it
        // for, apart from the ones the page sets by hand.
        let page = include_str!("../../src/api.ts");
        let list = &page[page.find("const sessionScoped = new Set([").unwrap()..];
        let list = &list[..list.find("]);").unwrap()];
        let mut scoped: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
        scoped.sort_unstable();
        let mut expected: Vec<&str> = headers
            .iter()
            .filter(|(name, in_header, _)| {
                *in_header > 0 && !SESSION_SET_BY_HAND.contains(&name.as_str())
            })
            .map(|(name, _, _)| name.as_str())
            .collect();
        expected.sort_unstable();
        assert_eq!(
            scoped, expected,
            "api.ts sessionScoped matches the commands"
        );
    }

    fn test_session() -> (Arc<Session>, tempfile::TempDir) {
        use crate::{
            clipboard::tests::{FakeClipboard, FakeData},
            policy::Signal,
            session::MonotonicClock,
        };
        use std::sync::Mutex;
        let directory = tempfile::tempdir().unwrap();
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::create(
                    directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                    None,
                )
                .map_err(Into::into)
            })
            .unwrap();
        (session, directory)
    }

    #[test]
    fn fr04_a_failed_export_with_a_remnant_names_it_in_the_error_and_in_the_warnings() {
        let (session, _directory) = test_session();
        let remnants = private_export::Remnants::default();
        let remnant = PathBuf::from(r"C:\fake-folder\.akm-export-00000000.tmp");
        remnants.record(remnant.clone());

        let error = report_remnants::<()>(
            &session,
            &remnants,
            Err(CommandError::new("stalePreview", "The vault changed.")),
        )
        .unwrap_err();

        assert_eq!(error.kind, "stalePreview");
        assert!(error.message.starts_with("The vault changed. "));
        let shown = remnant.display().to_string();
        assert!(error.message.contains(&shown), "{}", error.message);
        let warnings = serde_json::to_value(session.warnings().unwrap()).unwrap();
        let application = warnings["application"].as_array().unwrap();
        assert_eq!(application.len(), 1);
        assert!(application[0]["message"].as_str().unwrap().contains(&shown));
    }

    #[test]
    fn fr04_a_saved_export_with_a_remnant_still_warns() {
        let (session, _directory) = test_session();
        let remnants = private_export::Remnants::default();
        remnants.record(PathBuf::from(r"C:\fake-folder\.akm-export-11111111.tmp"));

        let saved = report_remnants(&session, &remnants, Ok(7)).unwrap();

        assert_eq!(saved, 7);
        let warnings = serde_json::to_value(session.warnings().unwrap()).unwrap();
        assert_eq!(warnings["application"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn fr04_without_a_remnant_the_result_and_the_warnings_are_untouched() {
        let (session, _directory) = test_session();
        let remnants = private_export::Remnants::default();

        let error = report_remnants::<()>(
            &session,
            &remnants,
            Err(CommandError::new("locked", "The session is locked.")),
        )
        .unwrap_err();

        assert_eq!(
            (error.kind.as_str(), error.message.as_str()),
            ("locked", "The session is locked.")
        );
        let warnings = serde_json::to_value(session.warnings().unwrap()).unwrap();
        assert!(warnings["application"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_rotation_committed_while_the_session_locks_reports_completion_and_no_secret() {
        let (session, directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                Ok(vault.add(akm_core::ApiKeyInput::new("Rotated", "fake-old-secret"))?)
            })
            .unwrap()
            .value;
        let snapshot = session.snapshot().unwrap();
        let completed = session
            .access(Access::Write, Some(snapshot), |vault| {
                rotate_key(vault, &key.id, "fake-new-secret".into())
            })
            .unwrap();
        let receipt = serde_json::to_string(&completed.value).unwrap();
        assert!(!receipt.contains("fake-old-secret") && !receipt.contains("fake-new-secret"));
        assert!(receipt.contains("\"changedAt\"") && receipt.contains("\"historyCount\":1"));
        session.lock().unwrap();
        let error = session.finish(Access::Write, completed).err().unwrap();
        assert_eq!(error.kind, "completedWhileLocked");
        let shown = serde_json::to_string(&error).unwrap();
        assert!(!shown.contains("fake-old-secret") && !shown.contains("fake-new-secret"));
        // The rotation is saved once: a second open shows the new secret and one history version.
        let reopened = Vault::open(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert_eq!(
            crate::test_support::secret_of(&reopened, &key.id),
            "fake-new-secret"
        );
        assert_eq!(reopened.get_by_id(&key.id).unwrap().history_count, 1);
    }

    #[test]
    fn repeating_a_rotation_with_the_stored_value_is_refused_and_adds_no_history() {
        let (session, _directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                Ok(vault.add(akm_core::ApiKeyInput::new("Rotated", "fake-old-secret"))?)
            })
            .unwrap()
            .value;
        let rotate = |secret: &str| {
            session.access(Access::Write, None, |vault| {
                rotate_key(vault, &key.id, secret.into())
            })
        };
        rotate("fake-new-secret").unwrap();
        let error = rotate("fake-new-secret").err().unwrap();
        assert_eq!(error.kind, "invalid");
        assert!(!error.message.contains("fake-new-secret"));
        let history = session
            .access(Access::Read, None, |vault| {
                Ok(vault.get_by_id(&key.id)?.history_count)
            })
            .unwrap()
            .value;
        assert_eq!(history, 1);
    }

    #[test]
    fn the_purge_command_deletes_only_a_key_that_is_in_the_recycle_bin() {
        let (session, _directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                Ok(vault.add(akm_core::ApiKeyInput::new("Kept", "fake-kept-secret"))?)
            })
            .unwrap()
            .value;
        let purge =
            |id: &str| session.access(Access::Write, None, |vault| purge_recycled(vault, id));
        assert_eq!(purge(&key.id).err().unwrap().kind, "notFound");
        session
            .access(Access::Write, None, |vault| Ok(vault.remove(&key.id)?))
            .unwrap();
        purge(&key.id).unwrap();
        let (live, recycled) = session
            .access(Access::Read, None, |vault| {
                Ok((vault.list().len(), vault.recycled().len()))
            })
            .unwrap()
            .value;
        assert_eq!((live, recycled), (0, 0));
    }

    #[test]
    fn the_history_command_returns_times_and_changed_field_names_and_never_a_value() {
        let (session, _directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                Ok(vault.add(akm_core::ApiKeyInput::new("Kept", "fake-first-secret"))?)
            })
            .unwrap()
            .value;
        let history = |id: &str| {
            session
                .access(Access::Read, None, |vault| history_views(vault, id))
                .map(|stamped| stamped.value)
        };
        assert!(history(&key.id).unwrap().is_empty());
        session
            .access(Access::Write, None, |vault| {
                rotate_key(vault, &key.id, "fake-second-secret".into())
            })
            .unwrap();
        session
            .access(Access::Write, None, |vault| {
                rotate_key(vault, &key.id, "fake-third-secret".into())
            })
            .unwrap();
        let views = history(&key.id).unwrap();
        assert_eq!(views.len(), 2);
        let wire = serde_json::to_value(&views).unwrap();
        assert_eq!(wire[0]["index"], 0);
        assert!(wire[0]["modifiedAt"].is_string());
        assert_eq!(wire[0]["secretChanged"], true);
        assert!(wire[0]["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "Password"));
        assert!(wire[0].get("modified_at").is_none() && wire[0].get("secret_changed").is_none());
        for secret in [
            "fake-first-secret",
            "fake-second-secret",
            "fake-third-secret",
        ] {
            assert!(!wire.to_string().contains(secret), "{secret}");
        }
        assert_eq!(history("not-a-key").unwrap_err().kind, "notFound");
    }

    #[test]
    fn a_lock_just_before_the_import_review_shows_no_review() {
        let (session, _directory) = test_session();
        let locker = session.clone();
        let confirm = ScriptedConfirm::new(true).on_prepare(move || locker.lock().unwrap());
        let error = import(&session, &confirm, "FAKE_KEY=fake-value\n", "app", "dev")
            .err()
            .unwrap();
        assert_eq!(error.kind, "locked");
        assert!(confirm.shown().is_empty());
    }

    fn import_notice_for(session: &Session, operation: &str) -> Option<serde_json::Value> {
        notices(session)
            .into_iter()
            .find(|record| record["id"] == operation)
    }

    /// Runs one import whose reply is dropped, as a page that lost the reply would.
    fn import_with_lost_reply(
        session: &Session,
        confirm: &ScriptedConfirm,
    ) -> (String, Result<Option<ImportReport>, CommandError>) {
        let operation = new_operation();
        let reply = run_import(
            session,
            confirm,
            &owner(session),
            &operation,
            Zeroizing::new("TOKEN=fake-unapplied\n".into()),
            Some("app".into()),
            Some("dev".into()),
        );
        (operation, reply)
    }

    #[test]
    fn a_lock_before_the_import_review_leaves_a_truthful_outcome_even_if_the_reply_is_lost() {
        let (session, directory) = test_session();
        let locker = session.clone();
        let confirm = ScriptedConfirm::new(true).on_prepare(move || locker.lock().unwrap());
        let (operation, reply) = import_with_lost_reply(&session, &confirm);
        assert_eq!(reply.err().unwrap().kind, "locked");
        assert!(confirm.shown().is_empty());
        let record = import_notice_for(&session, &operation).expect("no native outcome record");
        assert_eq!(record["kind"], "importFailed");
        let reopened = Vault::open(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert!(crate::test_support::env_of(&reopened, "app", "dev").is_empty());
    }

    #[test]
    fn a_lock_at_the_window_check_of_an_import_leaves_a_truthful_outcome() {
        let (session, _directory) = test_session();
        let locker = session.clone();
        let confirm = ScriptedConfirm::new(true).on_probe(move || locker.lock().unwrap());
        let (operation, reply) = import_with_lost_reply(&session, &confirm);
        assert_eq!(reply.err().unwrap().kind, "locked");
        assert!(confirm.shown().is_empty());
        let record = import_notice_for(&session, &operation).expect("no native outcome record");
        assert_eq!(record["kind"], "importFailed");
    }

    #[test]
    fn a_lock_just_before_the_import_window_appears_applies_nothing_even_if_the_user_approves() {
        let (session, directory) = test_session();
        let locker = session.clone();
        let confirm = ScriptedConfirm::new(true).on_present(move || locker.lock().unwrap());
        let (operation, reply) = import_with_lost_reply(&session, &confirm);
        assert_eq!(reply.err().unwrap().kind, "locked");
        assert_eq!(confirm.shown().len(), 1, "the accepted limit of option (a)");
        let record = import_notice_for(&session, &operation).expect("no native outcome record");
        assert_eq!(record["kind"], "importFailed");
        let reopened = Vault::open(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert!(crate::test_support::env_of(&reopened, "app", "dev").is_empty());
    }

    #[test]
    fn a_failed_import_window_is_an_error_and_never_a_decline() {
        let (session, directory) = test_session();
        let confirm = ScriptedConfirm::new(true).failing(crate::review_window::ReviewFailure::new(
            "TaskDialogIndirect failed",
        ));
        let (operation, reply) = import_with_lost_reply(&session, &confirm);
        assert_eq!(reply.err().unwrap().kind, "reviewFailed");
        let record = import_notice_for(&session, &operation).expect("no native outcome record");
        assert_eq!(record["kind"], "importFailed");
        let reopened = Vault::open(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert!(crate::test_support::env_of(&reopened, "app", "dev").is_empty());
    }

    #[test]
    fn a_custom_field_name_in_the_history_never_reaches_the_page() {
        let version = HistoryVersion {
            index: 0,
            modified_at: None,
            changed: vec![
                "Title".into(),
                "fake-secret-in-a-field-name".into(),
                "another-fake-name".into(),
                "AKM_PROJECT".into(),
            ],
            secret_changed: false,
        };
        let wire = serde_json::to_value(HistoryView::from(&version)).unwrap();
        assert_eq!(
            wire["changed"],
            serde_json::json!(["Title", "AKM_PROJECT", "Other"])
        );
    }

    #[test]
    fn the_launch_command_takes_a_session_and_a_scope_and_nothing_that_names_a_program() {
        let source = include_str!("commands.rs");
        let code = &source[..source.find("\nmod tests {").unwrap()];
        let start = code.find("pub async fn env_launch_terminal(").unwrap();
        let header = &code[start..start + code[start..].find(") ->").unwrap()];
        let parameters: Vec<&str> = header
            .lines()
            .skip(1)
            .filter_map(|line| line.trim().split(':').next())
            .filter(|name| !name.is_empty())
            .collect();
        assert_eq!(
            parameters,
            [
                "state",
                "terminal",
                "session_id",
                "operation_id",
                "project",
                "environment"
            ]
        );
    }

    #[test]
    fn status_owner_stamp_is_bound_to_the_completed_native_ticket() {
        let (session, _directory) = test_session();
        let completed = session
            .access(Access::Read, None, |vault| Ok(status_for(vault)))
            .unwrap();
        let owner = session.active_owner(completed.snapshot.ticket).unwrap();
        let status = finish_status(&session, Access::Read, completed).unwrap();
        assert!(status.unlocked);
        assert_eq!(status.session_id.as_deref(), Some(owner.as_str()));
        let completed = session
            .access(Access::Read, None, |vault| Ok(status_for(vault)))
            .unwrap();
        session.lock().unwrap();
        let error = finish_status(&session, Access::Read, completed)
            .err()
            .unwrap();
        assert_eq!(error.kind, "locked");
        assert!(!error.message.contains("vault.kdbx"));
    }

    #[test]
    fn native_owner_invalid_credentials_retire_the_claimed_reservation() {
        for create in [false, true] {
            let (session, directory) = test_session();
            let owner = session.prepare_open().unwrap();
            let ticket = session.claim_open(&owner).unwrap();
            let target = directory.path().join("invalid-credentials.kdbx");
            let load = || load_vault(&target, "", None, create);
            let error = if create {
                session.open_created(ticket, load)
            } else {
                session.open(ticket, load)
            }
            .unwrap_err();
            assert_eq!(error.kind, "invalid");
            assert!(session.snapshot().is_err());
            assert!(session.claim_open(&owner).is_err());
            assert!(!target.exists());
            assert_ne!(session.prepare_open().unwrap(), owner);
        }
    }

    #[test]
    fn status_owner_stamp_survives_reload_and_upgrade_without_a_new_owner() {
        let (session, _directory) = test_session();
        let owner = session
            .active_owner(session.snapshot().unwrap().ticket)
            .unwrap();
        for access in [Access::Reload, Access::Upgrade] {
            let status = session
                .access(access, None, |vault| {
                    match access {
                        Access::Reload => vault.reload()?,
                        Access::Upgrade => vault.upgrade()?,
                        _ => unreachable!(),
                    }
                    Ok(status_for(vault))
                })
                .unwrap();
            assert_eq!(
                finish_status(&session, access, status)
                    .unwrap()
                    .session_id
                    .as_deref(),
                Some(owner.as_str())
            );
        }
    }

    #[test]
    fn backend_export_requires_complete_scope_or_explicit_all() {
        assert!(ExportScope::new(None, None, false).is_err());
        assert!(ExportScope::new(Some("app".into()), None, false).is_err());
        assert!(ExportScope::new(Some("app".into()), Some("dev".into()), true).is_err());
        assert!(ExportScope::new(Some("app".into()), Some("dev".into()), false).is_ok());
        assert!(ExportScope::new(None, None, true).is_ok());
    }

    #[test]
    fn native_window_configuration_excludes_screenshots() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["app"]["windows"][0]["contentProtected"], true);
    }

    #[test]
    fn a_logged_disclosure_request_is_not_a_completion_when_lock_or_copy_failure_intervenes() {
        let (session, directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                vault
                    .add(akm_core::ApiKeyInput::new("Fake", "fake-secret"))
                    .map_err(Into::into)
            })
            .unwrap()
            .value;
        let requested = requested_key(&session, &key.id, KeyDisclosure::Reveal).unwrap();
        session.lock().unwrap();
        assert_eq!(session.reveal(requested).unwrap_err().kind, "locked");
        let log = crate::test_support::journal_of(&directory.path().join("vault.kdbx"));
        assert_eq!(log.read(1).unwrap()[0].action, "reveal-requested");

        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::open(
                    directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
        let requested = requested_key(&session, &key.id, KeyDisclosure::Copy).unwrap();
        session.lock().unwrap();
        assert_eq!(
            session
                .copy(requested.snapshot.ticket, requested.value)
                .unwrap_err()
                .kind,
            "locked"
        );
        assert_eq!(log.read(1).unwrap()[0].action, "copy-requested");
        assert!(log
            .read(10)
            .unwrap()
            .iter()
            .all(|entry| !matches!(entry.action.as_str(), "reveal-completed" | "copy-completed")));
    }

    #[test]
    fn failed_clipboard_publication_keeps_only_a_requested_event_and_logging_failure_blocks_disclosure(
    ) {
        struct RefusingClipboard;
        impl crate::clipboard::ClipboardBackend for RefusingClipboard {
            fn set_private(&mut self, _text: &str) -> crate::clipboard::Publication {
                crate::clipboard::Publication::for_test(crate::clipboard::Outcome::Untouched(
                    CommandError::new("clipboard", "Fake clipboard refusal"),
                ))
            }
            fn clear_matching(
                &mut self,
                _sequence: u32,
                _text: &str,
            ) -> Result<bool, CommandError> {
                Ok(false)
            }
        }
        let (_fixture, directory) = test_session();
        let path = directory.path().join("vault.kdbx");
        let session = Session::new(
            Arc::new(crate::session::MonotonicClock::default()),
            Box::new(RefusingClipboard),
            Arc::new(|_| {}),
        );
        session.observe(crate::policy::Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::open(&path, "fake-test-password-123!").map_err(Into::into)
            })
            .unwrap();
        let key = session
            .access(Access::Write, None, |vault| {
                vault
                    .add(akm_core::ApiKeyInput::new("Fake", "fake-secret"))
                    .map_err(Into::into)
            })
            .unwrap()
            .value;
        let requested = requested_key(&session, &key.id, KeyDisclosure::Copy).unwrap();
        assert_eq!(
            session
                .copy(requested.snapshot.ticket, requested.value)
                .unwrap_err()
                .kind,
            "clipboard"
        );
        let log = crate::test_support::journal_of(&path);
        assert_eq!(log.read(1).unwrap()[0].action, "copy-requested");
        std::fs::remove_file(log.path()).unwrap();
        std::fs::create_dir(log.path()).unwrap();
        assert!(requested_key(&session, &key.id, KeyDisclosure::Reveal).is_err());
        assert!(requested_key(&session, &key.id, KeyDisclosure::Copy).is_err());
    }

    #[test]
    fn unsafe_journal_appends_block_native_disclosure_with_sanitized_errors() {
        let (session, directory) = test_session();
        let key = session
            .access(Access::Write, None, |vault| {
                vault
                    .add(akm_core::ApiKeyInput::new("Fake", "fake-secret"))
                    .map_err(Into::into)
            })
            .unwrap()
            .value;
        let log = crate::test_support::journal_of(&directory.path().join("vault.kdbx"));
        let mut no_newline = std::fs::read(log.path()).unwrap();
        assert_eq!(no_newline.pop(), Some(b'\n'));
        for bytes in [
            no_newline,
            b"{\"seq\":\"FAKE_JOURNAL_MARKER\"}\n".to_vec(),
            b"\x0c".to_vec(),
            b"\x0c\n".to_vec(),
            b" \t\r".to_vec(),
        ] {
            std::fs::write(log.path(), &bytes).unwrap();
            for action in [KeyDisclosure::Reveal, KeyDisclosure::Copy] {
                let error = requested_key(&session, &key.id, action)
                    .err()
                    .expect("unsafe append disclosed a native secret");
                assert!(!error.message.contains("FAKE_JOURNAL_MARKER"));
                assert!(!error.message.contains("fake-secret"));
                assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
            }
            let scope = ExportScope::new(None, None, true).unwrap();
            let error = extract_export(&session, &scope, "test")
                .err()
                .expect("unsafe append permitted export");
            assert!(!error.message.contains("FAKE_JOURNAL_MARKER"));
            assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
        }
    }

    #[test]
    fn native_presentation_file_save_keeps_ticket_revision_and_journal_authority_at_extraction() {
        for change in ["hide", "lock", "revision", "journal"] {
            let (session, directory) = test_session();
            session
                .access(Access::Write, None, |vault| {
                    for (environment, secret) in
                        [("dev", "fake-approved-dev"), ("prod", "fake-excluded-prod")]
                    {
                        let mut input =
                            akm_core::ApiKeyInput::new("Fake export extraction", secret);
                        input.project = Some("demo".into());
                        input.environment = Some(environment.into());
                        input.env_var = Some("TOKEN".into());
                        vault.add(input)?;
                    }
                    Ok(())
                })
                .unwrap();
            let expected = session.snapshot().unwrap();
            session.expire_presentation().unwrap();
            let journal = crate::test_support::journal_of(&directory.path().join("vault.kdbx"));
            match change {
                "lock" => session.lock().unwrap(),
                "revision" => {
                    // A write that replaces the file moves the revision (commit-point.md).
                    session
                        .access(Access::Write, None, |vault| {
                            vault
                                .add(akm_core::ApiKeyInput::new("Fake other", "fake-other"))
                                .map(drop)
                                .map_err(Into::into)
                        })
                        .unwrap();
                }
                "journal" => {
                    fs::remove_file(journal.path()).unwrap();
                    fs::create_dir(journal.path()).unwrap();
                }
                "hide" => {}
                _ => unreachable!(),
            }
            let scope = ExportScope::new(Some("demo".into()), Some("dev".into()), false).unwrap();
            let result = requested_file_export_at(&session, expected, &scope);
            if change == "hide" {
                let exported = result.unwrap();
                assert_eq!(exported.body.as_str(), "TOKEN=fake-approved-dev\n");
                assert_eq!(exported.count, 1);
                assert_eq!(exported.vault_path, directory.path().join("vault.kdbx"));
                assert_eq!(journal.read(1).unwrap()[0].action, "export-requested");
            } else {
                let error = result
                    .err()
                    .expect("native lock/revision/journal must still refuse file extraction");
                if change == "lock" {
                    assert_eq!(error.kind, "locked");
                }
                if change == "revision" {
                    assert_eq!(error.kind, "stalePreview");
                }
                assert!(!error.message.contains("fake-approved-dev"));
                assert!(!error.message.contains("fake-excluded-prod"));
            }
        }
    }

    #[test]
    fn complete_export_record_with_failed_sync_keeps_publication_and_late_lock_receipt() {
        use std::{io::Write, sync::mpsc, time::Duration};
        for late_lock in [false, true] {
            let (session, directory) = test_session();
            session
                .access(Access::Write, None, |vault| {
                    vault
                        .add(akm_core::ApiKeyInput::new(
                            "Private A",
                            "fake-export-secret",
                        ))
                        .map_err(Into::into)
                })
                .unwrap();
            let scope = ExportScope::new(None, None, true).unwrap();
            let exported = extract_export(&session, &scope, "file export").unwrap();
            let expected = exported.snapshot;
            let path = directory.path().join("vault.kdbx");
            let log = crate::test_support::journal_of(&path);
            let before = fs::read(log.path()).unwrap();
            // Make valid completion bytes with the real encoder, then inject failure immediately
            // after writing those bytes to the actual test journal, before syncing that handle.
            let oracle = akm_core::audit::ChainFile::at(
                directory.path().join("encoder-only.jsonl"),
                &log.id(),
            );
            fs::write(oracle.path(), &before).unwrap();
            oracle.append("export-completed", None, None).unwrap();
            let completion = fs::read(oracle.path()).unwrap()[before.len()..].to_vec();
            let target = directory.path().join("fake-export.env");
            let worker_target = target.clone();
            let (written_tx, written_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let worker_session = session.clone();
            let worker = std::thread::spawn(move || {
                worker_session.publish(
                    expected,
                    || {
                        private_export::prepare_private(
                            &worker_target,
                            &exported.value,
                            std::slice::from_ref(&path),
                            &[],
                            &private_export::Remnants::default(),
                        )
                    },
                    |prepared| {
                        publish_and_record(&worker_session, expected, prepared, |_| {
                            let log = crate::test_support::journal_of(&path);
                            let mut file = fs::OpenOptions::new()
                                .append(true)
                                .open(log.path())
                                .unwrap();
                            file.write_all(&completion).unwrap();
                            written_tx.send(()).unwrap();
                            release_rx.recv().unwrap();
                            Err(std::io::Error::other("injected before successful sync").into())
                        })
                    },
                )
            });
            written_rx.recv_timeout(Duration::from_secs(20)).unwrap();
            assert!(target.exists());
            if late_lock {
                session.lock().unwrap();
                let newer = session.begin_open().unwrap();
                session
                    .open(newer, || {
                        Vault::create(
                            directory.path().join("b.kdbx"),
                            "fake-test-password-123!",
                            None,
                        )
                        .map_err(Into::into)
                    })
                    .unwrap();
            }
            release_tx.send(()).unwrap();
            let outcome = worker.join().unwrap();
            if late_lock {
                assert_eq!(outcome.unwrap_err().kind, "completedWhileLocked");
            } else {
                assert_eq!(outcome.unwrap(), target);
            }
            assert!(fs::read_to_string(&target)
                .unwrap()
                .contains("fake-export-secret"));
            assert_eq!(log.read(1).unwrap()[0].action, "export-completed");
            let report = serde_json::to_value(session.warnings().unwrap()).unwrap();
            assert_eq!(report["operations"][0]["kind"], "exportJournal");
            assert_eq!(
                report["operations"][0]["originSession"],
                expected.ticket.0.to_string()
            );
            for private in ["Private A", "fake-export-secret", "fake-export.env"] {
                assert!(!report.to_string().contains(private));
            }
            if late_lock {
                assert_ne!(session.snapshot().unwrap().ticket, expected.ticket);
            }
        }
    }

    #[test]
    fn export_uses_core_validation_not_only_native_or_frontend_scope_checks() {
        use akm_core::model::{META_ENVIRONMENT, META_ENV_VAR, META_PROJECT};
        use std::sync::Mutex;
        // 3-SA-01: `add` now refuses PATH, so the key is an older entry written to the file.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        let mut db = keepass::Database::new();
        db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
        {
            let mut root = db.root_mut();
            let mut group = root.add_group();
            group.name = "API Keys".into();
            let mut entry = group.add_entry();
            use keepass::db::fields;
            entry.set_unprotected(fields::TITLE, "Process control");
            entry.set_protected(fields::PASSWORD, "fake");
            entry.set_unprotected(META_ENV_VAR, "PATH");
            entry.set_unprotected(META_PROJECT, "app");
            entry.set_unprotected(META_ENVIRONMENT, "dev");
        }
        db.save(
            &mut std::fs::File::create(&path).unwrap(),
            keepass::DatabaseKey::new().with_password("fake-test-password-123!"),
        )
        .unwrap();
        let session = Session::new(
            Arc::new(crate::session::MonotonicClock::default()),
            Box::new(crate::clipboard::tests::FakeClipboard(Arc::new(
                Mutex::new(crate::clipboard::tests::FakeData::default()),
            ))),
            Arc::new(|_| {}),
        );
        session.observe(crate::policy::Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::open(&path, "fake-test-password-123!").map_err(Into::into)
            })
            .unwrap();
        let scope = ExportScope::new(Some("app".into()), Some("dev".into()), false).unwrap();
        let error = extract_export(&session, &scope, "test").err().unwrap();
        assert!(
            error.message.contains("Process control"),
            "{}",
            error.message
        );
        assert!(error.message.contains("reserved"), "{}", error.message);
        assert!(!error.message.contains("fake"), "{}", error.message);
    }

    #[test]
    fn add_refuses_a_reserved_variable_name_for_the_desktop_too() {
        let (session, _directory) = test_session();
        let error = session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput {
                    title: "Process control".into(),
                    secret: "fake".into(),
                    env_var: Some("PATH".into()),
                    ..Default::default()
                })?;
                Ok(())
            })
            .err()
            .unwrap();
        assert!(error.message.contains("reserved"), "{}", error.message);
    }
    struct ScriptedConfirm {
        answer: bool,
        shown: Mutex<Vec<String>>,
        while_open: Box<dyn Fn() + Send + Sync>,
        window: Arc<std::sync::atomic::AtomicBool>,
        probe: Box<dyn Fn() + Send + Sync>,
        preparing: Box<dyn Fn() + Send + Sync>,
        before_window: Box<dyn Fn() + Send + Sync>,
        moved: Box<dyn Fn() + Send + Sync>,
        failure: Mutex<Option<crate::review_window::ReviewFailure>>,
    }
    impl ScriptedConfirm {
        fn new(answer: bool) -> Self {
            Self::during(answer, || {})
        }
        fn during(answer: bool, while_open: impl Fn() + Send + Sync + 'static) -> Self {
            Self {
                answer,
                shown: Mutex::new(Vec::new()),
                while_open: Box::new(while_open),
                window: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                probe: Box::new(|| {}),
                preparing: Box::new(|| {}),
                before_window: Box::new(|| {}),
                moved: Box::new(|| {}),
                failure: Mutex::new(None),
            }
        }
        /// Runs after the last check of the revision and before the last check of the session.
        fn on_prepare(mut self, preparing: impl Fn() + Send + Sync + 'static) -> Self {
            self.preparing = Box::new(preparing);
            self
        }
        /// Runs after every check, just before the review appears.
        fn on_present(mut self, before: impl Fn() + Send + Sync + 'static) -> Self {
            self.before_window = Box::new(before);
            self
        }
        /// The window fails instead of answering.
        fn failing(self, failure: crate::review_window::ReviewFailure) -> Self {
            *self.failure.lock().unwrap() = Some(failure);
            self
        }
        /// The shared flag says whether the window is shown and not minimized.
        fn on_window(mut self, window: Arc<std::sync::atomic::AtomicBool>) -> Self {
            self.window = window;
            self
        }
        /// Runs each time native asks about the window: the last moment before the dialog.
        fn on_probe(mut self, probe: impl Fn() + Send + Sync + 'static) -> Self {
            self.probe = Box::new(probe);
            self
        }
        /// Runs right after a recovery moved the damaged journal.
        fn on_journal_moved(mut self, moved: impl Fn() + Send + Sync + 'static) -> Self {
            self.moved = Box::new(moved);
            self
        }
        fn shown(&self) -> Vec<String> {
            self.shown.lock().unwrap().clone()
        }
    }
    impl ImportConfirm for ScriptedConfirm {
        fn journal_moved(&self) {
            (self.moved)();
        }
        fn review_dialog(&self) -> Box<dyn crate::review_window::ReviewDialog + '_> {
            (self.preparing)();
            Box::new(self)
        }
        fn window_live(&self) -> bool {
            (self.probe)();
            self.window.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl crate::review_window::ReviewDialog for &ScriptedConfirm {
        fn show(
            &self,
            review: &crate::review_window::ReviewText,
        ) -> Result<i32, crate::review_window::ReviewFailure> {
            (self.before_window)();
            self.shown.lock().unwrap().push(review.text.clone());
            (self.while_open)();
            if let Some(failure) = self.failure.lock().unwrap().take() {
                return Err(failure);
            }
            Ok(if self.answer {
                crate::review_window::CONFIRM_BUTTON
            } else {
                crate::review_window::CANCEL_BUTTON
            })
        }
    }

    fn owner(session: &Session) -> String {
        session
            .active_owner(session.snapshot().unwrap().ticket)
            .unwrap()
    }

    fn new_operation() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn import(
        session: &Session,
        confirm: &dyn ImportConfirm,
        text: &str,
        project: &str,
        environment: &str,
    ) -> Result<Option<ImportReport>, CommandError> {
        run_import(
            session,
            confirm,
            &owner(session),
            &new_operation(),
            Zeroizing::new(text.into()),
            Some(project.into()),
            Some(environment.into()),
        )
    }

    fn notices(session: &Session) -> Vec<serde_json::Value> {
        serde_json::to_value(session.warnings().unwrap()).unwrap()["operations"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn exported(session: &Session, project: &str, environment: &str) -> String {
        let scope =
            ExportScope::new(Some(project.into()), Some(environment.into()), false).unwrap();
        extract_export(session, &scope, "test")
            .unwrap()
            .value
            .as_str()
            .to_string()
    }

    #[test]
    fn an_import_runs_only_after_the_user_approves_the_names_and_scope_that_were_shown() {
        let (session, _directory) = test_session();
        let declined = ScriptedConfirm::new(false);
        assert!(
            import(&session, &declined, "TOKEN=fake-dev\n", "app", "dev")
                .unwrap()
                .is_none()
        );
        assert_eq!(exported(&session, "app", "dev"), "");
        let approved = ScriptedConfirm::new(true);
        let report = import(&session, &approved, "TOKEN=fake-dev\n", "app", "dev")
            .unwrap()
            .unwrap();
        assert_eq!((report.added, report.updated), (1, 0));
        assert_eq!(
            approved.shown(),
            vec![
                "Import into project \"app\", environment \"dev\"?\n\nAdd 1:\n  TOKEN\n\nSecret values are not shown."
                    .to_string()
            ]
        );
        assert!(!approved.shown()[0].contains("fake-dev"));
        assert_eq!(exported(&session, "app", "dev"), "TOKEN=fake-dev\n");
        let replace = ScriptedConfirm::new(true);
        let report = import(&session, &replace, "TOKEN=fake-next\n", "app", "dev")
            .unwrap()
            .unwrap();
        assert_eq!((report.added, report.updated), (0, 1));
        assert!(replace.shown()[0].contains("Replace the secret of 1:\n  TOKEN (entry \""));
        let prod = ScriptedConfirm::new(true);
        import(&session, &prod, "TOKEN=fake-prod\n", "app", "prod").unwrap();
        assert_eq!(exported(&session, "app", "dev"), "TOKEN=fake-next\n");
        assert_eq!(exported(&session, "app", "prod"), "TOKEN=fake-prod\n");
    }

    #[test]
    fn invalid_or_oversized_imports_are_refused_before_any_dialog() {
        let (session, _directory) = test_session();
        let confirm = ScriptedConfirm::new(true);
        for text in ["TOKEN=", "TOKEN=fake\nTOKEN=other", "PATH=fake"] {
            assert!(import(&session, &confirm, text, "app", "dev").is_err());
        }
        let owner = owner(&session);
        let run = |project: Option<&str>, environment: Option<&str>, text: String| {
            run_import(
                &session,
                &confirm,
                &owner,
                &new_operation(),
                Zeroizing::new(text),
                project.map(Into::into),
                environment.map(Into::into),
            )
        };
        assert!(run(None, Some("dev"), "TOKEN=fake".into()).is_err());
        assert!(run(Some("app"), None, "TOKEN=fake".into()).is_err());
        let many: String = (0..26).map(|n| format!("KEY_{n}=fake-{n}\n")).collect();
        assert_eq!(
            run(Some("app"), Some("dev"), many).err().unwrap().kind,
            "tooLarge"
        );
        assert!(run_import(
            &session,
            &confirm,
            &owner,
            "not-a-uuid",
            Zeroizing::new("TOKEN=fake".into()),
            Some("app".into()),
            Some("dev".into())
        )
        .is_err());
        assert!(
            confirm.shown().is_empty(),
            "no dialog for a refused request"
        );
        assert_eq!(exported(&session, "app", "dev"), "");
        // Each refused request passed its session check, so each left one failed record. The
        // malformed operation ID left none.
        let kinds: Vec<serde_json::Value> = notices(&session)
            .iter()
            .map(|record| record["kind"].clone())
            .collect();
        assert_eq!(kinds, vec![serde_json::json!("importFailed"); 6]);
    }

    #[test]
    fn a_hostile_or_oversized_env_text_is_refused_with_a_clear_message_before_any_dialog() {
        let (session, _directory) = test_session();
        let confirm = ScriptedConfirm::new(true);
        let big_file = format!("A=1\n# {}\n", "x".repeat(akm_core::dotenv::MAX_BYTES));
        let long_line = format!("A={}\n", "x".repeat(akm_core::dotenv::MAX_LINE_BYTES));
        let many: String = (0..=akm_core::dotenv::MAX_ENTRIES)
            .map(|n| format!("K{n}=fake\n"))
            .collect();
        for (text, fragment) in [
            (big_file.as_str(), "larger than 256 KiB"),
            (long_line.as_str(), "longer than 16 KiB"),
            (many.as_str(), "more than 500 entries"),
            ("HTTPS_PROXY=http://attacker.example\n", "reserved"),
            ("PYTHONSTARTUP=C:\\x\\run.py\n", "reserved"),
            ("A=\"never closed\n", "unterminated quote"),
        ] {
            let error = import(&session, &confirm, text, "app", "dev")
                .err()
                .unwrap();
            assert_eq!(error.kind, "invalid");
            assert!(error.message.contains(fragment), "{}", error.message);
            assert!(
                !error.message.contains("attacker.example"),
                "{}",
                error.message
            );
        }
        assert!(confirm.shown().is_empty());
        assert_eq!(exported(&session, "app", "dev"), "");
        let records = notices(&session);
        assert_eq!(
            records.len(),
            6,
            "one failed record for each refused request"
        );
        for record in &records {
            assert_eq!(record["kind"], "importFailed");
            assert!(!record.to_string().contains("attacker.example"), "{record}");
        }
    }

    #[test]
    fn every_change_while_the_dialog_is_open_leaves_the_import_unapplied_and_recorded() {
        for change in ["lock", "session", "write", "reload"] {
            let (session, directory) = test_session();
            let confirm = {
                let session = session.clone();
                let other = directory.path().join("other.kdbx");
                ScriptedConfirm::during(true, move || match change {
                    "lock" => session.lock().unwrap(),
                    "session" => {
                        session.lock().unwrap();
                        let ticket = session.begin_open().unwrap();
                        session
                            .open(ticket, || {
                                Vault::create(&other, "fake-test-password-123!", None)
                                    .map_err(Into::into)
                            })
                            .unwrap();
                    }
                    "write" => {
                        session
                            .access(Access::Write, None, |vault| {
                                vault.add(akm_core::ApiKeyInput::new("Other", "fake-other"))?;
                                Ok(())
                            })
                            .unwrap();
                    }
                    "reload" => {
                        session
                            .access(Access::Reload, None, |vault| {
                                vault.reload().map_err(Into::into)
                            })
                            .unwrap();
                    }
                    _ => unreachable!(),
                })
            };
            let operation = new_operation();
            let error = run_import(
                &session,
                &confirm,
                &owner(&session),
                &operation,
                Zeroizing::new("TOKEN=fake-late\n".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .err()
            .unwrap_or_else(|| panic!("{change}: the import ran"));
            assert_eq!(
                error.kind,
                if change == "lock" {
                    "locked"
                } else {
                    "stalePreview"
                },
                "{change}"
            );
            assert_eq!(confirm.shown().len(), 1, "{change}");
            let record = notices(&session);
            assert_eq!(record.len(), 1, "{change}");
            assert_eq!(record[0]["id"], operation.as_str());
            assert_eq!(record[0]["kind"], "importFailed", "{change}");
            for name in ["vault.kdbx", "other.kdbx"] {
                let path = directory.path().join(name);
                if path.exists() {
                    let reopened = Vault::open(path, "fake-test-password-123!").unwrap();
                    assert!(
                        crate::test_support::env_of(&reopened, "app", "dev").is_empty(),
                        "{change}: nothing was imported into {name}"
                    );
                }
            }
        }
    }

    #[test]
    fn p2r2_02_the_click_in_the_dialog_stands_after_a_hide_a_page_load_a_discard_or_a_hidden_window(
    ) {
        for change in ["hide", "page-load", "discard", "window"] {
            let (session, directory) = test_session();
            let window = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let confirm = {
                let (session, hidden) = (session.clone(), window.clone());
                let id = owner(&session);
                ScriptedConfirm::during(true, move || match change {
                    "hide" => {
                        session.expire_presentation().unwrap();
                    }
                    "page-load" => session.document_started(),
                    "discard" => discard_view(&session, &id, |_| Ok(())).unwrap(),
                    "window" => hidden.store(false, std::sync::atomic::Ordering::SeqCst),
                    _ => unreachable!(),
                })
                .on_window(window)
            };
            let operation = new_operation();
            let report = run_import(
                &session,
                &confirm,
                &owner(&session),
                &operation,
                Zeroizing::new("TOKEN=fake-approved\n".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .unwrap_or_else(|error| panic!("{change}: the approval was refused: {error:?}"))
            .unwrap();
            assert_eq!((report.added, report.updated), (1, 0), "{change}");
            assert_eq!(confirm.shown().len(), 1, "{change}");
            let record = notices(&session);
            assert_eq!(record.len(), 1, "{change}");
            assert_eq!(record[0]["id"], operation.as_str());
            assert_eq!(record[0]["kind"], "importApplied", "{change}");
            assert!(session.snapshot().is_ok(), "{change}: still unlocked");
            let reopened = Vault::open(
                directory.path().join("vault.kdbx"),
                "fake-test-password-123!",
            )
            .unwrap();
            assert_eq!(
                crate::test_support::env_of(&reopened, "app", "dev"),
                [("TOKEN".into(), "fake-approved".into())],
                "{change}"
            );
        }
    }

    #[test]
    fn p2r2_02_a_discard_that_native_cannot_take_locks_the_session_and_the_lock_refuses_the_approval(
    ) {
        let (session, directory) = test_session();
        let confirm = {
            let session = session.clone();
            let id = owner(&session);
            ScriptedConfirm::during(true, move || {
                assert!(discard_view(&session, &id, |_| Err(())).is_err());
            })
        };
        let operation = new_operation();
        let error = run_import(
            &session,
            &confirm,
            &owner(&session),
            &operation,
            Zeroizing::new("TOKEN=fake-late\n".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .err()
        .expect("the import ran after the session was locked");
        assert_eq!(error.kind, "locked");
        let record = notices(&session);
        assert_eq!(record.len(), 1);
        assert_eq!(record[0]["kind"], "importFailed");
        let reopened = Vault::open(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert!(crate::test_support::env_of(&reopened, "app", "dev").is_empty());
    }

    #[test]
    fn review1_1_a_discard_whose_notice_is_lost_locks_that_owner_and_a_locked_session_ignores_it() {
        let (session, _directory) = test_session();
        let id = owner(&session);
        assert!(discard_view(&session, &id, |_| Err(())).is_err());
        assert_eq!(session.snapshot().unwrap_err().kind, "locked");
        let mut emitted = false;
        discard_view(&session, &id, |_| {
            emitted = true;
            Ok(())
        })
        .unwrap();
        assert!(!emitted, "a locked session has no view to discard");
    }

    fn reopen(session: &Session, directory: &tempfile::TempDir, name: &str) -> String {
        let owner = session.prepare_open().unwrap();
        let ticket = session.claim_open(&owner).unwrap();
        session
            .open(ticket, || {
                Vault::create(directory.path().join(name), "fake-test-password-123!", None)
                    .map_err(Into::into)
            })
            .unwrap();
        owner
    }

    #[test]
    fn p2r2_03_a_discard_for_a_session_that_is_not_current_succeeds_and_changes_nothing() {
        let (session, directory) = test_session();
        let a = owner(&session);
        let b = reopen(&session, &directory, "b.kdbx");
        let before = session.snapshot().unwrap();
        for stale in [
            a.clone(),
            uuid::Uuid::new_v4().to_string(),
            "not-an-id".into(),
        ] {
            let mut emitted = false;
            discard_view(&session, &stale, |_| {
                emitted = true;
                Ok(())
            })
            .unwrap();
            assert!(!emitted, "{stale}: no notice names the current session");
            assert_eq!(session.snapshot().unwrap(), before, "{stale}");
        }
        discard_view(&session, &a, |_| Err(())).expect("a stale discard never reaches the emit");
        assert!(session.snapshot().is_ok());
        let mut named = Vec::new();
        discard_view(&session, &b, |notice| {
            named.push(notice.session_id.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(named, [b]);
        assert_ne!(
            session.snapshot().unwrap().presentation,
            before.presentation
        );
    }

    #[test]
    fn p2r2_03_a_failed_discard_of_session_a_never_locks_the_session_that_replaced_it() {
        let (session, directory) = test_session();
        let a = owner(&session);
        let mut b = None;
        let error = discard_view(&session, &a, |notice| {
            assert_eq!(notice.session_id, a);
            b = Some(reopen(&session, &directory, "b.kdbx"));
            Err(())
        })
        .unwrap_err();
        assert_eq!(error.kind, "internal");
        assert_eq!(
            owner(&session),
            b.unwrap(),
            "B is still the unlocked session"
        );
    }

    #[test]
    fn t_r3_05_a_window_that_is_not_shown_gets_no_dialog_and_leaves_a_failed_record() {
        let (session, _directory) = test_session();
        let confirm = ScriptedConfirm::new(true)
            .on_window(Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let error = run_import(
            &session,
            &confirm,
            &owner(&session),
            &new_operation(),
            Zeroizing::new("TOKEN=fake-late\n".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .err()
        .expect("a dialog for a window nobody sees");
        assert_eq!(error.kind, "viewChanged");
        assert!(confirm.shown().is_empty());
        let records = notices(&session);
        assert_eq!(records.len(), 1, "a lost reply still has its outcome");
        assert_eq!(records[0]["kind"], "importFailed");
        assert_eq!(exported(&session, "app", "dev"), "");
    }

    #[test]
    fn review1_1_a_change_after_the_review_was_read_and_before_the_dialog_shows_no_dialog() {
        for change in ["lock", "write"] {
            let (session, directory) = test_session();
            let confirm = {
                let session = session.clone();
                ScriptedConfirm::new(true).on_probe(move || match change {
                    "lock" => session.lock().unwrap(),
                    _ => {
                        session
                            .access(Access::Write, None, |vault| {
                                vault.add(akm_core::ApiKeyInput::new("Other", "fake-other"))?;
                                Ok(())
                            })
                            .unwrap();
                    }
                })
            };
            let error = run_import(
                &session,
                &confirm,
                &owner(&session),
                &new_operation(),
                Zeroizing::new("TOKEN=fake-late\n".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .err()
            .unwrap_or_else(|| panic!("{change}: the import ran"));
            assert_eq!(
                error.kind,
                if change == "lock" {
                    "locked"
                } else {
                    "stalePreview"
                },
                "{change}"
            );
            assert!(confirm.shown().is_empty(), "{change}: a dialog was shown");
            let reopened = Vault::open(
                directory.path().join("vault.kdbx"),
                "fake-test-password-123!",
            )
            .unwrap();
            assert!(crate::test_support::env_of(&reopened, "app", "dev").is_empty());
        }
    }

    #[test]
    fn p2r2_02_a_hide_before_the_dialog_opens_does_not_withhold_the_dialog() {
        let (session, _directory) = test_session();
        let confirm = {
            let session = session.clone();
            ScriptedConfirm::new(true).on_probe(move || {
                session.expire_presentation().unwrap();
            })
        };
        let report = run_import(
            &session,
            &confirm,
            &owner(&session),
            &new_operation(),
            Zeroizing::new("TOKEN=fake-approved\n".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .unwrap()
        .unwrap();
        assert_eq!((report.added, report.updated), (1, 0));
        assert_eq!(confirm.shown().len(), 1);
    }

    #[test]
    fn a_request_for_an_older_or_unknown_session_is_refused_without_a_dialog() {
        let (session, directory) = test_session();
        let old = owner(&session);
        session.lock().unwrap();
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::open(
                    directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
        let confirm = ScriptedConfirm::new(true);
        for stale in [old, uuid::Uuid::new_v4().to_string(), "not-a-uuid".into()] {
            let error = run_import(
                &session,
                &confirm,
                &stale,
                &new_operation(),
                Zeroizing::new("TOKEN=fake\n".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .err()
            .unwrap();
            assert_eq!(error.kind, "locked");
        }
        assert!(confirm.shown().is_empty());
        assert_eq!(exported(&session, "app", "dev"), "");
        assert!(notices(&session).is_empty());
        assert!(import(
            &session,
            &ScriptedConfirm::new(true),
            "TOKEN=fake\n",
            "app",
            "dev"
        )
        .is_ok());
    }

    #[test]
    fn a_late_request_a_neither_consumes_nor_blocks_a_valid_request_b() {
        let (session, _directory) = test_session();
        let inner_session = session.clone();
        let a = ScriptedConfirm::during(true, move || {
            let b = ScriptedConfirm::new(true);
            let report = import(&inner_session, &b, "TOKEN=fake-b\n", "app", "dev")
                .unwrap()
                .unwrap();
            assert_eq!((report.added, report.updated), (1, 0));
        });
        let a_error = import(&session, &a, "TOKEN=fake-a\n", "app", "dev")
            .err()
            .unwrap();
        assert_eq!(a_error.kind, "stalePreview");
        assert_eq!(exported(&session, "app", "dev"), "TOKEN=fake-b\n");
        let again = ScriptedConfirm::new(false);
        assert!(import(&session, &again, "TOKEN=fake-a\n", "app", "dev")
            .unwrap()
            .is_none());
        assert_eq!(again.shown().len(), 1);
        assert_eq!(exported(&session, "app", "dev"), "TOKEN=fake-b\n");
        let replay = ScriptedConfirm::new(true);
        let report = import(&session, &replay, "TOKEN=fake-b\n", "app", "dev")
            .unwrap()
            .unwrap();
        assert_eq!((report.added, report.updated), (0, 0));
        assert!(replay.shown().is_empty());
    }

    #[test]
    fn a_dispatched_import_leaves_a_secret_free_outcome_record_that_a_lost_reply_cannot_erase() {
        let (session, _directory) = test_session();
        for (answer, text, kind) in [
            (true, "TOKEN=fake-secret-one\n", "importApplied"),
            (false, "TOKEN=fake-secret-two\n", "importDeclined"),
        ] {
            let operation = new_operation();
            let reply = run_import(
                &session,
                &ScriptedConfirm::new(answer),
                &owner(&session),
                &operation,
                Zeroizing::new(text.into()),
                Some("app".into()),
                Some("dev".into()),
            );
            drop(reply);
            let record = notices(&session)
                .into_iter()
                .find(|record| record["id"] == operation.as_str())
                .expect("record");
            assert_eq!(record["kind"], kind);
            assert!(!record.to_string().contains("fake-secret"));
            assert_eq!(record.as_object().unwrap().len(), 3);
        }
        session.lock().unwrap();
        assert_eq!(notices(&session).len(), 2, "records survive a lock");
        let ids: Vec<String> = notices(&session)
            .iter()
            .map(|record| record["id"].as_str().unwrap().to_string())
            .collect();
        session.dismiss_warnings(&ids).unwrap();
        assert!(notices(&session).is_empty());
    }

    #[test]
    fn a_committed_import_keeps_its_receipt_when_lock_arrives_during_the_journal_append() {
        use std::time::{Duration, Instant};
        let (session, directory) = test_session();
        let path = directory.path().join("vault.kdbx");
        let initial = fs::read(&path).unwrap();
        let journal = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(crate::test_support::journal_of(&path).lock_path())
            .unwrap();
        journal.lock().unwrap();
        let worker = session.clone();
        let operation = new_operation();
        let recorded = operation.clone();
        let writer = std::thread::spawn(move || {
            run_import(
                &worker,
                &ScriptedConfirm::new(true),
                &owner(&worker),
                &recorded,
                Zeroizing::new("TOKEN=fake-saved-once\n".into()),
                Some("app".into()),
                Some("dev".into()),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        let committed = loop {
            if fs::read(&path).unwrap() != initial {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::yield_now();
        };
        session.lock().unwrap();
        drop(journal);
        let outcome = writer.join().unwrap();
        assert!(committed, "import did not reach the disk commit");
        assert_eq!(outcome.err().unwrap().kind, "completedWhileLocked");
        assert!(session.snapshot().is_err());
        let record = notices(&session);
        assert_eq!(record.len(), 1);
        assert_eq!(record[0]["id"], operation.as_str());
        assert_eq!(record[0]["kind"], "importSessionEnded");
        let reopened = Vault::open(path, "fake-test-password-123!").unwrap();
        assert_eq!(
            crate::test_support::env_of(&reopened, "app", "dev"),
            [("TOKEN".into(), "fake-saved-once".into())]
        );
    }

    struct PausingPreferences {
        inner: crate::settings::FilePreferences,
        pause_first: std::sync::atomic::AtomicBool,
        entered: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl PreferenceStore for PausingPreferences {
        fn saved_vault_path(&self) -> Option<String> {
            self.inner.saved_vault_path()
        }
        fn default_vault_path(&self) -> Result<PathBuf, CommandError> {
            self.inner.default_vault_path()
        }
        fn save_vault_path(
            &self,
            ticket: crate::policy::Ticket,
            owned: crate::settings::OwnerFence<'_>,
            vault_path: &str,
        ) -> Result<crate::settings::PreferenceSave, CommandError> {
            if self
                .pause_first
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                self.entered.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .unwrap();
            }
            self.inner.save_vault_path(ticket, owned, vault_path)
        }
    }

    #[test]
    fn an_older_opening_never_overwrites_a_newer_openings_saved_path() {
        use crate::{clipboard::tests::FakeData, policy::Signal, session::MonotonicClock};
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let [a_path, b_path] = ["a.kdbx", "b.kdbx"].map(|name| {
            let path = directory.path().join(name);
            Vault::create(&path, "fake-test-password-123!", None).unwrap();
            path
        });
        let settings = directory.path().join("fake-settings").join("settings.json");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let store = Arc::new(PausingPreferences {
            inner: crate::settings::FilePreferences::new(
                Some(settings.clone()),
                Some(directory.path().join("default.kdbx")),
            ),
            pause_first: std::sync::atomic::AtomicBool::new(true),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(crate::clipboard::tests::FakeClipboard(Arc::new(
                Mutex::new(FakeData::default()),
            ))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        fn open(
            session: &Arc<Session>,
            store: &Arc<PausingPreferences>,
            owner: &str,
            path: PathBuf,
        ) -> Result<Status, CommandError> {
            tauri::async_runtime::block_on(open_vault(
                session.clone(),
                store.clone(),
                path,
                Zeroizing::new("fake-test-password-123!".into()),
                None,
                false,
                owner,
            ))
        }
        let owner_a = session.prepare_open().unwrap();
        let first = {
            let (session, store, owner, path) =
                (session.clone(), store.clone(), owner_a.clone(), a_path);
            std::thread::spawn(move || open(&session, &store, &owner, path))
        };
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        let owner_b = session.prepare_open().unwrap();
        let b_status = open(&session, &store, &owner_b, b_path.clone()).unwrap();
        assert_eq!(b_status.session_id.as_deref(), Some(owner_b.as_str()));
        let saved =
            || -> Settings { serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap() };
        assert_eq!(saved().vault_path, Some(b_path.display().to_string()));
        release_tx.send(()).unwrap();
        let a_outcome = first.join().unwrap();
        assert!(a_outcome.is_err(), "the retired opening A must not succeed");
        assert_eq!(saved().vault_path, Some(b_path.display().to_string()));
        let current = session.snapshot().unwrap();
        assert_eq!(session.active_owner(current.ticket).unwrap(), owner_b);
        let report = serde_json::to_value(session.warnings().unwrap()).unwrap();
        assert_eq!(report["operations"].as_array().unwrap().len(), 0);
    }

    struct BarrierAfterCheck {
        inner: crate::settings::FilePreferences,
        armed: std::sync::atomic::AtomicBool,
        entered: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
        log: Arc<Mutex<Vec<&'static str>>>,
    }
    impl PreferenceStore for BarrierAfterCheck {
        fn saved_vault_path(&self) -> Option<String> {
            self.inner.saved_vault_path()
        }
        fn default_vault_path(&self) -> Result<PathBuf, CommandError> {
            self.inner.default_vault_path()
        }
        fn save_vault_path(
            &self,
            ticket: crate::policy::Ticket,
            owned: crate::settings::OwnerFence<'_>,
            vault_path: &str,
        ) -> Result<crate::settings::PreferenceSave, CommandError> {
            if !self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return self.inner.save_vault_path(ticket, owned, vault_path);
            }
            let saved = self.inner.save_vault_path(
                ticket,
                &|effect| {
                    owned(&mut BarrierInAct {
                        store: self,
                        inner: effect,
                    })
                },
                vault_path,
            );
            self.log.lock().unwrap().push("A-saved");
            saved
        }
    }

    /// The act of the settings commit, with a barrier inside it, under the gate.
    struct BarrierInAct<'a> {
        store: &'a BarrierAfterCheck,
        inner: &'a mut dyn crate::session::Effect<Output = ()>,
    }
    impl crate::session::Effect for BarrierInAct<'_> {
        type Output = ();
        fn act(&mut self, held: &crate::session::Held<'_>) -> Result<(), CommandError> {
            self.store.entered.send(()).unwrap();
            self.store
                .release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap();
            self.inner.act(held)
        }
        fn revoke(&mut self) -> Result<(), String> {
            self.inner.revoke()
        }
    }

    #[test]
    fn review1_2_a_retiring_opening_cannot_interleave_between_the_ownership_check_and_the_settings_write(
    ) {
        use crate::{clipboard::tests::FakeData, policy::Signal, session::MonotonicClock};
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let a_path = directory.path().join("a.kdbx");
        Vault::create(&a_path, "fake-test-password-123!", None).unwrap();
        let missing = directory.path().join("missing.kdbx");
        let settings = directory.path().join("fake-settings").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, r#"{"vaultPath":"pre-existing.kdbx"}"#).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let log = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(BarrierAfterCheck {
            inner: crate::settings::FilePreferences::new(Some(settings.clone()), None),
            armed: std::sync::atomic::AtomicBool::new(true),
            entered: entered_tx,
            release: Mutex::new(release_rx),
            log: log.clone(),
        });
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(crate::clipboard::tests::FakeClipboard(Arc::new(
                Mutex::new(FakeData::default()),
            ))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        let open = |session: Arc<Session>, store: Arc<BarrierAfterCheck>, owner: String, path| {
            tauri::async_runtime::block_on(open_vault(
                session,
                store,
                path,
                Zeroizing::new("fake-test-password-123!".into()),
                None,
                false,
                &owner,
            ))
        };
        let owner_a = session.prepare_open().unwrap();
        let first = {
            let (session, store) = (session.clone(), store.clone());
            std::thread::spawn(move || open(session, store, owner_a, a_path))
        };
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        let (retired_tx, retired_rx) = mpsc::channel();
        let second = {
            let (session, store, log) = (session.clone(), store.clone(), log.clone());
            std::thread::spawn(move || {
                let owner_b = session.prepare_open().unwrap();
                log.lock().unwrap().push("B-retired-A");
                retired_tx.send(()).unwrap();
                open(session, store, owner_b, missing)
            })
        };
        let _ = retired_rx.recv_timeout(std::time::Duration::from_millis(500));
        release_tx.send(()).unwrap();
        let _ = first.join().unwrap();
        assert!(second.join().unwrap().is_err(), "B must fail to open");
        let saved: Settings = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        let events = log.lock().unwrap().clone();
        let a_wrote = saved
            .vault_path
            .as_deref()
            .is_some_and(|path| path.ends_with("a.kdbx"));
        if a_wrote {
            assert_eq!(
                events,
                ["A-saved", "B-retired-A"],
                "A may write only while it still owns the session, never after B retired it"
            );
        } else {
            assert_eq!(saved.vault_path.as_deref(), Some("pre-existing.kdbx"));
        }
    }

    #[test]
    fn review1_2_an_opening_retired_by_a_failed_newer_opening_leaves_the_earlier_preference() {
        use crate::{clipboard::tests::FakeData, policy::Signal, session::MonotonicClock};
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let a_path = directory.path().join("a.kdbx");
        Vault::create(&a_path, "fake-test-password-123!", None).unwrap();
        let settings = directory.path().join("fake-settings").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, r#"{"vaultPath":"pre-existing.kdbx"}"#).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let store = Arc::new(PausingPreferences {
            inner: crate::settings::FilePreferences::new(Some(settings.clone()), None),
            pause_first: std::sync::atomic::AtomicBool::new(true),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(crate::clipboard::tests::FakeClipboard(Arc::new(
                Mutex::new(FakeData::default()),
            ))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        let open = |session: Arc<Session>, store: Arc<PausingPreferences>, owner: String, path| {
            tauri::async_runtime::block_on(open_vault(
                session,
                store,
                path,
                Zeroizing::new("fake-test-password-123!".into()),
                None,
                false,
                &owner,
            ))
        };
        let owner_a = session.prepare_open().unwrap();
        let first = {
            let (session, store) = (session.clone(), store.clone());
            std::thread::spawn(move || open(session, store, owner_a, a_path))
        };
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        let owner_b = session.prepare_open().unwrap();
        assert!(open(
            session.clone(),
            store.clone(),
            owner_b,
            directory.path().join("missing.kdbx")
        )
        .is_err());
        release_tx.send(()).unwrap();
        assert!(first.join().unwrap().is_err());
        let saved: Settings = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        assert_eq!(saved.vault_path.as_deref(), Some("pre-existing.kdbx"));
    }

    // ---- FR-03: a disclosure needs the user, not the session ID ----

    struct Stepped(std::sync::atomic::AtomicU64);
    impl Stepped {
        fn set(&self, seconds: u64) {
            self.0.store(seconds, std::sync::atomic::Ordering::SeqCst);
        }
    }
    impl crate::session::Clock for Stepped {
        fn now(&self) -> std::time::Duration {
            std::time::Duration::from_secs(self.0.load(std::sync::atomic::Ordering::SeqCst))
        }
    }

    struct Disclosing {
        session: Arc<Session>,
        clock: Arc<Stepped>,
        clipboard: Arc<Mutex<crate::clipboard::tests::FakeData>>,
        keys: Vec<String>,
        _directory: tempfile::TempDir,
    }

    /// An unlocked session with a fake clock, a fake clipboard and three fake keys: two in
    /// `app/dev` and one in `app/prod`.
    fn disclosing() -> Disclosing {
        use crate::{
            clipboard::tests::{FakeClipboard, FakeData},
            policy::Signal,
        };
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(Stepped(std::sync::atomic::AtomicU64::new(1_000)));
        let clipboard = Arc::new(Mutex::new(FakeData::default()));
        let session = Session::new(
            clock.clone(),
            Box::new(FakeClipboard(clipboard.clone())),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::create(
                    directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                    None,
                )
                .map_err(Into::into)
            })
            .unwrap();
        let mut keys = Vec::new();
        session
            .access(Access::Write, None, |vault| {
                for (title, secret, project, environment, variable) in [
                    ("One", "fake-secret-one", "app", "dev", "ONE_KEY"),
                    ("Two", "fake-secret-two", "app", "dev", "TWO_KEY"),
                    ("Three", "fake-secret-three", "app", "prod", "THREE_KEY"),
                ] {
                    let key = vault.add(akm_core::ApiKeyInput {
                        project: Some(project.into()),
                        environment: Some(environment.into()),
                        env_var: Some(variable.into()),
                        ..akm_core::ApiKeyInput::new(title, secret)
                    })?;
                    keys.push(key.id.to_string());
                }
                Ok(())
            })
            .unwrap();
        Disclosing {
            session,
            clock,
            clipboard,
            keys,
            _directory: directory,
        }
    }

    fn reveal_one(
        f: &Disclosing,
        host: &ScriptedConfirm,
        action: KeyDisclosure,
    ) -> Result<Option<Stamped<Zeroizing<String>>>, CommandError> {
        single_disclosure(&f.session, host, &owner(&f.session), &f.keys[0], action)
    }

    fn bulk_preview(
        f: &Disclosing,
        host: &ScriptedConfirm,
        scope: &ExportScope,
    ) -> Result<Option<String>, CommandError> {
        bulk_disclosure(
            &f.session,
            host,
            &owner(&f.session),
            scope,
            BulkUse::Preview,
            |expected| {
                let exported = extract_export_at(&f.session, expected, scope, ".env preview")?;
                f.session.reveal(exported)
            },
        )
    }

    fn bulk_copy(
        f: &Disclosing,
        host: &ScriptedConfirm,
        scope: &ExportScope,
    ) -> Result<Option<u64>, CommandError> {
        bulk_disclosure(
            &f.session,
            host,
            &owner(&f.session),
            scope,
            BulkUse::Clipboard,
            |expected| {
                let exported = extract_export_at(&f.session, expected, scope, "clipboard")?;
                f.session.copy_stamped(exported)?;
                Ok(CLEAR_AFTER.as_secs())
            },
        )
    }

    fn everything() -> ExportScope {
        ExportScope::new(None, None, true).unwrap()
    }

    fn clipboard_text(f: &Disclosing) -> String {
        f.clipboard.lock().unwrap().text.clone()
    }

    #[test]
    fn fr03_a_bulk_preview_shows_the_scope_and_the_names_and_reveals_only_after_approval() {
        let f = disclosing();
        let declined = ScriptedConfirm::new(false);

        let answer = bulk_preview(&f, &declined, &everything()).unwrap();

        assert_eq!(answer, None, "declined gives no value");
        let shown = declined.shown();
        assert_eq!(shown.len(), 1);
        assert!(shown[0].contains("ALL keys of the vault"), "{}", shown[0]);
        for name in ["ONE_KEY", "TWO_KEY", "THREE_KEY"] {
            assert!(shown[0].contains(name), "{name}: {}", shown[0]);
        }
        assert!(
            !shown[0].contains("fake-secret"),
            "the window never shows a value: {}",
            shown[0]
        );

        let approved = ScriptedConfirm::new(true);
        let scope = ExportScope::new(Some("app".into()), Some("dev".into()), false).unwrap();
        let text = bulk_preview(&f, &approved, &scope).unwrap().unwrap();
        assert!(text.contains("fake-secret-one") && text.contains("fake-secret-two"));
        assert!(!text.contains("fake-secret-three"), "only the scope");
        let window = &approved.shown()[0];
        assert!(
            window.contains(r#"project "app", environment "dev""#),
            "{window}"
        );
        assert!(window.contains("2 variables"), "{window}");
        assert!(!window.contains("THREE_KEY"), "{window}");
    }

    #[test]
    fn fr03_a_declined_bulk_copy_writes_nothing_to_the_clipboard() {
        let f = disclosing();

        let answer = bulk_copy(&f, &ScriptedConfirm::new(false), &everything()).unwrap();

        assert_eq!(answer, None);
        let data = f.clipboard.lock().unwrap();
        assert_eq!(data.sequence, 0, "no clipboard write happened");
        assert_eq!(data.text, "");
    }

    #[test]
    fn fr03_an_approved_bulk_copy_reaches_the_clipboard_once() {
        let f = disclosing();

        let answer = bulk_copy(&f, &ScriptedConfirm::new(true), &everything()).unwrap();

        assert_eq!(answer, Some(CLEAR_AFTER.as_secs()));
        assert!(clipboard_text(&f).contains("fake-secret-three"));
        assert_eq!(f.clipboard.lock().unwrap().sequence, 1);
    }

    #[test]
    fn fr03_a_window_that_fails_is_an_error_and_never_a_decline() {
        let f = disclosing();
        let broken = ScriptedConfirm::new(true).failing(crate::review_window::ReviewFailure::new(
            "the window code stopped",
        ));

        let error = bulk_copy(&f, &broken, &everything()).unwrap_err();

        assert_eq!(error.kind, "reviewFailed");
        assert_eq!(f.clipboard.lock().unwrap().sequence, 0);
        let broken = ScriptedConfirm::new(true).failing(crate::review_window::ReviewFailure::new(
            "the window code stopped",
        ));
        assert_eq!(
            bulk_preview(&f, &broken, &everything()).unwrap_err().kind,
            "reviewFailed"
        );
    }

    #[test]
    fn fr03_a_lock_while_the_window_is_open_discloses_nothing() {
        let f = disclosing();
        let session = f.session.clone();
        let locking = ScriptedConfirm::during(true, move || session.lock().unwrap());

        let error = bulk_copy(&f, &locking, &everything()).unwrap_err();

        assert_eq!(error.kind, "locked");
        assert_eq!(
            f.clipboard.lock().unwrap().sequence,
            0,
            "nothing was copied"
        );
    }

    /// Class A: a modal window cannot run under the gate, so the session is checked again after
    /// it. A new session opened while the window was open ends the request as `locked`: the
    /// session that asked is gone. Nothing is disclosed.
    #[test]
    fn fr03_a_new_session_while_the_window_is_open_answers_locked_and_discloses_nothing() {
        let f = disclosing();
        let session = f.session.clone();
        let vault = f._directory.path().join("vault.kdbx");
        let reopening = ScriptedConfirm::during(true, move || {
            session.lock().unwrap();
            session.observe(crate::policy::Signal::Available);
            let ticket = session.begin_open().unwrap();
            session
                .open(ticket, || {
                    Vault::open(&vault, "fake-test-password-123!").map_err(Into::into)
                })
                .unwrap();
        });

        let error = bulk_preview(&f, &reopening, &everything()).unwrap_err();

        assert_eq!(error.kind, "locked", "{error:?}");
        assert!(!error.message.contains("fake"), "{}", error.message);
    }

    #[test]
    fn fr03_a_change_while_the_window_is_open_refuses_the_disclosure() {
        let f = disclosing();
        let session = f.session.clone();
        let changing = ScriptedConfirm::during(true, move || {
            session
                .access(Access::Write, None, |vault| {
                    vault.add(akm_core::ApiKeyInput {
                        env_var: Some("LATE_KEY".into()),
                        ..akm_core::ApiKeyInput::new("Late", "fake-late")
                    })?;
                    Ok(())
                })
                .unwrap();
        });

        let error = bulk_preview(&f, &changing, &everything()).unwrap_err();

        assert_eq!(
            error.kind, "stalePreview",
            "the plan that was approved is not the plan that would be read: {error:?}"
        );
    }

    #[test]
    fn fr03_a_hide_while_the_window_is_open_refuses_the_disclosure() {
        let f = disclosing();
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            reveal_one(&f, &ScriptedConfirm::new(true), KeyDisclosure::Copy)
                .unwrap()
                .unwrap();
        }
        let session = f.session.clone();
        let hiding = ScriptedConfirm::during(true, move || {
            session.expire_presentation().unwrap();
        });
        let single = reveal_one(&f, &hiding, KeyDisclosure::Reveal);
        assert!(
            single.is_err(),
            "a single key is not shown to a view that was hidden"
        );

        let session = f.session.clone();
        let hiding = ScriptedConfirm::during(true, move || {
            session.expire_presentation().unwrap();
        });
        let before = f.clipboard.lock().unwrap().sequence;
        assert!(bulk_copy(&f, &hiding, &everything()).is_err());
        assert!(bulk_preview(
            &f,
            &ScriptedConfirm::during(true, {
                let session = f.session.clone();
                move || {
                    session.expire_presentation().unwrap();
                }
            }),
            &everything()
        )
        .is_err());
        assert_eq!(
            f.clipboard.lock().unwrap().sequence,
            before,
            "nothing was copied"
        );
    }

    #[test]
    fn fr03_a_hidden_window_shows_no_dialog_and_discloses_nothing() {
        let f = disclosing();
        let hidden = ScriptedConfirm::new(true);
        hidden
            .window
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let error = bulk_copy(&f, &hidden, &everything()).unwrap_err();

        assert_eq!(error.kind, "viewChanged");
        assert!(hidden.shown().is_empty());
        assert_eq!(f.clipboard.lock().unwrap().sequence, 0);
    }

    #[test]
    fn fr03_an_empty_scope_shows_no_window_and_reveals_no_value() {
        let f = disclosing();
        let host = ScriptedConfirm::new(true);
        let scope = ExportScope::new(Some("none".into()), Some("none".into()), false).unwrap();

        let text = bulk_preview(&f, &host, &scope).unwrap().unwrap();

        assert!(host.shown().is_empty());
        assert!(!text.contains("fake-secret"), "{text}");
    }

    #[test]
    fn fr03_a_second_window_is_refused_while_one_is_open() {
        let f = disclosing();
        let session = f.session.clone();
        let inner = Arc::new(Mutex::new(None));
        let sink = inner.clone();
        let (gate_session, keys) = (f.session.clone(), f.keys.clone());
        let outer = ScriptedConfirm::during(true, move || {
            let second = ScriptedConfirm::new(true);
            let result = bulk_disclosure(
                &gate_session,
                &second,
                &owner(&gate_session),
                &everything(),
                BulkUse::Preview,
                |_| Ok(()),
            );
            *sink.lock().unwrap() = Some((result.err().map(|e| e.kind), second.shown().len()));
            let _ = &keys;
        });

        bulk_preview(&f, &outer, &everything()).unwrap();

        let (kind, shown) = inner.lock().unwrap().take().unwrap();
        assert_eq!(kind.as_deref(), Some("busy"));
        assert_eq!(shown, 0, "the second window never opened");
        let _ = session;
    }

    #[test]
    fn fr03_single_keys_are_free_up_to_the_budget_then_need_the_user() {
        let f = disclosing();
        let host = ScriptedConfirm::new(false);

        for n in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            let secret = reveal_one(&f, &host, KeyDisclosure::Reveal)
                .unwrap_or_else(|e| panic!("request {n}: {e:?}"))
                .unwrap_or_else(|| panic!("request {n} is inside the budget"));
            assert_eq!(secret.value.as_str(), "fake-secret-one");
        }
        assert!(host.shown().is_empty(), "no window inside the budget");

        let sixth = reveal_one(&f, &host, KeyDisclosure::Reveal).unwrap();

        assert!(sixth.is_none(), "the sixth is declined by the user");
        let shown = host.shown();
        assert_eq!(shown.len(), 1);
        assert!(shown[0].contains("One"), "names the key: {}", shown[0]);
        assert!(!shown[0].contains("fake-secret"), "{}", shown[0]);
    }

    #[test]
    fn fr03_a_loop_of_single_copies_cannot_drain_the_vault_without_approval() {
        let f = disclosing();
        let host = ScriptedConfirm::new(false);
        let mut leaked = 0;

        for _ in 0..40 {
            if single_disclosure(
                &f.session,
                &host,
                &owner(&f.session),
                &f.keys[1],
                KeyDisclosure::Copy,
            )
            .unwrap()
            .map(|secret| f.session.copy_stamped(secret))
            .transpose()
            .unwrap()
            .is_some()
            {
                leaked += 1;
            }
        }

        assert_eq!(leaked, crate::session::FREE_SINGLE_DISCLOSURES);
        assert_eq!(
            f.clipboard.lock().unwrap().sequence as usize,
            crate::session::FREE_SINGLE_DISCLOSURES,
            "writes stop at the budget"
        );
        assert_eq!(
            host.shown().len(),
            40 - crate::session::FREE_SINGLE_DISCLOSURES
        );
    }

    #[test]
    fn fr03_the_budget_is_a_rolling_minute_on_the_clock() {
        let f = disclosing();
        let host = ScriptedConfirm::new(false);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            assert!(reveal_one(&f, &host, KeyDisclosure::Reveal)
                .unwrap()
                .is_some());
            f.clock
                .set(f.clock.0.load(std::sync::atomic::Ordering::SeqCst) + 1);
        }
        assert!(reveal_one(&f, &host, KeyDisclosure::Reveal)
            .unwrap()
            .is_none());

        // The oldest request is 60 s old now: one slot frees, and only one.
        f.clock.set(1_000 + 60);
        assert!(reveal_one(&f, &host, KeyDisclosure::Reveal)
            .unwrap()
            .is_some());
        assert!(reveal_one(&f, &host, KeyDisclosure::Reveal)
            .unwrap()
            .is_none());

        f.clock.set(1_000 + 200);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            assert!(reveal_one(&f, &host, KeyDisclosure::Reveal)
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn fr03_an_approved_single_key_counts_against_the_budget_and_a_new_session_does_not_reset_it() {
        let f = disclosing();
        let yes = ScriptedConfirm::new(true);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            reveal_one(&f, &yes, KeyDisclosure::Reveal)
                .unwrap()
                .unwrap();
        }
        assert!(yes.shown().is_empty());
        reveal_one(&f, &yes, KeyDisclosure::Reveal)
            .unwrap()
            .unwrap();
        assert_eq!(yes.shown().len(), 1, "the sixth asked");
        reveal_one(&f, &yes, KeyDisclosure::Reveal)
            .unwrap()
            .unwrap();
        assert_eq!(
            yes.shown().len(),
            2,
            "an approval buys one request, not a pass"
        );

        // A page that knows the password locks and unlocks, in the same minute.
        f.session.lock().unwrap();
        f.session.observe(crate::policy::Signal::Available);
        let ticket = f.session.begin_open().unwrap();
        f.session
            .open(ticket, || {
                Vault::open(
                    f._directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
        let no = ScriptedConfirm::new(false);
        let id = f.keys[0].clone();
        let single = |host: &ScriptedConfirm| {
            single_disclosure(
                &f.session,
                host,
                &owner(&f.session),
                &id,
                KeyDisclosure::Reveal,
            )
            .unwrap()
        };
        assert!(single(&no).is_none(), "the new session asks too");
        assert_eq!(no.shown().len(), 1);

        // The budget frees itself only with time.
        f.clock.set(1_061);
        let free = ScriptedConfirm::new(false);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            assert!(single(&free).is_some());
        }
        assert!(
            free.shown().is_empty(),
            "a minute later the budget is free again"
        );
    }

    #[test]
    fn fr03_a_lock_while_a_single_key_window_is_open_discloses_nothing() {
        let f = disclosing();
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            reveal_one(&f, &ScriptedConfirm::new(true), KeyDisclosure::Copy)
                .unwrap()
                .unwrap();
        }
        let session = f.session.clone();
        let locking = ScriptedConfirm::during(true, move || session.lock().unwrap());

        let error = reveal_one(&f, &locking, KeyDisclosure::Copy).err().unwrap();

        assert_eq!(error.kind, "locked");
    }

    #[test]
    fn fr03_every_disclosing_command_goes_through_the_native_approval() {
        let source = include_str!("commands.rs");
        let code = &source[..source.find("\nmod tests {").unwrap()];
        for (command, helper) in [
            ("key_reveal", "single_disclosure("),
            ("key_copy", "single_disclosure("),
            ("env_export", "bulk_disclosure("),
            ("env_copy", "bulk_disclosure("),
        ] {
            let start = code
                .find(&format!("pub async fn {command}("))
                .unwrap_or_else(|| panic!("{command} is a command"));
            let body = &code[start..];
            let body = &body[..body.find("\n}\n").unwrap()];
            assert!(body.contains(helper), "{command} calls {helper}");
            assert!(
                body.contains("confirm: State<'_, ConfirmState>"),
                "{command} takes the native window"
            );
            assert!(
                !body.contains("snapshot_for"),
                "{command} takes no snapshot of its own: the helper binds the read to the approval"
            );
        }
    }

    #[test]
    fn fr03_an_approved_request_fills_the_rolling_minute_too() {
        let f = disclosing();
        let yes = ScriptedConfirm::new(true);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES {
            reveal_one(&f, &yes, KeyDisclosure::Reveal)
                .unwrap()
                .unwrap();
        }
        f.clock.set(1_050);
        reveal_one(&f, &yes, KeyDisclosure::Reveal)
            .unwrap()
            .unwrap();
        assert_eq!(yes.shown().len(), 1, "the sixth asked and was approved");

        // The five free requests are 61 s old. The approved one is 11 s old and still counts.
        f.clock.set(1_061);
        let no = ScriptedConfirm::new(false);
        for _ in 0..crate::session::FREE_SINGLE_DISCLOSURES - 1 {
            assert!(reveal_one(&f, &no, KeyDisclosure::Reveal)
                .unwrap()
                .is_some());
        }
        assert!(no.shown().is_empty());
        assert!(reveal_one(&f, &no, KeyDisclosure::Reveal)
            .unwrap()
            .is_none());
    }

    // ---- FR-14: a new journal after damage, with the user's approval ----

    /// Appends a line that is no record to the journal of the fixture. Returns its path and bytes.
    fn damage_journal(f: &Disclosing) -> (PathBuf, Vec<u8>) {
        let path = f
            .session
            .access(Access::Read, None, |vault| {
                Ok(vault.audit_log().journal_path()?)
            })
            .unwrap()
            .value
            .expect("the added keys made a journal");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut file, b"{this is not a record}\n").unwrap();
        drop(file);
        let bytes = fs::read(&path).unwrap();
        (path, bytes)
    }

    fn recover(
        f: &Disclosing,
        host: &ScriptedConfirm,
    ) -> Result<Option<JournalRecoveryView>, CommandError> {
        run_recovery(&f.session, host, &owner(&f.session))
    }

    /// The files of this journal only: the journal, its lock, and any kept or pending file with its
    /// name. The journal folder is shared by every test, so other journals are not listed.
    fn folder_files(path: &std::path::Path) -> Vec<String> {
        let own = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut names: Vec<String> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&own))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn fr14_the_reviewers_probe_through_the_handler_damage_refuse_recover_reveal_old_bytes_kept() {
        let f = disclosing();
        let yes = ScriptedConfirm::new(true);
        assert!(reveal_one(&f, &yes, KeyDisclosure::Reveal)
            .unwrap()
            .is_some());
        let (journal, damaged) = damage_journal(&f);

        let refused = reveal_one(&f, &yes, KeyDisclosure::Reveal).err().unwrap();
        assert_eq!(
            refused.kind, "journal",
            "disclosure is blocked while the journal is damaged"
        );
        assert_eq!(fs::read(&journal).unwrap(), damaged);

        let kept = recover(&f, &yes).unwrap().expect("approved");

        let kept_path = journal.with_file_name(&kept.kept_name);
        assert_eq!(
            fs::read(&kept_path).unwrap(),
            damaged,
            "the old bytes are kept as they were"
        );
        assert_eq!(kept.bytes, damaged.len() as u64);
        assert_eq!(kept.sha256.len(), 64);
        let window = yes.shown().last().unwrap().clone();
        assert!(
            window.contains(journal.file_name().unwrap().to_str().unwrap()),
            "{window}"
        );
        assert!(window.contains("cannot be proved complete"), "{window}");
        assert!(window.contains("stay blocked"), "{window}");
        assert!(
            reveal_one(&f, &yes, KeyDisclosure::Reveal)
                .unwrap()
                .is_some(),
            "disclosure works after the recovery"
        );
        let log = f
            .session
            .access(Access::Read, None, |vault| {
                Ok(vault.audit_log().read(50)?)
            })
            .unwrap()
            .value;
        assert_eq!(log[0].action, "journal-recovered");
        assert!(log[0].detail.as_deref().unwrap().contains(&kept.sha256));
    }

    #[test]
    fn fr14_a_decline_moves_nothing_and_disclosure_stays_blocked() {
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let before = folder_files(&journal);

        let answer = recover(&f, &ScriptedConfirm::new(false)).unwrap();

        assert!(answer.is_none());
        assert_eq!(fs::read(&journal).unwrap(), damaged);
        assert_eq!(folder_files(&journal), before, "no kept file, no new file");
        let blocked = reveal_one(&f, &ScriptedConfirm::new(true), KeyDisclosure::Reveal)
            .err()
            .unwrap();
        assert_eq!(blocked.kind, "journal");
    }

    #[test]
    fn fr14_a_journal_that_is_not_damaged_is_refused_before_any_window() {
        let f = disclosing();
        let host = ScriptedConfirm::new(true);
        let journal = f
            .session
            .access(Access::Read, None, |vault| {
                Ok(vault.audit_log().journal_path()?)
            })
            .unwrap()
            .value
            .unwrap();
        let before = fs::read(&journal).unwrap();

        let error = recover(&f, &host).unwrap_err();

        assert_eq!(error.kind, "journal");
        assert!(host.shown().is_empty(), "no window for an intact journal");
        assert_eq!(fs::read(&journal).unwrap(), before);
    }

    #[test]
    fn fr14_a_window_that_fails_is_an_error_and_moves_nothing() {
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let before = folder_files(&journal);
        let broken = ScriptedConfirm::new(true).failing(crate::review_window::ReviewFailure::new(
            "the window code stopped",
        ));

        let error = recover(&f, &broken).unwrap_err();

        assert_eq!(error.kind, "reviewFailed");
        assert_eq!(fs::read(&journal).unwrap(), damaged);
        assert_eq!(folder_files(&journal), before);
    }

    #[test]
    fn fr14_a_lock_while_the_window_is_open_moves_nothing() {
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let before = folder_files(&journal);
        let session = f.session.clone();
        let locking = ScriptedConfirm::during(true, move || session.lock().unwrap());

        let error = recover(&f, &locking).unwrap_err();

        assert_eq!(error.kind, "locked");
        assert_eq!(fs::read(&journal).unwrap(), damaged);
        assert_eq!(
            folder_files(&journal),
            before,
            "nothing was kept, nothing was started"
        );
    }

    #[test]
    fn fr14_a_lock_after_the_journal_moved_answers_completed_while_locked() {
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let own = journal.file_name().unwrap().to_string_lossy().into_owned();
        let session = f.session.clone();
        let locking = ScriptedConfirm::new(true).on_journal_moved(move || {
            session.lock().unwrap();
        });

        let error = recover(&f, &locking).unwrap_err();

        assert_eq!(
            error.kind, "completedWhileLocked",
            "the journal moved, so the answer must not say that nothing changed"
        );
        let kept = folder_files(&journal)
            .into_iter()
            .filter(|name| name != &own)
            .any(|name| fs::read(journal.with_file_name(name)).ok().as_ref() == Some(&damaged));
        assert!(kept, "the old bytes are kept under a dated name");
    }

    #[test]
    fn fr14_a_hide_while_the_window_is_open_does_not_revoke_what_the_user_approved() {
        // As for an import (`Session::access_reviewed`): the user clicked in a native window,
        // and the recovery discloses nothing to the page. A lock and a change do revoke it.
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let session = f.session.clone();
        let hiding = ScriptedConfirm::during(true, move || {
            session.expire_presentation().unwrap();
        });

        let kept = recover(&f, &hiding).unwrap().expect("approved");

        assert_eq!(
            fs::read(journal.with_file_name(kept.kept_name)).unwrap(),
            damaged
        );
        assert!(
            reveal_one(&f, &ScriptedConfirm::new(true), KeyDisclosure::Reveal)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn fr14_a_hidden_window_shows_no_dialog_and_moves_nothing() {
        let f = disclosing();
        let (journal, damaged) = damage_journal(&f);
        let hidden = ScriptedConfirm::new(true);
        hidden
            .window
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let error = recover(&f, &hidden).unwrap_err();

        assert_eq!(error.kind, "viewChanged");
        assert!(hidden.shown().is_empty());
        assert_eq!(fs::read(&journal).unwrap(), damaged);
    }

    #[test]
    fn fr14_the_command_is_registered_and_takes_the_session_and_the_native_window() {
        let source = include_str!("commands.rs");
        let code = &source[..source.find("\nmod tests {").unwrap()];
        let start = code.find("pub async fn audit_recover(").expect("a command");
        let body = &code[start..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("confirm: State<'_, ConfirmState>"));
        assert!(body.contains("session_id: String"));
        assert!(body.contains("run_recovery("));
        assert!(
            !body.contains("recover_damaged"),
            "the command asks first, in run_recovery"
        );
        let run = &code[code.find("fn run_recovery(").unwrap()..];
        let run = &run[..run.find("\n}\n").unwrap()];
        assert!(
            run.find("disclosure::ask(").unwrap() < run.find("recover_damaged()").unwrap(),
            "the user is asked before anything moves"
        );
    }
}
