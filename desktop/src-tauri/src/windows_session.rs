use std::sync::Arc;

use crate::{error::CommandError, policy::Signal, session::Session};

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
        sync::{mpsc, Mutex},
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Power::{RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification},
            RemoteDesktop::{
                WTSActive, WTSFreeMemory, WTSQuerySessionInformationW,
                WTSRegisterSessionNotification, WTSSessionInfoEx, WTSUnRegisterSessionNotification,
                WTSINFOEXW, WTS_SESSIONSTATE_UNLOCK,
            },
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            GetWindowLongPtrW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer,
            SetWindowLongPtrW, TranslateMessage, UnregisterClassW, CREATESTRUCTW,
            DEVICE_NOTIFY_WINDOW_HANDLE, GWLP_USERDATA, MSG, WM_CLOSE, WM_DESTROY, WM_NCCREATE,
            WM_NCDESTROY, WM_POWERBROADCAST, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_POPUP,
        },
    };

    struct Context {
        session: Arc<Session>,
        window: Arc<Mutex<Option<isize>>>,
    }

    pub struct Monitor {
        window: Arc<Mutex<Option<isize>>>,
    }
    impl Monitor {
        pub fn stop(&self) {
            // The owning thread destroys its own window and unregisters its notifications.
            if let Ok(window) = self.window.lock() {
                if let Some(hwnd) = *window {
                    unsafe {
                        PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
                    }
                }
            }
        }
    }
    impl Drop for Monitor {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub fn query_signal() -> Signal {
        unsafe {
            let mut buffer = null_mut();
            let mut bytes = 0;
            if WTSQuerySessionInformationW(
                null_mut(),
                u32::MAX,
                WTSSessionInfoEx,
                &mut buffer,
                &mut bytes,
            ) == 0
            {
                return Signal::Unobservable;
            }
            let result = if !buffer.is_null() && bytes as usize >= size_of::<WTSINFOEXW>() {
                let info = &*buffer.cast::<WTSINFOEXW>();
                if info.Level != 1 {
                    Signal::Unobservable
                } else {
                    let session = info.Data.WTSInfoExLevel1;
                    if session.SessionState == WTSActive
                        && session.SessionFlags as u32 == WTS_SESSIONSTATE_UNLOCK
                    {
                        Signal::Available
                    } else {
                        Signal::Locked
                    }
                }
            } else {
                Signal::Unobservable
            };
            WTSFreeMemory(buffer.cast());
            result
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let context = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
        if !context.is_null() {
            let session = &(*context).session;
            match message {
                WM_WTSSESSION_CHANGE => {
                    // Lock/disconnect/logoff must invalidate pending unlocks even before WTS catches up.
                    match wparam {
                        2 | 4 | 6 | 7 => session.observe(Signal::Locked),
                        _ => session.observe(query_signal()),
                    }
                    return 0;
                }
                WM_POWERBROADCAST => {
                    // PBT_APMSUSPEND and every resume invalidate the old session.
                    if matches!(wparam, 4 | 6 | 7 | 18) {
                        session.observe(Signal::Suspend);
                        if wparam != 4 {
                            session.observe(query_signal());
                        }
                    }
                    return 1;
                }
                WM_TIMER => {
                    session.observe(query_signal());
                    session.tick();
                    return 0;
                }
                WM_CLOSE => {
                    session.observe(Signal::Unobservable);
                    PostQuitMessage(0);
                    return 0;
                }
                WM_DESTROY => {
                    if let Ok(mut window) = (*context).window.lock() {
                        *window = None;
                    }
                    PostQuitMessage(0);
                    return 0;
                }
                WM_NCDESTROY => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    pub fn start(session: Arc<Session>) -> Result<Monitor, CommandError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let window = Arc::new(Mutex::new(None));
        let thread_window = window.clone();
        std::thread::Builder::new()
            .name("akm-windows-session".into())
            .spawn(move || {
                let mut context = Box::new(Context {
                    session: session.clone(),
                    window: thread_window.clone(),
                });
                let name: Vec<u16> = format!("AKMSession-{}", uuid::Uuid::new_v4())
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                unsafe {
                    let instance = GetModuleHandleW(null());
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(window_proc),
                        hInstance: instance,
                        lpszClassName: name.as_ptr(),
                        ..Default::default()
                    };
                    if RegisterClassW(&class) == 0 {
                        let _ = ready_tx.send(Err(std::io::Error::last_os_error().to_string()));
                        session.observe(Signal::Unobservable);
                        return;
                    }
                    // An invisible top-level window receives broadcast power messages as well as directed WTS messages.
                    let hwnd = CreateWindowExW(
                        0,
                        name.as_ptr(),
                        name.as_ptr(),
                        WS_POPUP,
                        0,
                        0,
                        0,
                        0,
                        null_mut(),
                        null_mut(),
                        instance,
                        (&mut *context as *mut Context).cast(),
                    );
                    if hwnd.is_null() {
                        let _ = ready_tx.send(Err(std::io::Error::last_os_error().to_string()));
                        UnregisterClassW(name.as_ptr(), instance);
                        session.observe(Signal::Unobservable);
                        return;
                    }
                    if let Ok(mut window) = thread_window.lock() {
                        *window = Some(hwnd as isize);
                    }
                    let wts = WTSRegisterSessionNotification(hwnd, 0) != 0;
                    let power =
                        RegisterSuspendResumeNotification(hwnd, DEVICE_NOTIFY_WINDOW_HANDLE);
                    let timer = SetTimer(hwnd, 1, 1000, None);
                    if !wts || power == 0 || timer == 0 {
                        let _ = ready_tx.send(Err(
                            "Windows lock/suspend notifications could not be registered.".into(),
                        ));
                    } else {
                        session.observe(query_signal());
                        if ready_tx.send(Ok(hwnd as isize)).is_ok() {
                            let mut message = MSG::default();
                            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                                TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                    }
                    session.observe(Signal::Unobservable);
                    KillTimer(hwnd, 1);
                    if power != 0 {
                        UnregisterSuspendResumeNotification(power);
                    }
                    if wts {
                        WTSUnRegisterSessionNotification(hwnd);
                    }
                    DestroyWindow(hwnd);
                    UnregisterClassW(name.as_ptr(), instance);
                }
            })
            .map_err(CommandError::internal)?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(_)) => Ok(Monitor { window }),
            Ok(Err(error)) => Err(CommandError::new("sessionUnavailable", error)),
            Err(error) => Err(CommandError::new("sessionUnavailable", error.to_string())),
        }
    }
}

#[cfg(windows)]
pub use windows::{start, Monitor};

#[cfg(not(windows))]
pub struct Monitor;

#[cfg(not(windows))]
impl Monitor {
    pub fn stop(&self) {}
}

#[cfg(not(windows))]
pub fn start(session: Arc<Session>) -> Result<Monitor, CommandError> {
    session.observe(Signal::Unobservable);
    Err(CommandError::new(
        "sessionUnavailable",
        "Native session protection is implemented for Windows only.",
    ))
}
