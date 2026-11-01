use std::{fs, path::PathBuf, sync::Arc};

use akm_core::{audit::AuditLog, dotenv, Vault};
use chrono::{DateTime, Utc};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    clipboard::CLEAR_AFTER,
    error::CommandError,
    private_export,
    session::{Access, Session, Snapshot, Stamped, VaultState},
    settings::{current_path, settings_file, write_settings, Settings},
    views::*,
};

#[tauri::command]
pub fn vault_status(app: AppHandle, state: State<'_, VaultState>) -> Result<Status, CommandError> {
    match state
        .0
        .access(Access::Read, None, |vault| Ok(status_for(vault)))
    {
        Ok(status) => Ok(status.value),
        Err(error) if error.kind == "locked" => {
            let path = current_path(&app, &state.0)?;
            Ok(Status {
                path: path.display().to_string(),
                exists: path.exists(),
                unlocked: false,
                key_count: 0,
                vault_name: None,
                needs_upgrade: false,
            })
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub async fn vault_unlock(
    app: AppHandle,
    state: State<'_, VaultState>,
    path: String,
    password: String,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err(CommandError::new("invalid", "Enter the master password."));
    }
    open_vault(app, state.0.clone(), path.into(), password, None, false).await
}

#[tauri::command]
pub async fn vault_create(
    app: AppHandle,
    state: State<'_, VaultState>,
    path: String,
    password: String,
    name: Option<String>,
) -> Result<Status, CommandError> {
    let password = Zeroizing::new(password);
    if password.chars().count() < 12 {
        return Err(CommandError::new(
            "invalid",
            "Use at least 12 characters for the master password.",
        ));
    }
    open_vault(
        app,
        state.0.clone(),
        path.into(),
        password,
        clean(name),
        true,
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
) -> Result<Status, CommandError> {
    let ticket = session.begin_open()?;
    let completion = if create { Access::Write } else { Access::Read };
    let worker = session.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let load = || {
            if create {
                Vault::create(&target, &password, name.as_deref()).map_err(Into::into)
            } else {
                Vault::open(&target, &password).map_err(Into::into)
            }
        };
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
    if let Err(error) = write_settings(
        &app,
        &Settings {
            vault_path: Some(opened.value.path.clone()),
        },
    ) {
        session.warn(format!(
            "The vault opened, but its path preference was not saved. {}",
            error.message
        ));
    }
    session.finish(completion, opened)
}

#[tauri::command]
pub fn vault_lock(state: State<'_, VaultState>) -> Result<(), CommandError> {
    state.0.lock()
}

#[tauri::command]
pub fn vault_activity(state: State<'_, VaultState>) -> Result<(), CommandError> {
    state.0.activity()
}

#[tauri::command]
pub fn vault_warnings(state: State<'_, VaultState>) -> Result<Vec<String>, CommandError> {
    state.0.warnings()
}

#[tauri::command]
pub async fn vault_upgrade(state: State<'_, VaultState>) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Upgrade, |vault| {
        vault.upgrade()?;
        Ok(status_for(vault))
    })
    .await
}

#[tauri::command]
pub async fn vault_reload(state: State<'_, VaultState>) -> Result<Status, CommandError> {
    let session = state.0.clone();
    let expected = session.snapshot()?;
    run_session(session, expected, Access::Reload, |vault| {
        vault.reload()?;
        Ok(status_for(vault))
    })
    .await
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

#[tauri::command]
pub fn keys_list(state: State<'_, VaultState>) -> Result<Vec<KeyView>, CommandError> {
    state
        .0
        .access(Access::Read, None, |vault| {
            let mut keys = vault.list();
            let views = keys.iter().map(KeyView::from).collect();
            for key in &mut keys {
                key.secret.zeroize();
            }
            Ok(views)
        })
        .map(|s| s.value)
}

#[tauri::command]
pub fn key_reveal(state: State<'_, VaultState>, id: String) -> Result<String, CommandError> {
    let secret = state.0.access(Access::Read, None, |vault| {
        let mut key = vault.get_by_id(&id)?;
        let secret = Zeroizing::new(std::mem::take(&mut key.secret));
        vault.log("reveal", Some(&key), None)?;
        Ok(secret)
    })?;
    state.0.reveal(secret)
}

#[tauri::command]
pub fn key_copy(state: State<'_, VaultState>, id: String) -> Result<u64, CommandError> {
    let secret = state.0.access(Access::Read, None, |vault| {
        let mut key = vault.get_by_id(&id)?;
        let secret = Zeroizing::new(std::mem::take(&mut key.secret));
        vault.log("copy", Some(&key), None)?;
        Ok(secret)
    })?;
    state.0.copy(secret.snapshot.ticket, secret.value)?;
    Ok(CLEAR_AFTER.as_secs())
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
pub fn audit_read(
    state: State<'_, VaultState>,
    limit: Option<usize>,
) -> Result<AuditReport, CommandError> {
    state
        .0
        .access(Access::Read, None, |vault| {
            let log = AuditLog::for_vault(vault.path());
            let broken_at = log.verify()?;
            let entries = log
                .read(limit.unwrap_or(500))?
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
                broken_at,
            })
        })
        .map(|s| s.value)
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

fn extract_export(
    session: &Session,
    scope: &ExportScope,
    target: &str,
) -> Result<Stamped<Zeroizing<String>>, CommandError> {
    session.access(Access::Read, None, |vault| {
        let (body, count) = export_body(vault, scope)?;
        vault.log("export", None, Some(&format!("{count} keys to {target}")))?;
        Ok(body)
    })
}

#[tauri::command]
pub fn env_export(
    state: State<'_, VaultState>,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<String, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let exported = extract_export(&state.0, &scope, ".env preview")?;
    state.0.reveal(exported)
}

#[tauri::command]
pub fn env_copy(
    state: State<'_, VaultState>,
    project: Option<String>,
    environment: Option<String>,
    all: bool,
) -> Result<u64, CommandError> {
    let scope = ExportScope::new(project, environment, all)?;
    let exported = extract_export(&state.0, &scope, "clipboard")?;
    if exported.value.is_empty() {
        return Err(CommandError::new(
            "invalid",
            "No keys match that export scope.",
        ));
    }
    state.0.copy(exported.snapshot.ticket, exported.value)?;
    Ok(CLEAR_AFTER.as_secs())
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
        let exported = session.access(Access::Read, Some(expected), |vault| {
            let (body, count) = export_body(vault, &scope)?;
            if count == 0 {
                return Err(CommandError::new(
                    "invalid",
                    "No keys match that export scope.",
                ));
            }
            Ok((body, count, vault.path().to_path_buf()))
        })?;
        let (body, count, vault_path) = exported.value;
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
            |prepared| prepared.publish(),
        )?;
        if let Err(error) = session.access(Access::Read, Some(expected), |vault| {
            vault
                .log(
                    "export",
                    None,
                    Some(&format!("{count} keys to {}", saved.display())),
                )
                .map_err(Into::into)
        }) {
            session.warn(format!(
                "The environment file was saved, but its audit entry was not recorded. {}",
                error.message
            ));
        }
        Ok(Some(saved.display().to_string()))
    })
    .await
    .map_err(CommandError::internal)?
}

#[tauri::command]
pub fn env_import_preview(
    state: State<'_, VaultState>,
    text: String,
    project: Option<String>,
    environment: Option<String>,
) -> Result<PreviewReport, CommandError> {
    let text = Zeroizing::new(text);
    prepare_import(&state.0, text, project, environment)
}

fn prepare_import(
    session: &Session,
    text: Zeroizing<String>,
    project: Option<String>,
    environment: Option<String>,
) -> Result<PreviewReport, CommandError> {
    let project = clean(project);
    let environment = clean(environment);
    let entries = SecretEntries(dotenv::parse(&text)?);
    let preview = session.access(Access::Read, None, |vault| {
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
pub fn backup_info(state: State<'_, VaultState>) -> Result<BackupInfo, CommandError> {
    state
        .0
        .access(Access::Read, None, |vault| {
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
        .map(|s| s.value)
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
            Arc::new(|| {}),
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
