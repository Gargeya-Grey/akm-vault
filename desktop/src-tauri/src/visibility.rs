use std::sync::Mutex;
use tauri::{Emitter, Manager};

use crate::{
    error::CommandError,
    session::{NativeViewNotice, Session, VaultState},
};

/// One owned main window. Observe actual current minimized state, not focus loss
/// (a native save dialog must not expire its own originating presentation).
#[derive(Default)]
pub struct WindowPresentation(Mutex<bool>);

impl WindowPresentation {
    fn observe(&self, minimized: bool) -> Result<bool, CommandError> {
        let mut previous = self
            .0
            .lock()
            .map_err(|_| CommandError::internal("Native presentation state failed."))?;
        let edge = minimized && !*previous;
        *previous = minimized;
        Ok(edge)
    }
}

pub fn expire_and_notify(
    session: &Session,
    emit: impl FnOnce(&NativeViewNotice) -> Result<(), ()>,
) -> Result<Option<NativeViewNotice>, CommandError> {
    notify_expiry(session, session.expire_presentation(), emit)
}

/// For a page request that names its session. A session that is not the current one has
/// nothing to expire: no notice and no lock for the session that replaced it.
pub fn expire_and_notify_for(
    session: &Session,
    session_id: &str,
    emit: impl FnOnce(&NativeViewNotice) -> Result<(), ()>,
) -> Result<Option<NativeViewNotice>, CommandError> {
    notify_expiry(session, session.expire_presentation_of(session_id), emit)
}

fn notify_expiry(
    session: &Session,
    expired: Result<Option<NativeViewNotice>, CommandError>,
    emit: impl FnOnce(&NativeViewNotice) -> Result<(), ()>,
) -> Result<Option<NativeViewNotice>, CommandError> {
    let Some(notice) = expired? else {
        return Ok(None);
    };
    if emit(&notice).is_err() {
        session.warn("A native view-expiry notice could not be delivered. Check AKM's lock state before relying on the view.".into());
        // A replacement owner may already exist by the time delivery fails.
        // Never use an unconditional lock for an old presentation notice.
        let _ = session.lock_owned(Some(&notice.session_id));
        return Err(CommandError::internal(
            "Could not deliver native presentation expiry.",
        ));
    }
    Ok(Some(notice))
}

fn fail_observation(session: &Session) {
    session.warn("Native window visibility could not be observed. Check that AKM is locked before leaving it.".into());
    if let Ok(Some(notice)) = session.expire_presentation() {
        let _ = session.lock_owned(Some(&notice.session_id));
    }
}

pub fn on_window_event<R: tauri::Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if window.label() != "main" || !visibility_event(event) {
        return;
    }
    let Some(session) = window.try_state::<VaultState>() else {
        return;
    };
    let observed = window.is_minimized().map_err(|_| ()).and_then(|minimized| {
        window
            .try_state::<WindowPresentation>()
            .ok_or(())?
            .observe(minimized)
            .map_err(|_| ())
    });
    match observed {
        Ok(true) => {
            let _ = expire_and_notify(&session.0, |notice| {
                window.emit("vault-view-expired", notice).map_err(|_| ())
            });
        }
        Ok(false) => {}
        Err(()) => fail_observation(&session.0),
    }
}

fn visibility_event(event: &tauri::WindowEvent) -> bool {
    matches!(event, tauri::WindowEvent::Resized(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_presentation_minimized_edges_coalesce_and_restore_rearms() {
        let presentation = WindowPresentation::default();
        assert!(!presentation.observe(false).unwrap());
        assert!(presentation.observe(true).unwrap());
        assert!(!presentation.observe(true).unwrap());
        assert!(!presentation.observe(false).unwrap());
        assert!(!presentation.observe(false).unwrap());
        assert!(presentation.observe(true).unwrap());
    }

    #[test]
    fn native_presentation_state_failure_is_reported_instead_of_panicking() {
        let presentation = std::sync::Arc::new(WindowPresentation::default());
        let target = presentation.clone();
        assert!(std::thread::spawn(move || {
            let _guard = target.0.lock().unwrap();
            panic!("fake presentation poison");
        })
        .join()
        .is_err());
        assert!(presentation.observe(true).is_err());
    }

    #[test]
    fn native_presentation_focus_loss_and_zero_size_do_not_manufacture_a_hide() {
        assert!(!visibility_event(&tauri::WindowEvent::Focused(false)));
        assert!(!visibility_event(&tauri::WindowEvent::Moved(
            tauri::PhysicalPosition::new(0, 0)
        )));
        assert!(visibility_event(&tauri::WindowEvent::Resized(
            tauri::PhysicalSize::new(0, 0)
        )));
        assert!(
            !WindowPresentation::default().observe(false).unwrap(),
            "even a zero-sized resize requires actual minimized state"
        );
    }
}
