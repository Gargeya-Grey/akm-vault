//! Core review 1, finding 10 (FR-17): one documented caseless rule, `model::caseless`, for
//! every title comparison. Live and recycled selection use it, and so do search and the CLI
//! listing order.

use akm_core::model::caseless;
use akm_core::{ApiKeyInput, Error, Vault};

const PASSWORD: &str = "fake-caseless-password";

fn vault(titles: &[&str]) -> (tempfile::TempDir, Vault) {
    let directory = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(directory.path().join("vault.kdbx"), PASSWORD, None).unwrap();
    for (index, title) in titles.iter().enumerate() {
        let mut input = ApiKeyInput::new(*title, format!("fake-secret-{index}"));
        input.env_var = Some(format!("CASELESS_{index}_KEY"));
        vault.add(input).unwrap();
    }
    (directory, vault)
}

/// The rule, as a table. Equal rows fold to one text; the last rows stay apart on purpose.
#[test]
fn the_caseless_rule_joins_case_pairs_and_keeps_other_letters_apart() {
    let equal: &[&[&str]] = &[
        &["ΟΔΟΣ", "οδος", "οδοσ", "Οδος"],
        &["Σ", "σ", "ς"],
        &["Éclair", "éclair", "ÉCLAIR"],
        &["Straße", "STRAẞE", "straße"],
        &["\u{212A}ey", "Key", "KEY"],
        &["Ꭰ", "ꭰ"],
        &["ǅ", "Ǆ", "ǆ"],
    ];
    for group in equal {
        for text in *group {
            assert_eq!(
                caseless(text),
                caseless(group[0]),
                "{text} and {}",
                group[0]
            );
        }
    }
    let apart = [
        ("Straße", "STRASSE"),
        ("Straße", "strasse"),
        ("e\u{301}", "é"),
    ];
    for (one, other) in apart {
        assert_ne!(caseless(one), caseless(other), "{one} and {other}");
    }
}

/// The reviewer's case: a title with a final sigma is selected by its capital letter, on a
/// live key and on a recycled key, and two titles that fold to one text stay ambiguous.
#[test]
fn greek_sigma_titles_are_selected_in_any_case_live_and_recycled_and_twins_stay_ambiguous() {
    let (_directory, mut vault) = vault(&["λόγος", "ΝΟΜΟΣ", "νόμος2", "ΝΌΜΟΣ2"]);
    for selector in ["λόγος", "ΛΌΓΟΣ", "λόγοσ", "Λόγος"] {
        assert_eq!(vault.find(selector).unwrap().title, "λόγος", "{selector}");
    }
    for selector in ["νομος", "νομοσ", "Νομος"] {
        assert_eq!(vault.find(selector).unwrap().title, "ΝΟΜΟΣ", "{selector}");
    }
    assert!(matches!(vault.find("νόμοσ2"), Err(Error::Ambiguous(_, 2))));
    assert!(vault
        .candidates("ΛΌΓ")
        .iter()
        .any(|key| key.title == "λόγος"));

    let id = vault.find("λόγος").unwrap().id;
    vault.remove(&id).unwrap();
    assert!(matches!(vault.find("ΛΌΓΟΣ"), Err(Error::EntryNotFound(_))));
    for selector in ["ΛΌΓΟΣ", "λόγοσ"] {
        assert_eq!(
            vault.find_recycled(selector).unwrap().key.id,
            id,
            "{selector}"
        );
    }
    for title in ["νόμος2", "ΝΌΜΟΣ2"] {
        let twin = vault
            .list()
            .into_iter()
            .find(|key| key.title == title)
            .unwrap()
            .id;
        vault.remove(&twin).unwrap();
    }
    assert!(matches!(
        vault.find_recycled("Νόμοσ2"),
        Err(Error::Ambiguous(_, 2))
    ));
    assert!(vault
        .recycled_candidates("ΛΌΓ")
        .iter()
        .any(|item| item.key.id == id));
}
