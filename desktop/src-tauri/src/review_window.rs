//! The native window in which a person approves an operation that the page cannot script.

use std::{path::Path, sync::mpsc};

use tauri::{AppHandle, Manager};

use crate::session::Held;

/// Waits for the person's answer. The session gate is not held while it waits.
pub type ReviewAnswer<'a> = Box<dyn FnOnce() -> bool + Send + 'a>;

/// A review that is built but not yet on screen. It appears only under the session gate, so a
/// lock that came first means that no review of the old session appears.
pub type ReviewWindow<'a> = Box<dyn FnOnce(&Held<'_>) -> ReviewAnswer<'a> + Send + 'a>;

const CONFIRM_BUTTON: i32 = 1000;
const CANCEL_BUTTON: i32 = 1001;
/// Enter at the moment the window appears must not approve anything.
const DEFAULT_BUTTON: i32 = CANCEL_BUTTON;

fn approved(button: i32) -> bool {
    button == CONFIRM_BUTTON
}

const LONGEST_NAME: usize = 80;

fn hidden_from_the_reader(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{e0000}'..='\u{e007f}'
        )
}

fn push_visible(out: &mut String, c: char) {
    if hidden_from_the_reader(c) {
        out.push_str(&format!("\\u{{{:x}}}", c as u32));
    } else {
        out.push(c);
    }
}

/// A name from the vault or the page, for the text of a review. It stays on its line and inside
/// its quotation marks, so that it cannot pass for another line of the review.
pub fn review_name(raw: &str) -> String {
    let mut out = String::new();
    for (index, c) in raw.chars().enumerate() {
        if index == LONGEST_NAME {
            out.push_str(&format!("… ({} characters)", raw.chars().count()));
            break;
        }
        if c == '"' {
            out.push_str("\\\"");
        } else {
            push_visible(&mut out, c);
        }
    }
    out
}

/// A path for the text of a review. It is shown whole, because the person approves that folder.
pub fn review_path(path: &Path) -> String {
    let mut out = String::new();
    for c in path.display().to_string().chars() {
        push_visible(&mut out, c);
    }
    out
}

/// The native review, ready to show. Everything that needs the main thread happens here,
/// before the session gate is taken, because the main thread can be waiting for that gate.
pub fn native_review<'a, R: tauri::Runtime>(
    app: &AppHandle<R>,
    title: &'static str,
    message: String,
    warning: bool,
    confirm_label: &'static str,
) -> ReviewWindow<'a> {
    #[cfg(windows)]
    let parent = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map_or(0, |handle| handle.0 as isize);
    #[cfg(not(windows))]
    let parent = {
        let _ = app.get_webview_window("main");
        0
    };
    Box::new(move |_held| {
        let (answer, answered) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = answer.send(show(parent, title, &message, warning, confirm_label));
        });
        Box::new(move || answered.recv().unwrap_or(false))
    })
}

#[cfg(windows)]
fn show(parent: isize, title: &str, message: &str, warning: bool, confirm_label: &str) -> bool {
    use windows_sys::Win32::{
        Foundation::{HWND, S_OK},
        UI::Controls::{
            TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOG_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION,
            TDF_SIZE_TO_CONTENT, TD_INFORMATION_ICON, TD_WARNING_ICON,
        },
    };

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let (title, message, confirm_label, cancel_label) = (
        wide(title),
        wide(message),
        wide(confirm_label),
        wide("Cancel"),
    );
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: CONFIRM_BUTTON,
            pszButtonText: confirm_label.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: CANCEL_BUTTON,
            pszButtonText: cancel_label.as_ptr(),
        },
    ];
    // SAFETY: all-zero is a valid empty TASKDIALOGCONFIG: null pointers and zero counts.
    let mut config: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
    config.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
    config.hwndParent = parent as HWND;
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT;
    config.pszWindowTitle = title.as_ptr();
    config.pszContent = message.as_ptr();
    config.Anonymous1.pszMainIcon = if warning {
        TD_WARNING_ICON
    } else {
        TD_INFORMATION_ICON
    };
    config.pButtons = buttons.as_ptr();
    config.cButtons = buttons.len() as u32;
    config.nDefaultButton = DEFAULT_BUTTON;

    let mut pressed = 0;
    // SAFETY: the config and the buffers it points to outlive this call, and `pressed` is
    // writable. The other two out-pointers are optional.
    let result = unsafe {
        TaskDialogIndirect(
            &config,
            &mut pressed,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    result == S_OK && approved(pressed)
}

#[cfg(not(windows))]
fn show(_: isize, _: &str, _: &str, _: bool, _: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_confirm_button_approves_and_it_is_not_the_default() {
        const ESCAPE_OR_CLOSE: i32 = 2;
        assert!(approved(CONFIRM_BUTTON));
        for button in [CANCEL_BUTTON, ESCAPE_OR_CLOSE, 0, 1] {
            assert!(!approved(button), "{button}");
        }
        assert!(!approved(DEFAULT_BUTTON));
    }

    #[test]
    fn a_name_stays_on_its_line_and_inside_its_quotes() {
        let shown = review_name("demo\"\nFolder: C:\\trusted\u{202e}");
        assert_eq!(shown, "demo\\\"\\u{a}Folder: C:\\trusted\\u{202e}");
    }

    #[test]
    fn ordinary_names_are_unchanged_and_a_long_one_says_how_long_it_is() {
        assert_eq!(review_name("my-app (staging)"), "my-app (staging)");
        assert_eq!(review_name("Müller 東京"), "Müller 東京");
        let shown = review_name(&"x".repeat(500));
        assert!(shown.starts_with(&"x".repeat(80)), "{shown}");
        assert!(shown.ends_with("… (500 characters)"), "{shown}");
    }

    #[test]
    fn a_path_is_shown_whole_with_hidden_characters_made_visible() {
        let long = format!("C:\\{}", "a".repeat(200));
        assert_eq!(review_path(Path::new(&long)), long);
        assert_eq!(
            review_path(Path::new("C:\\work\u{202e}txt")),
            "C:\\work\\u{202e}txt"
        );
    }
}
