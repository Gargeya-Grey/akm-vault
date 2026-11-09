//! The native approval that a disclosure needs when a request from the page is no proof of a
//! user gesture (finding FR-03).
//!
//! A page can read its session ID, so a call that names the session proves nothing about the
//! user. A bulk request (`env_export`, `env_copy`) therefore always needs the user's click in a
//! native window, and a single-key request (`key_reveal`, `key_copy`) needs it once a session
//! has made `FREE_SINGLE_DISCLOSURES` requests in a rolling minute. The window shows the scope
//! and the names, never a value. `ask` takes no gate: the window is modal. The caller reads the
//! vault again after the answer, bound to the session, the revision and the presentation of the
//! request, so a lock, a change or a hide during the window refuses the disclosure.

use akm_core::KeyDisclosure;

use crate::{
    error::CommandError,
    import_review::{ImportConfirm, MAX_REVIEWED_CHANGES},
    policy::Ticket,
    review_window::{check_shown_in_full, review_name, review_names, ReviewAnswer},
    session::{Session, DISCLOSURE_WINDOW, FREE_SINGLE_DISCLOSURES},
};

/// The user's answer to a review that did show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    Approved,
    Declined,
}

/// What a bulk request does with the values, in the words of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkUse {
    /// The values appear in the AKM window.
    Preview,
    /// The values go to the system clipboard.
    Clipboard,
}

/// The text of one disclosure review. It holds names and counts and never a value.
pub struct DisclosureReview {
    title: &'static str,
    body: String,
    names: Vec<String>,
    confirm: &'static str,
}

impl DisclosureReview {
    /// A single key that went over the budget of the session.
    pub fn single(action: KeyDisclosure, key_title: &str) -> Result<Self, CommandError> {
        check_shown_in_full([key_title])?;
        let (title, what, confirm) = if matches!(action, KeyDisclosure::Copy) {
            (
                "Copy a secret value?",
                "put the value of this key on the system clipboard, where other programs can read it until AKM clears it",
                "Copy value",
            )
        } else {
            (
                "Show a secret value?",
                "show the value of this key",
                "Show value",
            )
        };
        Ok(Self {
            title,
            body: format!(
                "This session already showed or copied {FREE_SINGLE_DISCLOSURES} keys in the last {} seconds. Do you want to {what}?\n\nOnly a click in this window can approve it. Secret values are not shown here.\n\nThe key:",
                DISCLOSURE_WINDOW.as_secs()
            ),
            names: vec![key_title.to_string()],
            confirm,
        })
    }

    /// Every variable of a scope. `scope` is `None` for all keys of the vault. A list longer
    /// than one window can hold is shown in part, and the text says so before the list.
    pub fn bulk(
        purpose: BulkUse,
        scope: Option<(&str, &str)>,
        names: Vec<String>,
    ) -> Result<Self, CommandError> {
        check_shown_in_full(
            names.iter().map(String::as_str).chain(
                scope
                    .into_iter()
                    .flat_map(|(project, environment)| [project, environment]),
            ),
        )?;
        let count = names.len();
        let plural = if count == 1 { "" } else { "s" };
        let scope_text = match scope {
            Some((project, environment)) => format!(
                "project \"{}\", environment \"{}\"",
                review_name(project),
                review_name(environment)
            ),
            None => "ALL keys of the vault".to_string(),
        };
        let (title, action, effect, confirm) = match purpose {
            BulkUse::Preview => (
                "Show these secret values?",
                "Show",
                "The values appear in the AKM window and hide after 30 seconds.",
                "Show values",
            ),
            BulkUse::Clipboard => (
                "Copy these secret values?",
                "Copy",
                "The values go to the system clipboard, where other programs can read them until AKM clears it. AKM tries to clear it, and cannot promise that it works.",
                "Copy values",
            ),
        };
        let listed = names.len().min(MAX_REVIEWED_CHANGES);
        let partial = if count > listed {
            format!("\nThis window lists the first {listed} of {count} names. The other names are not listed.")
        } else {
            String::new()
        };
        Ok(Self {
            title,
            body: format!(
                "{action} the values of {count} variable{plural} of {scope_text}?\n\n{effect}\nOnly a click in this window can approve it. Secret values are not shown here.{partial}\n\nThe variables:"
            ),
            names: names.into_iter().take(listed).collect(),
            confirm,
        })
    }

    /// Starting a new journal after the old one was found damaged (finding FR-14). `file` is
    /// the name of the damaged journal file, shown as the one name of the list.
    pub fn recovery(file: &str) -> Result<Self, CommandError> {
        check_shown_in_full([file])?;
        Ok(Self {
            title: "Start a new activity journal?",
            body: "The activity journal of this vault is damaged. Key reveal, copy and export stay blocked until a new journal starts.\n\nIf you continue, AKM keeps the damaged file unchanged beside it, under a new name with the date. Then it starts a new journal. The first record of the new journal states the gap and the SHA-256 of the kept file.\n\nHistory before the damage cannot be proved complete. Only a click in this window can approve it.\n\nThe damaged journal:".to_string(),
            names: vec![file.to_string()],
            confirm: "Start new journal",
        })
    }

    #[cfg(test)]
    pub fn body(&self) -> &str {
        &self.body
    }

    #[cfg(test)]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Shows the review and answers with the click. An error is never a decline: a window that
/// could not be shown is `reviewFailed`, a window nobody sees is `viewChanged`, and another
/// open window is `busy`. No gate is held while the window is open. The session is checked
/// before the window and again after it.
pub fn ask(
    session: &Session,
    host: &dyn ImportConfirm,
    ticket: Ticket,
    review: &DisclosureReview,
) -> Result<Approval, CommandError> {
    let _only_window = session.open_review()?;
    if !host.window_live() {
        return Err(CommandError::new(
            "viewChanged",
            "The AKM window is not shown, so no confirmation window was opened. Nothing was disclosed. Start it again.",
        ));
    }
    session.ensure(ticket)?;
    let dialog = host.review_dialog();
    let answer = review_names(
        dialog.as_ref(),
        review.title,
        &review.body,
        &review.names,
        review.confirm,
        true,
    );
    session.ensure(ticket)?;
    match answer {
        ReviewAnswer::Approved => Ok(Approval::Approved),
        ReviewAnswer::Declined => Ok(Approval::Declined),
        ReviewAnswer::Failed(failure) => Err(failure.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_review_names_the_key_and_the_budget_and_no_value() {
        let review = DisclosureReview::single(KeyDisclosure::Reveal, "OpenAI dev").unwrap();
        assert_eq!(
            review.body(),
            "This session already showed or copied 5 keys in the last 60 seconds. Do you want to show the value of this key?\n\nOnly a click in this window can approve it. Secret values are not shown here.\n\nThe key:"
        );
        assert_eq!(review.names(), ["OpenAI dev"]);
        let copy = DisclosureReview::single(KeyDisclosure::Copy, "OpenAI dev").unwrap();
        assert!(copy.body().contains("system clipboard"));
        assert_eq!(copy.confirm, "Copy value");
        assert_eq!(review.confirm, "Show value");
    }

    #[test]
    fn a_bulk_review_states_the_scope_the_count_and_each_name() {
        let review = DisclosureReview::bulk(
            BulkUse::Preview,
            Some(("app", "dev")),
            vec!["ONE".into(), "TWO".into()],
        )
        .unwrap();
        assert_eq!(
            review.body(),
            "Show the values of 2 variables of project \"app\", environment \"dev\"?\n\nThe values appear in the AKM window and hide after 30 seconds.\nOnly a click in this window can approve it. Secret values are not shown here.\n\nThe variables:"
        );
        assert_eq!(review.names(), ["ONE", "TWO"]);
        let all = DisclosureReview::bulk(BulkUse::Clipboard, None, vec!["ONE".into()]).unwrap();
        assert!(all
            .body()
            .starts_with("Copy the values of 1 variable of ALL keys of the vault?"));
        assert!(all.body().contains("cannot promise"));
    }

    #[test]
    fn a_recovery_review_states_the_block_the_kept_file_and_the_gap_and_shows_no_value() {
        let review = DisclosureReview::recovery("j-0123.jsonl").unwrap();
        for part in [
            "stay blocked",
            "keeps the damaged file unchanged",
            "SHA-256 of the kept file",
            "cannot be proved complete",
            "Only a click in this window can approve it",
        ] {
            assert!(review.body().contains(part), "{part}: {}", review.body());
        }
        assert_eq!(review.names(), ["j-0123.jsonl"]);
        assert_eq!(review.confirm, "Start new journal");
        let long = "x".repeat(400);
        assert!(DisclosureReview::recovery(&long).is_err());
    }

    #[test]
    fn a_list_longer_than_a_window_says_so_before_the_list() {
        let names: Vec<String> = (0..40).map(|n| format!("NAME_{n}")).collect();
        let review = DisclosureReview::bulk(BulkUse::Preview, Some(("app", "dev")), names).unwrap();
        assert_eq!(review.names().len(), MAX_REVIEWED_CHANGES);
        assert!(review.body().contains("lists the first 25 of 40 names"));
        assert!(
            review.body().find("lists the first").unwrap()
                < review.body().find("The variables:").unwrap()
        );
    }

    #[test]
    fn hostile_text_cannot_pass_for_a_line_of_the_review() {
        let review = DisclosureReview::bulk(
            BulkUse::Preview,
            Some(("app\"\nSecret values are not shown.", "dev\u{202e}")),
            vec!["NAME\nThe variables:".into()],
        )
        .unwrap();
        assert!(
            !review
                .body()
                .contains("\nSecret values are not shown.\n\nThe variables:\n"),
            "{}",
            review.body()
        );
        assert!(!review.body().contains('\u{202e}'));
        assert_eq!(
            crate::review_window::review_text(review.body(), review.names())
                .lines()
                .filter(|line| *line == "The variables:")
                .count(),
            1
        );
    }

    #[test]
    fn a_name_too_long_to_show_in_full_is_refused_before_any_window() {
        let long = "x".repeat(400);
        for (scope, names) in [
            (Some((long.as_str(), "dev")), vec!["A".to_string()]),
            (Some(("app", long.as_str())), vec!["A".to_string()]),
            (Some(("app", "dev")), vec![long.clone()]),
        ] {
            let error = DisclosureReview::bulk(BulkUse::Preview, scope, names)
                .err()
                .unwrap();
            assert_eq!(error.kind, "tooLarge");
            assert!(!error.message.contains(&long));
        }
        assert!(DisclosureReview::single(KeyDisclosure::Reveal, &long).is_err());
    }
}
