use akm_core::ImportPreview;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::{
    error::CommandError,
    review_window::{
        check_shown_in_full, review_name, review_names, NativeReviewDialog, ReviewAnswer,
        ReviewDialog,
    },
};

/// What the review says about every name that it lists (FR-02, core review 2). No rule can
/// tell from a name whether a program reads its value as a secret, a file or a setting, so the
/// review sorts no name into a safer group. The person who clicks is the check.
pub const NAME_CAUTION: &str = "A program that you start with these keys gets each name as a variable, and can read it as a setting. Import only the names that you expect.";

/// One dialog lists every change. A bigger import is refused instead of cut short,
/// so the user never approves names that the dialog did not show.
pub const MAX_REVIEWED_CHANGES: usize = 25;

pub struct ImportReview {
    project: String,
    environment: String,
    preview: ImportPreview,
}

impl ImportReview {
    pub fn new(
        project: String,
        environment: String,
        preview: ImportPreview,
    ) -> Result<Self, CommandError> {
        if preview.added + preview.updated > MAX_REVIEWED_CHANGES {
            return Err(CommandError::new(
                "tooLarge",
                format!(
                    "This import changes more than {MAX_REVIEWED_CHANGES} keys. Split it into smaller .env files so that every change can be reviewed."
                ),
            ));
        }
        check_shown_in_full(
            [project.as_str(), environment.as_str()].into_iter().chain(
                preview
                    .changes
                    .iter()
                    .flat_map(|change| [change.name.as_str(), change.title.as_str()]),
            ),
        )?;
        Ok(Self {
            project,
            environment,
            preview,
        })
    }

    pub fn preview(&self) -> &ImportPreview {
        &self.preview
    }

    pub fn changes_anything(&self) -> bool {
        self.preview.added + self.preview.updated > 0
    }

    pub fn replaces_secrets(&self) -> bool {
        self.preview.updated > 0
    }

    pub fn title(&self) -> &'static str {
        "Confirm import into the vault"
    }

    /// Shows the review and waits for the click. No gate is held. The write that follows
    /// checks the session and the revision again.
    pub fn ask(&self, dialog: &dyn ReviewDialog) -> ReviewAnswer {
        review_names(
            dialog,
            self.title(),
            &self.message(),
            &[],
            "Import",
            self.replaces_secrets(),
        )
    }

    /// Names, entry titles and counts only. A secret value never reaches this text.
    pub fn message(&self) -> String {
        let mut text = format!(
            "Import into project \"{}\", environment \"{}\"?\n",
            review_name(&self.project),
            review_name(&self.environment)
        );
        let listed = |action: &str| {
            self.preview
                .changes
                .iter()
                .filter(|change| change.action == action)
                .collect::<Vec<_>>()
        };
        let added = listed("add");
        if !added.is_empty() {
            text.push_str(&format!("\nAdd {}:\n", added.len()));
            for change in added {
                text.push_str(&format!("  {}\n", review_name(&change.name)));
            }
        }
        let replaced = listed("update");
        if !replaced.is_empty() {
            text.push_str(&format!("\nReplace the secret of {}:\n", replaced.len()));
            for change in replaced {
                text.push_str(&format!(
                    "  {} (entry \"{}\")\n",
                    review_name(&change.name),
                    review_name(&change.title)
                ));
            }
        }
        if self.preview.unchanged > 0 {
            text.push_str(&format!(
                "\nKeep unchanged: {} (not listed).\n",
                self.preview.unchanged
            ));
        }
        if self.changes_anything() {
            text.push_str(&format!("\n{NAME_CAUTION}\n"));
        }
        text.push_str("\nSecret values are not shown.");
        text
    }
}

/// The only authority to apply an import. It answers on the user's own click in a
/// window that the webview cannot script.
pub trait ImportConfirm: Send + Sync {
    fn review_dialog(&self) -> Box<dyn ReviewDialog + '_>;
    /// Whether the main window is shown and not minimized right now. Native asks it only
    /// before the dialog opens, so that no dialog appears over a window nobody can see. It
    /// does not gate the answer: the user's click in the dialog stands.
    fn window_live(&self) -> bool;
    /// Runs right after a recovery moved the damaged journal, inside its vault access.
    /// Production does nothing here. A test locks there.
    fn journal_moved(&self) {}
}

pub struct ConfirmState(pub Arc<dyn ImportConfirm>);

pub struct NativeConfirm<R: tauri::Runtime>(pub AppHandle<R>);

impl<R: tauri::Runtime> ImportConfirm for NativeConfirm<R> {
    fn review_dialog(&self) -> Box<dyn ReviewDialog + '_> {
        Box::new(NativeReviewDialog(self.0.clone()))
    }

    fn window_live(&self) -> bool {
        self.0.get_webview_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akm_core::model::ImportChange;

    fn change(name: &str, title: &str, action: &str) -> ImportChange {
        ImportChange {
            name: name.into(),
            title: title.into(),
            action: action.into(),
        }
    }

    #[test]
    fn the_dialog_lists_scope_names_and_counts_but_no_value() {
        let review = ImportReview::new(
            "app".into(),
            "dev".into(),
            ImportPreview {
                added: 2,
                updated: 1,
                unchanged: 3,
                changes: vec![
                    change("NEW_ONE", "NEW_ONE", "add"),
                    change("OLD_ONE", "OpenAI dev", "update"),
                    change("NEW_TWO", "NEW_TWO", "add"),
                    change("SAME", "Same key", "unchanged"),
                ],
            },
        )
        .unwrap();
        assert_eq!(
            review.message(),
            "Import into project \"app\", environment \"dev\"?\n\
             \n\
             Add 2:\n  NEW_ONE\n  NEW_TWO\n\
             \n\
             Replace the secret of 1:\n  OLD_ONE (entry \"OpenAI dev\")\n\
             \n\
             Keep unchanged: 3 (not listed).\n\
             \n\
             A program that you start with these keys gets each name as a variable, and can read it as a setting. Import only the names that you expect.\n\
             \n\
             Secret values are not shown."
        );
        assert!(review.replaces_secrets());
        assert!(review.changes_anything());
    }

    #[test]
    fn hostile_names_cannot_pass_for_a_line_of_the_import_review() {
        let review = ImportReview::new(
            "demo\"\nKeep unchanged: 0".into(),
            "dev\u{202e}".into(),
            ImportPreview {
                added: 0,
                updated: 1,
                unchanged: 0,
                changes: vec![change("OLD", "title\nAdd 9:", "update")],
            },
        )
        .unwrap();
        let message = review.message();
        assert!(!message.contains("\nKeep unchanged"), "{message}");
        assert!(!message.contains("\nAdd 9:"), "{message}");
        assert!(!message.contains('\u{202e}'), "{message}");
    }

    #[test]
    fn a_name_too_long_to_show_in_full_is_refused_before_the_import_review() {
        let long = "x".repeat(400);
        let preview = |name: &str, title: &str| ImportPreview {
            added: 1,
            updated: 0,
            unchanged: 0,
            changes: vec![change(name, title, "add")],
        };
        for (project, environment, name, title) in [
            (long.as_str(), "dev", "OK", "OK"),
            ("app", long.as_str(), "OK", "OK"),
            ("app", "dev", long.as_str(), "OK"),
            ("app", "dev", "OK", long.as_str()),
        ] {
            let error = ImportReview::new(
                project.to_string(),
                environment.to_string(),
                preview(name, title),
            )
            .err()
            .unwrap();
            assert_eq!(error.kind, "tooLarge");
            assert!(!error.message.contains(&long));
        }
    }

    #[test]
    fn empty_sections_are_left_out() {
        let review = ImportReview::new(
            "app".into(),
            "prod".into(),
            ImportPreview {
                added: 1,
                updated: 0,
                unchanged: 0,
                changes: vec![change("ONLY", "ONLY", "add")],
            },
        )
        .unwrap();
        assert_eq!(
            review.message(),
            "Import into project \"app\", environment \"prod\"?\n\
             \n\
             Add 1:\n  ONLY\n\
             \n\
             A program that you start with these keys gets each name as a variable, and can read it as a setting. Import only the names that you expect.\n\
             \n\
             Secret values are not shown."
        );
        assert!(!review.replaces_secrets());
    }

    /// FR-02 and core review 2: the review lists every name in the same way, whatever its
    /// shape, and says once what a name does. A name that looks like a credential gets no
    /// other treatment than a file location or a plain setting.
    #[test]
    fn every_name_is_listed_the_same_way_and_the_review_says_what_a_name_does() {
        let names = [
            "OPENAI_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "AWS_SHARED_CREDENTIALS_FILE",
            "DATABASE_URL",
            "DEBUG",
            "db_password",
        ];
        for action in ["add", "update"] {
            let mut lines = Vec::new();
            for name in names {
                let review = ImportReview::new(
                    "app".into(),
                    "dev".into(),
                    ImportPreview {
                        added: usize::from(action == "add"),
                        updated: usize::from(action == "update"),
                        unchanged: 0,
                        changes: vec![change(name, "Same title", action)],
                    },
                )
                .unwrap();
                let message = review.message();
                assert_eq!(message.matches(NAME_CAUTION).count(), 1, "{message}");
                assert_eq!(message.matches(name).count(), 1, "{message}");
                // The same text for every name, with only the name changed.
                lines.push(message.replace(name, "NAME"));
            }
            assert!(
                lines.windows(2).all(|pair| pair[0] == pair[1]),
                "{lines:#?}"
            );
        }
        // A review that changes nothing lists no name, so it has no caution.
        let nothing = ImportReview::new(
            "app".into(),
            "dev".into(),
            ImportPreview {
                added: 0,
                updated: 0,
                unchanged: 2,
                changes: vec![change("SAME", "Same", "unchanged")],
            },
        )
        .unwrap();
        assert!(!nothing.message().contains(NAME_CAUTION));
    }

    #[test]
    fn a_review_over_the_limit_is_refused_instead_of_cut_short() {
        let big = |added, updated| ImportPreview {
            added,
            updated,
            unchanged: 0,
            changes: Vec::new(),
        };
        assert!(ImportReview::new("p".into(), "e".into(), big(25, 0)).is_ok());
        assert!(ImportReview::new("p".into(), "e".into(), big(20, 5)).is_ok());
        let error = ImportReview::new("p".into(), "e".into(), big(20, 6))
            .err()
            .unwrap();
        assert_eq!(error.kind, "tooLarge");
        assert!(error.message.contains("25"));
    }
}
