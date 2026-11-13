mod clipboard;
#[cfg(test)]
mod clipboard_fake;
mod commands;
mod disclosure;
#[doc(hidden)]
pub mod error;
mod import_review;
mod launch;
mod outcome;
mod policy;
mod private_export;
#[cfg(windows)]
mod process;
#[doc(hidden)]
pub mod publication;
mod review_window;
mod session;
mod settings;
#[cfg(test)]
mod test_support;
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
            app.manage(settings::PreferenceState(Arc::new(
                settings::FilePreferences::for_app(&handle),
            )));
            app.manage(import_review::ConfirmState(Arc::new(
                import_review::NativeConfirm(handle.clone()),
            )));
            app.manage(launch::TerminalState(Arc::new(launch::NativeTerminal(
                handle.clone(),
            ))));
            let session = Session::new(
                Arc::new(MonotonicClock::default()),
                Box::new(clipboard::NativeClipboard),
                Arc::new(move |notice| {
                    let _ = handle.emit("vault-locked", notice);
                }),
            );
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
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                if let Some(state) = webview.try_state::<VaultState>() {
                    state.0.document_started();
                }
            }
        })
        .on_window_event(|window, event| {
            visibility::on_window_event(window, event);
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                if let Some(state) = window.try_state::<VaultState>() {
                    state.0.shutdown();
                }
            }
        })
        .invoke_handler(commands::production_handlers!())
        .build(tauri::generate_context!())
        .expect("error while building AKM");
    app.run(|app, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            if let Some(state) = app.try_state::<VaultState>() {
                state.0.shutdown();
            }
            if let Some(monitor) = app.try_state::<windows_session::Monitor>() {
                monitor.stop();
            }
        }
    });
}
