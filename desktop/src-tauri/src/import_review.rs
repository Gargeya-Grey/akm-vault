use akm_core::ImportPreview;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::error::CommandError;

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

    /// Names, entry titles and counts only. A secret value never reaches this text.
    pub fn message(&self) -> String {
        let mut text = format!(
            "Import into project \"{}\", environment \"{}\"?\n",
            self.project, self.environment
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
                text.push_str(&format!("  {}\n", change.name));
            }
        }
        let replaced = listed("update");
        if !replaced.is_empty() {
            text.push_str(&format!("\nReplace the secret of {}:\n", replaced.len()));
            for change in replaced {
                text.push_str(&format!("  {} (entry \"{}\")\n", change.name, change.title));
            }
        }
        if self.preview.unchanged > 0 {
            text.push_str(&format!(
                "\nKeep unchanged: {} (not listed).\n",
                self.preview.unchanged
            ));
        }
        text.push_str("\nSecret values are not shown.");
        text
    }
}

/// The only authority to apply an import. It answers on the user's own click in a
/// window that the webview cannot script.
pub trait ImportConfirm: Send + Sync {
    fn confirm(&self, review: &ImportReview) -> bool;
}

pub struct ConfirmState(pub Arc<dyn ImportConfirm>);

pub struct NativeConfirm<R: tauri::Runtime>(pub AppHandle<R>);

impl<R: tauri::Runtime> ImportConfirm for NativeConfirm<R> {
    fn confirm(&self, review: &ImportReview) -> bool {
        let mut dialog = self
            .0
            .dialog()
            .message(review.message())
            .title(review.title())
            .kind(if review.replaces_secrets() {
                MessageDialogKind::Warning
            } else {
                MessageDialogKind::Info
            })
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Import".into(),
                "Cancel".into(),
            ));
        if let Some(window) = self.0.get_webview_window("main") {
            dialog = dialog.parent(&window);
        }
        dialog.blocking_show()
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
             Secret values are not shown."
        );
        assert!(review.replaces_secrets());
        assert!(review.changes_anything());
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
             Secret values are not shown."
        );
        assert!(!review.replaces_secrets());
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
