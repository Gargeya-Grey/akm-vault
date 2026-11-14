use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager};

use crate::{
    error::CommandError,
    policy::Ticket,
    session::{Effect, Held},
};

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

/// Commits the prepared rename only while the opening that asks still owns the session:
/// `Session::commit`, bound to the ticket of that opening. It revokes the preparation when it
/// refuses or the rename fails.
pub type OwnerFence<'a> = &'a dyn Fn(&mut dyn Effect<Output = ()>) -> Result<(), CommandError>;

/// Where the last opened vault path lives. A newer opening always wins over an older one.
pub trait PreferenceStore: Send + Sync {
    fn saved_vault_path(&self) -> Option<String>;
    fn default_vault_path(&self) -> Result<PathBuf, CommandError>;
    /// The rename happens inside `owned`, so the ownership check and the rename are one step.
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
        // Lock order is this lock, then the session gate. The temporary file is written
        // outside the gate. The ownership check and the rename are one step under that gate,
        // so a newer opening can neither retire the owner between them nor write before this
        // rename is done.
        let mut newest = self
            .newest
            .lock()
            .map_err(|_| CommandError::internal("The settings state failed. Restart AKM."))?;
        if ticket.0 <= *newest {
            return Ok(PreferenceSave::Superseded);
        }
        let mut prepared = PreparedSettings::write(
            file,
            &Settings {
                vault_path: Some(vault_path.to_string()),
            },
        )?;
        match owned(&mut prepared) {
            Ok(()) => {
                *newest = ticket.0;
                Ok(PreferenceSave::Saved)
            }
            Err(error) if prepared.attempted => Err(error),
            Err(_) => Ok(PreferenceSave::Superseded),
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

/// Writes `body` to a new file beside `path` and returns its name. The name is random and the
/// file is created with `create_new`, so the write never opens a file that exists, and the
/// later rename or delete uses a name that no other writer chose (fault class F, U1-R1-05).
pub(crate) fn write_beside(path: &Path, body: &[u8]) -> Result<PathBuf, CommandError> {
    let io = |error: std::io::Error| CommandError::new("io", error.to_string());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(io)?;
    if let Err(error) = file.write_all(body) {
        drop(file);
        let _ = fs::remove_file(&tmp);
        return Err(io(error));
    }
    Ok(tmp)
}

/// The settings file, written beside its place. The commit renames it into place. A drop
/// before the rename deletes it, so a refusal, a failed rename and a panic leave no temporary
/// file. The file holds only the vault path.
struct PreparedSettings {
    tmp: PathBuf,
    path: PathBuf,
    attempted: bool,
    placed: bool,
}

impl PreparedSettings {
    fn write(path: &Path, settings: &Settings) -> Result<Self, CommandError> {
        let body = serde_json::to_vec_pretty(settings).map_err(CommandError::internal)?;
        Ok(Self {
            tmp: write_beside(path, &body)?,
            path: path.to_path_buf(),
            attempted: false,
            placed: false,
        })
    }
}

impl Effect for PreparedSettings {
    type Output = ();

    fn act(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
        self.attempted = true;
        fs::rename(&self.tmp, &self.path)
            .map_err(|error| CommandError::new("io", error.to_string()))?;
        self.placed = true;
        Ok(())
    }

    /// Nothing to undo here: the drop deletes the temporary file.
    fn revoke(&mut self) -> Result<(), String> {
        Ok(())
    }
}

impl Drop for PreparedSettings {
    fn drop(&mut self) {
        if !self.placed {
            let _ = fs::remove_file(&self.tmp);
        }
    }
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

    /// `Session::commit` for a session that is current: act, and revoke when the act fails.
    fn owned(effect: &mut dyn Effect<Output = ()>) -> Result<(), CommandError> {
        let result = effect.act(&Held::for_test());
        if result.is_err() {
            effect.revoke().unwrap();
        }
        result
    }

    /// `Session::commit` for a session that ended: refuse and revoke.
    fn not_owned(effect: &mut dyn Effect<Output = ()>) -> Result<(), CommandError> {
        effect.revoke().unwrap();
        Err(CommandError::locked())
    }

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
        assert!(
            folder_names(directory.path()).is_empty(),
            "a refused commit deletes the prepared file"
        );
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

    fn folder_names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// U1-R1-05 (class F): a file at the old fixed name belongs to someone else. A committed, a
    /// refused, a failed and a panic-dropped preparation each leave it as it was, and leave no
    /// temporary file of their own.
    #[test]
    fn u1_r1_05_a_sentinel_at_the_old_fixed_name_survives_every_way_a_preparation_ends() {
        type Fence = fn(&mut dyn Effect<Output = ()>) -> Result<(), CommandError>;
        let panics: Fence = |_| panic!("a fake fault after the preparation");
        let ways: [(&str, Fence); 4] = [
            ("committed", owned),
            ("refused", not_owned),
            ("failed", owned),
            ("panic-dropped", panics),
        ];
        for (way, fence) in ways {
            let directory = tempfile::tempdir().unwrap();
            let file = directory.path().join("settings.json");
            if way == "failed" {
                fs::create_dir(&file).unwrap();
            }
            let sentinel = file.with_extension("json.tmp");
            fs::write(&sentinel, "fake-file-of-another-process").unwrap();
            let store = FilePreferences::new(Some(file.clone()), None);
            let saved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store.save_vault_path(Ticket(1), &fence, "fake.kdbx")
            }));
            match way {
                "committed" => assert_eq!(saved.unwrap().unwrap(), PreferenceSave::Saved),
                "refused" => assert_eq!(saved.unwrap().unwrap(), PreferenceSave::Superseded),
                "failed" => assert!(saved.unwrap().is_err()),
                _ => assert!(saved.is_err(), "{way}: the panic unwound"),
            }
            assert_eq!(
                fs::read_to_string(&sentinel).unwrap(),
                "fake-file-of-another-process",
                "{way}: the file at the old fixed name is unchanged"
            );
            let expected: &[&str] = match way {
                "committed" | "failed" => &["settings.json", "settings.json.tmp"],
                _ => &["settings.json.tmp"],
            };
            assert_eq!(
                folder_names(directory.path()),
                expected,
                "{way}: no temporary file remains"
            );
        }
    }

    /// U1-R1-05: two preference stores (two AKM processes) prepare at once. Each renames its own
    /// temporary file into place, so the last rename wins with its own content, in either order.
    #[test]
    fn u1_r1_05_two_interleaved_stores_each_publish_their_own_content() {
        for a_moves_first in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let file = directory.path().join("settings.json");
            let a = FilePreferences::new(Some(file.clone()), None);
            let b = FilePreferences::new(Some(file.clone()), None);
            let inner = |effect: &mut dyn Effect<Output = ()>| -> Result<(), CommandError> {
                if a_moves_first {
                    owned(effect)?;
                }
                // B prepares and renames while A's temporary file waits.
                assert_eq!(
                    b.save_vault_path(Ticket(1), &owned, "fake-b.kdbx").unwrap(),
                    PreferenceSave::Saved
                );
                assert_eq!(saved(&file).as_deref(), Some("fake-b.kdbx"));
                if a_moves_first {
                    Ok(())
                } else {
                    owned(effect)
                }
            };
            assert_eq!(
                a.save_vault_path(Ticket(1), &inner, "fake-a.kdbx").unwrap(),
                PreferenceSave::Saved
            );
            let last = if a_moves_first {
                "fake-b.kdbx"
            } else {
                "fake-a.kdbx"
            };
            assert_eq!(
                saved(&file).as_deref(),
                Some(last),
                "a_moves_first: {a_moves_first}"
            );
            assert_eq!(folder_names(directory.path()), ["settings.json"]);
        }
    }
}
