//! The native window in which a person approves an operation that the page cannot script.

use std::path::Path;

use tauri::{AppHandle, Manager};

use crate::error::CommandError;

pub const CONFIRM_BUTTON: i32 = 1000;
pub const CANCEL_BUTTON: i32 = 1001;
/// Enter at the moment the window appears must not approve anything.
#[cfg(windows)]
const DEFAULT_BUTTON: i32 = CANCEL_BUTTON;
/// Windows answers Escape and the close button with this id, because the window is created with
/// `TDF_ALLOW_DIALOG_CANCELLATION`.
const ESCAPE_OR_CLOSE: i32 = 2;

/// Why a review window has no answer. It holds no name and no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewFailure(String);

impl ReviewFailure {
    pub fn new(detail: impl Into<String>) -> Self {
        Self(detail.into())
    }
}

impl From<ReviewFailure> for CommandError {
    fn from(failure: ReviewFailure) -> Self {
        CommandError::new(
            "reviewFailed",
            format!(
                "The confirmation window could not be shown, so nothing was done. Try again. ({})",
                failure.0
            ),
        )
    }
}

/// The answer of one review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAnswer {
    Approved,
    Declined,
    Failed(ReviewFailure),
}

/// The text of one review, ready to show.
#[derive(Clone, Debug)]
pub struct ReviewText {
    pub title: String,
    pub text: String,
    pub warning: bool,
    pub confirm_label: String,
}

/// The edge of the operating system. Production is `NativeReviewDialog`, and a test scripts it.
/// `show` runs with no session gate and no vault lock held: the window is modal and can stay
/// open for as long as the person likes.
pub trait ReviewDialog: Send + Sync {
    /// Shows the window and answers with the button that closed it, or why there is no button.
    fn show(&self, review: &ReviewText) -> Result<i32, ReviewFailure>;
}

/// Asks a person to approve something, in a window that the page cannot script.
///
/// `title` and `body` are text that AKM wrote. A name that came from the vault or the page must
/// already be escaped with `review_name` if it is in `body`. `names` come raw: each one is
/// escaped here and listed on its own line after `body`. The window answers `Approved` only on
/// a click on the `confirm_label` button. Its default button is Cancel.
///
/// This function does not know about the session. A caller that acts on the answer must check
/// the session again first (AGENTS.md, class A).
pub fn review_names(
    dialog: &dyn ReviewDialog,
    title: &str,
    body: &str,
    names: &[String],
    confirm_label: &str,
    warning: bool,
) -> ReviewAnswer {
    let review = ReviewText {
        title: title.to_string(),
        text: review_text(body, names),
        warning,
        confirm_label: confirm_label.to_string(),
    };
    answer_for(dialog.show(&review))
}

/// The whole text of a review: the body, then one line per name.
pub fn review_text(body: &str, names: &[String]) -> String {
    let mut text = body.to_string();
    for name in names {
        text.push_str(&format!("\n  {}", review_name(name)));
    }
    text
}

fn approved(button: i32) -> bool {
    button == CONFIRM_BUTTON
}

fn answer_for(shown: Result<i32, ReviewFailure>) -> ReviewAnswer {
    match shown {
        Ok(button) if approved(button) => ReviewAnswer::Approved,
        _ => ReviewAnswer::Declined,
    }
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

/// The real window, with the main window of the app as its owner.
pub struct NativeReviewDialog<R: tauri::Runtime>(pub AppHandle<R>);

impl<R: tauri::Runtime> ReviewDialog for NativeReviewDialog<R> {
    fn show(&self, review: &ReviewText) -> Result<i32, ReviewFailure> {
        #[cfg(windows)]
        let parent = self
            .0
            .get_webview_window("main")
            .and_then(|window| window.hwnd().ok())
            .map_or(0, |handle| handle.0 as isize);
        #[cfg(not(windows))]
        let parent = {
            let _ = self.0.get_webview_window("main");
            0
        };
        task_dialog(parent, review)
    }
}

/// The buffers that a `TASKDIALOGCONFIG` points into. It must outlive the config.
#[cfg(windows)]
struct DialogText {
    title: Vec<u16>,
    text: Vec<u16>,
    _confirm: Vec<u16>,
    _cancel: Vec<u16>,
    buttons: [windows_sys::Win32::UI::Controls::TASKDIALOG_BUTTON; 2],
    warning: bool,
}

#[cfg(windows)]
impl DialogText {
    fn new(review: &ReviewText) -> Self {
        use windows_sys::Win32::UI::Controls::TASKDIALOG_BUTTON;
        let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
        let confirm = wide(&review.confirm_label);
        let cancel = wide("Cancel");
        let buttons = [
            TASKDIALOG_BUTTON {
                nButtonID: CONFIRM_BUTTON,
                pszButtonText: confirm.as_ptr(),
            },
            TASKDIALOG_BUTTON {
                nButtonID: CANCEL_BUTTON,
                pszButtonText: cancel.as_ptr(),
            },
        ];
        Self {
            title: wide(&review.title),
            text: wide(&review.text),
            _confirm: confirm,
            _cancel: cancel,
            buttons,
            warning: review.warning,
        }
    }

    fn config(&self, parent: isize) -> windows_sys::Win32::UI::Controls::TASKDIALOGCONFIG {
        use windows_sys::Win32::{
            Foundation::HWND,
            UI::Controls::{
                TASKDIALOGCONFIG, TDF_ALLOW_DIALOG_CANCELLATION, TDF_SIZE_TO_CONTENT,
                TD_INFORMATION_ICON, TD_WARNING_ICON,
            },
        };
        // SAFETY: all-zero is a valid empty TASKDIALOGCONFIG: null pointers and zero counts.
        let mut config: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
        config.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
        config.hwndParent = parent as HWND;
        config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT;
        config.pszWindowTitle = self.title.as_ptr();
        config.pszContent = self.text.as_ptr();
        config.Anonymous1.pszMainIcon = if self.warning {
            TD_WARNING_ICON
        } else {
            TD_INFORMATION_ICON
        };
        config.pButtons = self.buttons.as_ptr();
        config.cButtons = self.buttons.len() as u32;
        config.nDefaultButton = DEFAULT_BUTTON;
        config
    }
}

#[cfg(windows)]
fn task_dialog(parent: isize, review: &ReviewText) -> Result<i32, ReviewFailure> {
    use windows_sys::Win32::UI::Controls::TaskDialogIndirect;

    let text = DialogText::new(review);
    let config = text.config(parent);
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
    button_from(result, pressed)
}

#[cfg(not(windows))]
fn task_dialog(_: isize, _: &ReviewText) -> Result<i32, ReviewFailure> {
    Err(ReviewFailure::new(
        "A review window exists on Windows only.",
    ))
}

/// The result of the system call, as a button or as the reason that there is none.
#[cfg_attr(not(windows), allow(dead_code))]
fn button_from(result: i32, pressed: i32) -> Result<i32, ReviewFailure> {
    if result == 0 {
        Ok(pressed)
    } else {
        Err(ReviewFailure::new(format!(
            "TaskDialogIndirect returned {result:#010x}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_confirm_button_approves_and_it_is_not_the_default() {
        assert!(approved(CONFIRM_BUTTON));
        for button in [CANCEL_BUTTON, ESCAPE_OR_CLOSE, 0, 1] {
            assert!(!approved(button), "{button}");
        }
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

    #[test]
    fn the_names_come_after_the_body_one_per_line_and_escaped() {
        assert_eq!(
            review_text("Hold these:", &["A".into(), "B\nC".into()]),
            "Hold these:\n  A\n  B\\u{a}C"
        );
        assert_eq!(review_text("Only text", &[]), "Only text");
    }
}
