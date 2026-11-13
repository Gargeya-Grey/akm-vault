//! The native window in which a person approves an operation that the page cannot script.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
};

use tauri::{AppHandle, Manager};

use crate::error::CommandError;

pub const CONFIRM_BUTTON: i32 = 1000;
pub const CANCEL_BUTTON: i32 = 1001;
/// Enter at the moment the window appears must not approve anything.
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
    if check_shown_in_full(names.iter().map(String::as_str)).is_err() {
        return ReviewAnswer::Failed(ReviewFailure::new("a name is too long to show in full"));
    }
    let review = ReviewText {
        title: title.to_string(),
        text: review_text(body, names),
        warning,
        confirm_label: confirm_label.to_string(),
    };
    let shown = catch_unwind(AssertUnwindSafe(|| dialog.show(&review)))
        .unwrap_or_else(|_| Err(ReviewFailure::new("the window code stopped unexpectedly")));
    answer_for(shown)
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

/// Only a click on the confirm button approves. Cancel, Escape and close decline. A window that
/// could not answer is a failure and never a decline: the person did not say no.
fn answer_for(shown: Result<i32, ReviewFailure>) -> ReviewAnswer {
    match shown {
        Ok(button) if approved(button) => ReviewAnswer::Approved,
        Ok(CANCEL_BUTTON | ESCAPE_OR_CLOSE) => ReviewAnswer::Declined,
        Ok(other) => ReviewAnswer::Failed(ReviewFailure::new(format!("unexpected button {other}"))),
        Err(failure) => ReviewAnswer::Failed(failure),
    }
}

/// The longest name, as shown, that a review can hold. A longer one is refused before any window
/// opens, because a review never cuts a name short.
pub const LONGEST_SHOWN_NAME: usize = 256;

/// A character is shown as itself only if it is on this list. Anything else, known or not, is
/// shown as an escape, so a character that a list of invisible ones forgot cannot hide.
fn shown_as_itself(c: char) -> bool {
    match c {
        ' '..='~' => true,
        _ if c.is_ascii() => false,
        // Letters by their Unicode class, but they draw nothing.
        '\u{115f}' | '\u{1160}' | '\u{3164}' | '\u{ffa0}' => false,
        _ => c.is_alphanumeric(),
    }
}

fn push_escaped(out: &mut String, c: char) {
    out.push_str(&format!("\\u{{{:x}}}", c as u32));
}

fn space_could_hide(chars: &[char], index: usize) -> bool {
    index == 0 || index + 1 == chars.len() || chars[index - 1] == ' '
}

fn push_visible(out: &mut String, chars: &[char], index: usize) {
    let c = chars[index];
    if shown_as_itself(c) && !(c == ' ' && space_could_hide(chars, index)) {
        out.push(c);
    } else {
        push_escaped(out, c);
    }
}

/// A name from the vault or the page, for the text of a review. It is shown whole. It stays on
/// its line and inside its quotation marks. The escapes can be read back without doubt, so two
/// different names never read the same.
pub fn review_name(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    for (index, c) in chars.iter().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => push_visible(&mut out, &chars, index),
        }
    }
    out
}

/// Refuses a name that a review could not show in full.
pub fn check_shown_in_full<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), CommandError> {
    if names
        .into_iter()
        .any(|name| review_name(name).chars().count() > LONGEST_SHOWN_NAME)
    {
        return Err(CommandError::new(
            "tooLarge",
            format!(
                "A name in this request is longer than the {LONGEST_SHOWN_NAME} characters that a confirmation window can show in full, so nothing was done."
            ),
        ));
    }
    Ok(())
}

/// A path for the text of a review. It is shown whole, because the person approves that folder.
/// Its backslashes stay as they are. `show_exactly` refuses a path that this text could not
/// tell apart from another one.
pub fn review_path(path: &Path) -> String {
    let chars: Vec<char> = path.display().to_string().chars().collect();
    let mut out = String::new();
    for index in 0..chars.len() {
        push_visible(&mut out, &chars, index);
    }
    out
}

/// Whether `review_path` can show this path exactly. A path that is not valid Unicode has no
/// exact text, and the literal text of an escape would read the same as a real hidden character.
pub fn show_exactly(path: &Path) -> bool {
    path.to_str().is_some_and(|text| !text.contains("\\u{"))
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
    fn only_the_confirm_button_approves_and_escape_cancel_and_close_decline() {
        assert_eq!(answer_for(Ok(CONFIRM_BUTTON)), ReviewAnswer::Approved);
        for button in [CANCEL_BUTTON, ESCAPE_OR_CLOSE] {
            assert_eq!(answer_for(Ok(button)), ReviewAnswer::Declined, "{button}");
        }
        for button in [0, 1, 6, 7] {
            assert!(
                matches!(answer_for(Ok(button)), ReviewAnswer::Failed(_)),
                "{button}"
            );
        }
    }

    #[test]
    fn a_failed_system_call_is_a_failure_and_never_a_decline() {
        const E_FAIL: i32 = 0x8000_4005_u32 as i32;
        assert!(matches!(
            answer_for(button_from(E_FAIL, 0)),
            ReviewAnswer::Failed(_)
        ));
        assert_eq!(
            answer_for(button_from(0, CANCEL_BUTTON)),
            ReviewAnswer::Declined
        );
        assert_eq!(
            answer_for(button_from(0, CONFIRM_BUTTON)),
            ReviewAnswer::Approved
        );
    }

    struct Panics;
    impl ReviewDialog for Panics {
        fn show(&self, _: &ReviewText) -> Result<i32, ReviewFailure> {
            panic!("the window worker failed")
        }
    }

    #[test]
    fn a_worker_failure_in_the_review_window_is_a_failure_and_never_a_decline() {
        let answer = review_names(&Panics, "Title", "Body", &[], "Go", false);
        assert!(matches!(answer, ReviewAnswer::Failed(_)), "{answer:?}");
    }

    struct Recording(std::sync::Mutex<Vec<ReviewText>>);
    impl ReviewDialog for Recording {
        fn show(&self, review: &ReviewText) -> Result<i32, ReviewFailure> {
            self.0.lock().unwrap().push(review.clone());
            Ok(CONFIRM_BUTTON)
        }
    }

    #[test]
    fn review_names_lists_each_name_in_the_window_and_returns_the_answer() {
        let dialog = Recording(std::sync::Mutex::new(Vec::new()));
        let answer = review_names(
            &dialog,
            "Title",
            "Body:",
            &["ONE".into(), "TWO".into()],
            "Approve",
            true,
        );
        assert_eq!(answer, ReviewAnswer::Approved);
        let shown = dialog.0.lock().unwrap().clone();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].title, "Title");
        assert_eq!(shown[0].text, "Body:\n  ONE\n  TWO");
        assert_eq!(shown[0].confirm_label, "Approve");
        assert!(shown[0].warning);
    }

    #[cfg(windows)]
    #[test]
    fn the_real_window_config_has_cancel_as_its_default_button() {
        use windows_sys::Win32::UI::Controls::TDF_ALLOW_DIALOG_CANCELLATION;
        let text = DialogText::new(&ReviewText {
            title: "Title".into(),
            text: "Body".into(),
            warning: true,
            confirm_label: "Go".into(),
        });
        let config = text.config(0);
        let (default_button, count, flags) = ({ config.nDefaultButton }, { config.cButtons }, {
            config.dwFlags
        });
        assert_eq!(default_button, CANCEL_BUTTON);
        assert_eq!(count, 2);
        assert_ne!(
            flags & TDF_ALLOW_DIALOG_CANCELLATION,
            0,
            "Escape and close stay available"
        );
        // SAFETY: the config points into `text`, which is alive, and holds two buttons.
        let buttons = unsafe { std::slice::from_raw_parts(config.pButtons, 2) };
        let ids: Vec<i32> = buttons.iter().map(|button| button.nButtonID).collect();
        assert_eq!(ids, [CONFIRM_BUTTON, CANCEL_BUTTON]);
    }

    /// Reads a review's text back. If this is a true inverse, two different names can never
    /// read the same.
    fn decoded(shown: &str) -> Option<String> {
        let mut out = String::new();
        let mut chars = shown.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next()? {
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            }
        }
        Some(out)
    }

    const AWKWARD: [char; 18] = [
        '\\',
        '"',
        'u',
        '{',
        '}',
        '2',
        'e',
        'a',
        ' ',
        '\n',
        '\u{202e}',
        '\u{200b}',
        '\u{feff}',
        '\u{a0}',
        '\u{3164}',
        '\u{fe0f}',
        '\u{e0001}',
        '\u{1f511}',
    ];

    fn every_awkward_string(longest: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut last = vec![String::new()];
        for _ in 0..longest {
            let mut next = Vec::new();
            for prefix in &last {
                for c in AWKWARD {
                    next.push(format!("{prefix}{c}"));
                }
            }
            all.extend(next.iter().cloned());
            last = next;
        }
        all
    }

    #[test]
    fn two_different_names_never_give_the_same_review_text() {
        let mut seen = std::collections::HashMap::new();
        for name in every_awkward_string(3) {
            let shown = review_name(&name);
            assert_eq!(decoded(&shown).as_deref(), Some(name.as_str()), "{shown}");
            if let Some(other) = seen.insert(shown.clone(), name.clone()) {
                panic!("{name:?} and {other:?} both read {shown:?}");
            }
        }
    }

    #[test]
    fn invisible_and_format_characters_are_never_shown_as_themselves() {
        for hidden in [
            '\u{ad}',
            '\u{34f}',
            '\u{61c}',
            '\u{115f}',
            '\u{1160}',
            '\u{17b4}',
            '\u{17b5}',
            '\u{180e}',
            '\u{200b}',
            '\u{200c}',
            '\u{200d}',
            '\u{200e}',
            '\u{200f}',
            '\u{2028}',
            '\u{2029}',
            '\u{202a}',
            '\u{202e}',
            '\u{202f}',
            '\u{205f}',
            '\u{2060}',
            '\u{2066}',
            '\u{206f}',
            '\u{2800}',
            '\u{3000}',
            '\u{3164}',
            '\u{fe00}',
            '\u{fe0f}',
            '\u{feff}',
            '\u{ffa0}',
            '\u{fff9}',
            '\u{1d173}',
            '\u{e0001}',
            '\u{e0100}',
            '\u{e000}',
            '\u{f0000}',
            '\u{10ffff}',
        ] {
            let name = format!("a{hidden}b");
            let shown = review_name(&name);
            assert!(!shown.contains(hidden), "{:x}: {shown}", hidden as u32);
            assert!(
                shown.contains(&format!("\\u{{{:x}}}", hidden as u32)),
                "{shown}"
            );
        }
    }

    #[test]
    fn a_space_that_could_hide_is_shown_as_an_escape() {
        assert_eq!(review_name(" a"), "\\u{20}a");
        assert_eq!(review_name("a "), "a\\u{20}");
        assert_eq!(review_name("a  b"), "a \\u{20}b");
        assert_eq!(review_name("a b"), "a b");
    }

    #[test]
    fn a_name_stays_on_its_line_and_inside_its_quotes() {
        let shown = review_name("demo\"\nFolder: C:\\trusted\u{202e}");
        assert_eq!(shown, "demo\\\"\\u{a}Folder: C:\\\\trusted\\u{202e}");
    }

    #[test]
    fn ordinary_names_are_unchanged_and_a_long_one_is_shown_whole() {
        assert_eq!(review_name("my-app (staging)"), "my-app (staging)");
        assert_eq!(review_name("Müller 東京"), "Müller 東京");
        let long = "x".repeat(500);
        assert_eq!(review_name(&long), long);
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
