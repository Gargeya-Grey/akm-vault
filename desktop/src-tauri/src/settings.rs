use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

use crate::{error::CommandError, session::Session};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub vault_path: Option<String>,
}

pub fn settings_file<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, CommandError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| CommandError::new("config", error.to_string()))?;
    Ok(dir.join("settings.json"))
}

fn read_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> Settings {
    settings_file(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn write_settings(app: &AppHandle, settings: &Settings) -> Result<(), CommandError> {
    let path = settings_file(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| CommandError::new("io", error.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(settings).map_err(CommandError::internal)?;
    fs::write(&tmp, body).map_err(|error| CommandError::new("io", error.to_string()))?;
    fs::rename(&tmp, &path).map_err(|error| CommandError::new("io", error.to_string()))?;
    Ok(())
}

pub fn current_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
) -> Result<PathBuf, CommandError> {
    if let Some(path) = session.path()? {
        return Ok(path);
    }
    if let Some(path) = read_settings(app).vault_path {
        return Ok(path.into());
    }
    let base = app
        .path()
        .data_dir()
        .map_err(|error| CommandError::new("config", error.to_string()))?;
    Ok(base.join("akm").join("vault.kdbx"))
}
