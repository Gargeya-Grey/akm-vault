use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager};

use crate::{error::CommandError, policy::Ticket, session::Held};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub vault_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreferenceSave {
    Saved,
    Superseded,
}

/// Runs the effect only while the opening that asks still owns the session, and holds the
/// session gate (the one that lock and a newer opening take) while it runs.
pub type OwnerFence<'a> = &'a dyn Fn(&mut dyn FnMut(&Held<'_>));

/// Where the last opened vault path lives. A newer opening always wins over an older one.
pub trait PreferenceStore: Send + Sync {
    fn saved_vault_path(&self) -> Option<String>;
    fn default_vault_path(&self) -> Result<PathBuf, CommandError>;
    /// The write happens inside `owned`, so the ownership check and the write are one step.
    fn save_vault_path(
        &self,
        ticket: Ticket,
        owned: OwnerFence<'_>,
        vault_path: &str,
    ) -> Result<PreferenceSave, CommandError>;
}

pub struct PreferenceState(pub Arc<dyn PreferenceStore>);

pub struct FilePreferences {
    file: Option<PathBuf>,
    default_vault: Option<PathBuf>,
    newest: Mutex<u64>,
}

impl FilePreferences {
    pub fn new(file: Option<PathBuf>, default_vault: Option<PathBuf>) -> Self {
        Self {
            file,
            default_vault,
            newest: Mutex::new(0),
        }
    }

    pub fn for_app<R: tauri::Runtime>(app: &AppHandle<R>) -> Self {
        Self::new(
            settings_file(app).ok(),
            app.path()
                .data_dir()
                .ok()
                .map(|base| base.join("akm").join("vault.kdbx")),
        )
    }
}

impl PreferenceStore for FilePreferences {
    fn saved_vault_path(&self) -> Option<String> {
        let raw = fs::read_to_string(self.file.as_ref()?).ok()?;
        serde_json::from_str::<Settings>(&raw).ok()?.vault_path
    }

    fn default_vault_path(&self) -> Result<PathBuf, CommandError> {
        self.default_vault.clone().ok_or_else(|| {
            CommandError::new("config", "The default vault location is not available.")
        })
    }

    fn save_vault_path(
        &self,
        ticket: Ticket,
        owned: OwnerFence<'_>,
        vault_path: &str,
    ) -> Result<PreferenceSave, CommandError> {
        let file = self.file.as_ref().ok_or_else(|| {
            CommandError::new("config", "The settings location is not available.")
        })?;
        // A fencing token: tickets only grow, and a newer opening's write is final.
        // Lock order is this lock, then the session gate. The ownership check and the
        // write are one step under that gate, so a newer opening can neither retire the
        // owner between them nor write before this write is done.
        let mut newest = self
            .newest
            .lock()
            .map_err(|_| CommandError::internal("The settings state failed. Restart AKM."))?;
        if ticket.0 <= *newest {
            return Ok(PreferenceSave::Superseded);
        }
        let mut written = None;
        owned(&mut |held| {
            written = Some(write_settings_file(
                held,
                file,
                &Settings {
                    vault_path: Some(vault_path.to_string()),
                },
            ));
        });
        match written {
            Some(Ok(())) => {
                *newest = ticket.0;
                Ok(PreferenceSave::Saved)
            }
            Some(Err(error)) => Err(error),
            None => Ok(PreferenceSave::Superseded),
        }
    }
}

pub fn settings_file<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| CommandError::new("config", error.to_string()))?;
    Ok(dir.join("settings.json"))
}

fn write_settings_file(
    _held: &Held<'_>,
    path: &Path,
    settings: &Settings,
) -> Result<(), CommandError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| CommandError::new("io", error.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(settings).map_err(CommandError::internal)?;
    fs::write(&tmp, body).map_err(|error| CommandError::new("io", error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| CommandError::new("io", error.to_string()))?;
    Ok(())
}

pub fn current_path(
    preferences: &dyn PreferenceStore,
    session: &crate::session::Session,
) -> Result<PathBuf, CommandError> {
    if let Some(path) = session.path()? {
        return Ok(path);
    }
    if let Some(path) = preferences.saved_vault_path() {
        return Ok(path.into());
    }
    preferences.default_vault_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(effect: &mut dyn FnMut(&Held<'_>)) {
        effect(&Held::for_test());
    }

    fn not_owned(_effect: &mut dyn FnMut(&Held<'_>)) {}

    fn saved(file: &Path) -> Option<String> {
        FilePreferences::new(Some(file.to_path_buf()), None).saved_vault_path()
    }

    #[test]
    fn an_older_ticket_cannot_write_after_a_newer_one_even_if_it_still_claims_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("fake-settings.json");
        let store = FilePreferences::new(Some(file.clone()), None);
        assert_eq!(
            store
                .save_vault_path(Ticket(7), &owned, "newer.kdbx")
                .unwrap(),
            PreferenceSave::Saved
        );
        for older in [6, 7] {
            assert_eq!(
                store
                    .save_vault_path(Ticket(older), &owned, "older.kdbx")
                    .unwrap(),
                PreferenceSave::Superseded
            );
        }
        assert_eq!(saved(&file).as_deref(), Some("newer.kdbx"));
        assert_eq!(
            store
                .save_vault_path(Ticket(8), &owned, "newest.kdbx")
                .unwrap(),
            PreferenceSave::Saved
        );
        assert_eq!(saved(&file).as_deref(), Some("newest.kdbx"));
    }

    #[test]
    fn a_ticket_that_no_longer_owns_the_session_writes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("fake-settings.json");
        let store = FilePreferences::new(Some(file.clone()), None);
        assert_eq!(
            store
                .save_vault_path(Ticket(3), &not_owned, "retired.kdbx")
                .unwrap(),
            PreferenceSave::Superseded
        );
        assert!(!file.exists());
        assert_eq!(
            store
                .save_vault_path(Ticket(2), &owned, "later-but-older.kdbx")
                .unwrap(),
            PreferenceSave::Saved,
            "a refused ticket does not advance the fence"
        );
    }

    #[test]
    fn a_failed_write_does_not_advance_the_fence() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-folder");
        fs::write(&blocker, b"fake").unwrap();
        let file = blocker.join("settings.json");
        let store = FilePreferences::new(Some(file), None);
        assert!(store.save_vault_path(Ticket(5), &owned, "a.kdbx").is_err());
        fs::remove_file(&blocker).unwrap();
        fs::create_dir(&blocker).unwrap();
        assert_eq!(
            store.save_vault_path(Ticket(4), &owned, "b.kdbx").unwrap(),
            PreferenceSave::Saved
        );
    }
}
