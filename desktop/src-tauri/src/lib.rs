mod clipboard;
mod commands;
mod error;
mod pending;
mod policy;
mod private_export;
mod session;
mod settings;
mod views;
mod visibility;
mod windows_session;

use std::sync::Arc;
use tauri::{Emitter, Manager};

use commands::*;
use session::{MonotonicClock, Session, VaultState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(visibility::WindowPresentation::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let session = Arc::new(Session::new(
                Arc::new(MonotonicClock::default()),
                Box::new(clipboard::NativeClipboard),
                Arc::new(move |notice| {
                    let _ = handle.emit("vault-locked", notice);
                }),
            ));
            match windows_session::start(session.clone()) {
                Ok(monitor) => {
                    app.manage(monitor);
                }
                Err(error) => {
                    session.warn(error.message);
                }
            }
            app.manage(VaultState(session));
            Ok(())
        })
        .on_window_event(|window, event| {
            visibility::on_window_event(window, event);
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                if let Some(state) = window.try_state::<VaultState>() {
                    let _ = state.0.lock();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
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
            audit_read,
            env_export,
            env_copy,
            env_save,
            env_import_preview,
            env_import_confirm,
            backup_info
        ])
        .build(tauri::generate_context!())
        .expect("error while building AKM");
    app.run(|app, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            if let Some(state) = app.try_state::<VaultState>() {
                let _ = state.0.lock();
            }
            if let Some(monitor) = app.try_state::<windows_session::Monitor>() {
                monitor.stop();
            }
        }
    });
}
