use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use akm_core::{audit::AuditLog, dotenv, Vault};
use chrono::{DateTime, Utc};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    clipboard::CLEAR_AFTER,
    error::CommandError,
    private_export,
    session::{
        Access, OperationWarningKind, Session, Snapshot, Stamped, VaultState, WarningReport,
    },
    settings::{current_path, settings_file, write_settings, Settings},
    views::*,
};

#[tauri::command]
pub async fn vault_status<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, VaultState>,
) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot();
    blocking(move || match expected {
        Ok(expected) => {
            let status =
                session.access(Access::Read, Some(expected), |vault| Ok(status_for(vault)))?;
            finish_status(&session, Access::Read, status)
        }
        Err(error) if error.kind == "locked" => {
            let path = current_path(&app, &session)?;
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
    app: AppHandle,
    state: State<'_, VaultState>,
    path: String,
    password: String,
    session_id: String,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    open_vault(
        app,
        state.0.clone(),
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
    app: AppHandle,
    state: State<'_, VaultState>,
    path: String,
    password: String,
    name: Option<String>,
    session_id: String,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    open_vault(
        app,
        state.0.clone(),
        path.into(),
        password,
        clean(name),
        true,
        &session_id,
    )
    .await
}

async fn open_vault(
    app: AppHandle,
    session: Arc<Session>,
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
    if write_settings(
        &app,
        &Settings {
            vault_path: Some(opened.value.path.clone()),
        },
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
pub fn vault_activity(state: State<'_, VaultState>) -> Result<(), CommandError> {
    state.0.activity()
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
pub async fn vault_upgrade(state: State<'_, VaultState>) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
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
pub async fn vault_reload(state: State<'_, VaultState>) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
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
pub async fn keys_list(state: State<'_, VaultState>) -> Result<Vec<KeyView>, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Read, |vault| {
        let mut keys = vault.list();
        let views = keys.iter().map(KeyView::from).collect();
        for key in &mut keys {
            key.secret.zeroize();
        }
        Ok(views)
    })
    .await
}

#[tauri::command]
pub async fn key_reveal(state: State<'_, VaultState>, id: String) -> Result<String, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    blocking(move || {
        let secret = requested_key_at(&session, expected, &id, "reveal-requested")?;
        session.reveal(secret)
    })
    .await
}

#[cfg(test)]
fn requested_key(
    session: &Session,
    id: &str,
    action: &str,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    requested_key_at(session, session.snapshot()?, id, action)
}

fn requested_key_at(
    session: &Session,
    expected: Snapshot,
    id: &str,
    action: &str,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    session.access(Access::Read, Some(expected), |vault| {
        let mut key = vault.get_by_id(id)?;
        let secret = Zeroizing::new(std::mem::take(&mut key.secret));
        vault.log(action, Some(&key), None)?;
        Ok(secret)
    })
}

#[tauri::command]
pub async fn key_copy(state: State<'_, VaultState>, id: String) -> Result<u64, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    blocking(move || {
        let secret = requested_key_at(&session, expected, &id, "copy-requested")?;
        session.copy_stamped(secret)?;
        Ok(CLEAR_AFTER.as_secs())
    })
    .await
}

#[tauri::command]
pub async fn key_add(
    state: State<'_, VaultState>,
    draft: KeyDraft,
) -> Result<KeyView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Write, move |vault| {
        let mut key = vault.add(draft.into_input(None)?)?;
        let view = KeyView::from(&key);
        key.secret.zeroize();
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn key_update(
    state: State<'_, VaultState>,
    id: String,
    draft: KeyDraft,
) -> Result<KeyView, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Write, move |vault| {
        let existing = vault.get_by_id(&id)?;
        let mut key = vault.update(&id, draft.into_input(Some(existing))?)?;
        let view = KeyView::from(&key);
        key.secret.zeroize();
        Ok(view)
    })
    .await
}

#[tauri::command]
pub async fn key_remove(state: State<'_, VaultState>, id: String) -> Result<(), CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Write, move |vault| {
        let mut removed = vault.remove(&id)?;
        removed.secret.zeroize();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn audit_read(
    state: State<'_, VaultState>,
    limit: Option<usize>,
) -> Result<AuditReport, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Read, move |vault| {
        let log = AuditLog::for_vault(vault.path());
        let snapshot = log.snapshot(limit.unwrap_or(500))?;
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| AuditView {
                seq: entry.seq,
                ts: entry.ts,
                action: entry.action,
                key_title: entry.key_title,
                project: entry.project,
                environment: entry.environment,
                detail: entry.detail,
            })
            .collect();
        Ok(AuditReport {
            path: log.path().display().to_string(),
            entries,
            broken_at: snapshot.broken_at,
            state: snapshot.state,
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
) -> Result<(Zeroizing<String>, usize), CommandError> {
    let vars = SecretEntries(vault.env_vars(
        scope.project.as_deref(),
        scope.environment.as_deref(),
        scope.all,
    )?);
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
        let (body, count) = export_body(vault, scope)?;
        vault.log(
            "export-requested",
            None,
            Some(&format!("{count} keys to {target}")),
        )?;
        Ok(body)
    })
}

#[tauri::command]
pub async fn env_export(
    state: State<'_, VaultState>,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<String, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let expected = session.snapshot()?;
    blocking(move || {
        let exported = extract_export_at(&session, expected, &scope, ".env preview")?;
        session.reveal(exported)
    })
    .await
}

#[tauri::command]
pub async fn env_copy(
    state: State<'_, VaultState>,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<u64, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let expected = session.snapshot()?;
    blocking(move || {
        let exported = extract_export_at(&session, expected, &scope, "clipboard")?;
        if exported.value.is_empty() {
            return Err(CommandError::new(
                "invalid",
                "No keys match that export scope.",
            ));
        }
        session.copy_stamped(exported)?;
        Ok(CLEAR_AFTER.as_secs())
    })
    .await
}

#[tauri::command]
pub async fn env_save(
    app: AppHandle,
    state: State<'_, VaultState>,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<Option<String>, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let session = state.0.clone();
    let expected = session.snapshot()?;
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
        } = requested_file_export_at(&session, expected, &scope)?;
        let journal = AuditLog::for_vault(&vault_path);
        let settings = settings_file(&app)?;
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(CommandError::internal)?;
        let saved = session.publish(
            expected,
            || {
                private_export::prepare_private(
                    &target,
                    &body,
                    &[vault_path, settings],
                    &[config_dir],
                )
            },
            |prepared| {
                publish_and_record(&session, expected, prepared, |saved| {
                    journal.append(
                        "export-completed",
                        None,
                        None,
                        None,
                        None,
                        Some(&format!("{count} keys to {}", saved.display())),
                    )
                })
            },
        )?;
        Ok(Some(saved.display().to_string()))
    })
    .await
    .map_err(CommandError::internal)?
}

struct FileExport {
    body: Zeroizing<String>,
    count: usize,
    vault_path: PathBuf,
}

fn requested_file_export_at(
    session: &Session,
    expected: Snapshot,
    scope: &ExportScope,
) -> Result<FileExport, CommandError> {
    session
        .read_for_publication(expected, |vault| {
            let (body, count) = export_body(vault, scope)?;
            if count == 0 {
                return Err(CommandError::new(
                    "invalid",
                    "No keys match that export scope.",
                ));
            }
            vault.log(
                "export-requested",
                None,
                Some(&format!("file export requested; {count} keys")),
            )?;
            Ok(FileExport {
                body,
                count,
                vault_path: vault.path().to_path_buf(),
            })
        })
        .map(|exported| exported.value)
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

#[tauri::command]
pub async fn env_import_preview(
    state: State<'_, VaultState>,
    text: String,
    project: Option<String>,
    environment: Option<String>,
) -> Result<PreviewReport, CommandError> {
    let text = Zeroizing::new(text);
    let session = state.0.clone();
    let expected = session.snapshot()?;
    blocking(move || prepare_import_at(&session, expected, text, project, environment)).await
}

#[cfg(test)]
fn prepare_import(
    session: &Session,
    text: Zeroizing<String>,
    project: Option<String>,
    environment: Option<String>,
) -> Result<PreviewReport, CommandError> {
    prepare_import_at(session, session.snapshot()?, text, project, environment)
}

fn prepare_import_at(
    session: &Session,
    expected: Snapshot,
    text: Zeroizing<String>,
    project: Option<String>,
    environment: Option<String>,
) -> Result<PreviewReport, CommandError> {
    let project = clean(project);
    let environment = clean(environment);
    let entries = SecretEntries(dotenv::parse(&text)?);
    let preview = session.access(Access::Read, Some(expected), |vault| {
        vault
            .preview_import_env(&entries.0, project.as_deref(), environment.as_deref())
            .map_err(Into::into)
    })?;
    let token = session.store_preview(preview.snapshot, text, project, environment)?;
    Ok(PreviewReport {
        preview: preview.value,
        token,
    })
}

#[tauri::command]
pub async fn env_import_confirm(
    state: State<'_, VaultState>,
    token: String,
) -> Result<ImportReport, CommandError> {
    let session = state.0.clone();
    let pending = session.take_preview(&token)?;
    let snapshot = pending.snapshot;
    let worker = session.clone();
    let report = tauri::async_runtime::spawn_blocking(move || confirm_import(&worker, pending))
        .await
        .map_err(CommandError::internal)??;
    session.finish(
        Access::Write,
        Stamped {
            snapshot,
            value: report,
        },
    )
}

fn confirm_import(
    session: &Session,
    pending: Stamped<crate::pending::PendingImport>,
) -> Result<ImportReport, CommandError> {
    let input = pending.value;
    let entries = SecretEntries(dotenv::parse(&input.text)?);
    session
        .access(Access::Write, Some(pending.snapshot), |vault| {
            let (added, updated) = vault.import_env(
                &entries.0,
                input.project.as_deref(),
                input.environment.as_deref(),
            )?;
            Ok(ImportReport { added, updated })
        })
        .map(|s| s.value)
}

#[tauri::command]
pub async fn backup_info(state: State<'_, VaultState>) -> Result<BackupInfo, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
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

    fn test_session() -> (Session, tempfile::TempDir) {
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
        let requested = requested_key(&session, &key.id, "reveal-requested").unwrap();
        session.lock().unwrap();
        assert_eq!(session.reveal(requested).unwrap_err().kind, "locked");
        let log = AuditLog::for_vault(&directory.path().join("vault.kdbx"));
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
        let requested = requested_key(&session, &key.id, "copy-requested").unwrap();
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
        let requested = requested_key(&session, &key.id, "copy-requested").unwrap();
        assert_eq!(
            session
                .copy(requested.snapshot.ticket, requested.value)
                .unwrap_err()
                .kind,
            "clipboard"
        );
        let log = AuditLog::for_vault(&path);
        assert_eq!(log.read(1).unwrap()[0].action, "copy-requested");
        std::fs::remove_file(log.path()).unwrap();
        std::fs::create_dir(log.path()).unwrap();
        assert!(requested_key(&session, &key.id, "reveal-requested").is_err());
        assert!(requested_key(&session, &key.id, "copy-requested").is_err());
    }

    #[test]
    fn import_preview_is_secret_free_scoped_and_single_use() {
        let (session, _directory) = test_session();
        let preview = prepare_import(
            &session,
            Zeroizing::new("TOKEN=fake-dev\n".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .unwrap();
        let json = serde_json::to_value(&preview).unwrap();
        assert_eq!(json["added"], 1);
        assert_eq!(json["updated"], 0);
        assert!(!json.to_string().contains("fake-dev"));
        let pending = session.take_preview(&preview.token).unwrap();
        let result = confirm_import(&session, pending).unwrap();
        assert_eq!((result.added, result.updated), (1, 0));
        assert!(session.take_preview(&preview.token).is_err());

        let prod = prepare_import(
            &session,
            Zeroizing::new("TOKEN=fake-prod\n".into()),
            Some("app".into()),
            Some("prod".into()),
        )
        .unwrap();
        let result = confirm_import(&session, session.take_preview(&prod.token).unwrap()).unwrap();
        assert_eq!((result.added, result.updated), (1, 0));
        let dev_scope = ExportScope::new(Some("app".into()), Some("dev".into()), false).unwrap();
        let body = extract_export(&session, &dev_scope, "test").unwrap();
        assert_eq!(body.value.as_str(), "TOKEN=fake-dev\n");
        assert!(extract_export(
            &session,
            &ExportScope::new(None, None, true).unwrap(),
            "test"
        )
        .is_err());
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
        let log = AuditLog::for_vault(&directory.path().join("vault.kdbx"));
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
            for action in ["reveal-requested", "copy-requested"] {
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
                        let mut key = vault.add(input)?;
                        key.secret.zeroize();
                    }
                    Ok(())
                })
                .unwrap();
            let expected = session.snapshot().unwrap();
            session.expire_presentation().unwrap();
            let journal = AuditLog::for_vault(&directory.path().join("vault.kdbx"));
            match change {
                "lock" => session.lock().unwrap(),
                "revision" => {
                    session.access(Access::Write, None, |_| Ok(())).unwrap();
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
            let session = Arc::new(session);
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
            let log = AuditLog::for_vault(&path);
            let before = fs::read(log.path()).unwrap();
            // Make valid completion bytes with the real encoder, then inject failure immediately
            // after writing those bytes to the actual test journal, before syncing that handle.
            let oracle = AuditLog::for_vault(&directory.path().join("encoder-only.kdbx"));
            fs::write(oracle.path(), &before).unwrap();
            oracle
                .append("export-completed", None, None, None, None, None)
                .unwrap();
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
                        )
                    },
                    |prepared| {
                        publish_and_record(&worker_session, expected, prepared, |_| {
                            let log = AuditLog::for_vault(&path);
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
    fn invalid_imports_and_preview_confirm_after_an_intervening_write_are_rejected() {
        let (session, _directory) = test_session();
        for text in ["TOKEN=", "TOKEN=fake\nTOKEN=other", "PATH=fake"] {
            assert!(prepare_import(
                &session,
                Zeroizing::new(text.into()),
                Some("app".into()),
                Some("dev".into())
            )
            .is_err());
        }
        assert!(prepare_import(
            &session,
            Zeroizing::new("TOKEN=fake".into()),
            None,
            Some("dev".into())
        )
        .is_err());
        let preview = prepare_import(
            &session,
            Zeroizing::new("TOKEN=fake".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .unwrap();
        let consumed = session.take_preview(&preview.token).unwrap();
        session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput::new("Other", "other-fake"))?;
                Ok(())
            })
            .unwrap();
        assert!(confirm_import(&session, consumed).is_err());
        let scoped = ExportScope::new(Some("app".into()), Some("dev".into()), false).unwrap();
        assert_eq!(
            extract_export(&session, &scoped, "test")
                .unwrap()
                .value
                .as_str(),
            ""
        );
    }

    #[test]
    fn committed_import_keeps_its_receipt_when_lock_arrives_during_journal_append() {
        use std::time::{Duration, Instant};
        let (session, directory) = test_session();
        let session = Arc::new(session);
        let path = directory.path().join("vault.kdbx");
        let preview = prepare_import(
            &session,
            Zeroizing::new("TOKEN=fake-saved-once\n".into()),
            Some("app".into()),
            Some("dev".into()),
        )
        .unwrap();
        let pending = session.take_preview(&preview.token).unwrap();
        let initial = fs::read(&path).unwrap();
        let journal = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.path().join("vault.kdbx.audit.jsonl.lock"))
            .unwrap();
        journal.lock().unwrap();
        let worker = session.clone();
        let writer = std::thread::spawn(move || confirm_import(&worker, pending));
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
        let reopened = Vault::open(path, "fake-test-password-123!").unwrap();
        assert_eq!(reopened.list().len(), 1);
        assert_eq!(
            reopened.env_vars(Some("app"), Some("dev"), false).unwrap(),
            [("TOKEN".into(), "fake-saved-once".into())]
        );
    }

    #[test]
    fn export_uses_core_validation_not_only_native_or_frontend_scope_checks() {
        let (session, _directory) = test_session();
        session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput {
                    title: "Process control".into(),
                    secret: "fake".into(),
                    env_var: Some("PATH".into()),
                    project: Some("app".into()),
                    environment: Some("dev".into()),
                    ..Default::default()
                })?;
                Ok(())
            })
            .unwrap();
        let scope = ExportScope::new(Some("app".into()), Some("dev".into()), false).unwrap();
        assert!(extract_export(&session, &scope, "test").is_err());
    }
}
