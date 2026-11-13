//! Unit 3, finding L2, through the real `akm` binary: a key is selected by its id or its exact
//! title. A partial word never reveals, rotates or deletes a key. A refusal lists candidates
//! with their titles and ids, and never a secret, a note or a URL.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use keepass::config::{DatabaseVersion, KdfConfig};
use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use tempfile::TempDir;

const PASSWORD: &str = "fake-cli-selection-password";

struct Key {
    title: &'static str,
    var: &'static str,
    secret: &'static str,
    notes: Option<&'static str>,
    url: Option<&'static str>,
}

const KEYS: [Key; 5] = [
    Key {
        title: "Alpha",
        var: "ALPHA_KEY",
        secret: "fake-secret-alpha",
        notes: Some("primary payments key, rotate yearly"),
        url: None,
    },
    Key {
        title: "Alpha Staging",
        var: "ALPHA_STAGING_KEY",
        secret: "fake-secret-staging",
        notes: Some("copy of Alpha for tests"),
        url: None,
    },
    Key {
        title: "Gamma",
        var: "GAMMA_KEY",
        secret: "fake-secret-gamma",
        notes: None,
        url: Some("https://alpha.example/private-api"),
    },
    Key {
        title: "Twin",
        var: "TWIN_ONE_KEY",
        secret: "fake-secret-twin-one",
        notes: None,
        url: None,
    },
    Key {
        title: "TWIN",
        var: "TWIN_TWO_KEY",
        secret: "fake-secret-twin-two",
        notes: None,
        url: None,
    },
];

struct Fixture {
    _directory: TempDir,
    vault: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new(keys: &[Key]) -> Self {
        let base = std::env::temp_dir().join("opencode");
        fs::create_dir_all(&base).unwrap();
        let directory = tempfile::Builder::new()
            .prefix("akm-cli-selection-")
            .tempdir_in(base)
            .unwrap();
        let vault = directory.path().join("vault.kdbx");
        let mut db = Database::new();
        db.config.version = DatabaseVersion::KDB4(1);
        db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
        {
            let mut root = db.root_mut();
            let mut group = root.add_group();
            group.name = "API Keys".into();
            for key in keys {
                let mut entry = group.add_entry();
                entry.set_unprotected(fields::TITLE, key.title);
                entry.set_protected(fields::PASSWORD, key.secret);
                entry.set_unprotected("AKM_ENV_VAR", key.var);
                entry.set_unprotected("AKM_PROJECT", "demo");
                entry.set_unprotected("AKM_ENV", "dev");
                if let Some(notes) = key.notes {
                    entry.set_unprotected(fields::NOTES, notes);
                }
                if let Some(url) = key.url {
                    entry.set_unprotected(fields::URL, url);
                }
            }
        }
        db.save(
            &mut fs::File::create(&vault).unwrap(),
            DatabaseKey::new().with_password(PASSWORD),
        )
        .unwrap();
        let root = directory.path().join("journal-root");
        Self {
            _directory: directory,
            vault,
            root,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_akm"));
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("AKM_")
            {
                command.env_remove(name);
            }
        }
        command
            .env("AKM_JOURNAL_ROOT", &self.root)
            .env("AKM_PASSWORD", PASSWORD)
            .stdin(Stdio::null())
            .arg("--vault")
            .arg(&self.vault);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn run_with_input(&self, args: &[&str], input: &str) -> Output {
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn id_of(&self, title: &str) -> String {
        let output = self.run(&["list", "--json"]);
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value
            .as_array()
            .unwrap()
            .iter()
            .find(|key| key["title"] == title)
            .unwrap_or_else(|| panic!("no key titled {title}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn bytes(&self) -> Vec<u8> {
        fs::read(&self.vault).unwrap()
    }

    fn journal_text(&self) -> String {
        let mut text = String::new();
        let mut stack = vec![self.root.clone()];
        while let Some(path) = stack.pop() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                    text.push_str(&fs::read_to_string(&path).unwrap());
                }
            }
        }
        text
    }
}

fn out(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn err(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn every_secret() -> impl Iterator<Item = &'static str> {
    KEYS.iter().map(|key| key.secret)
}

fn assert_refusal_without_private_text(output: &Output, label: &str) {
    assert!(!output.status.success(), "{label}: accepted");
    assert!(output.stdout.is_empty(), "{label}: stdout {}", out(output));
    let text = err(output);
    for secret in every_secret() {
        assert!(!text.contains(secret), "{label}: {text}");
    }
    for private in ["primary payments", "copy of Alpha", "private-api"] {
        assert!(!text.contains(private), "{label}: {text}");
    }
}

#[test]
fn a_partial_word_never_reveals_rotates_or_deletes_and_lists_candidates_without_private_text() {
    let fixture = Fixture::new(&KEYS);
    let before = fixture.bytes();
    let alpha = fixture.id_of("Alpha");
    for selector in [
        "alp",
        "payments",
        "alpha.example",
        "staging",
        "ALPHA STAG",
        "key",
    ] {
        let invocations: [(&str, Vec<&str>, Option<&str>); 6] = [
            ("get reveal", vec!["get", selector, "--reveal"], None),
            (
                "get secret",
                vec!["get", selector, "--field", "secret"],
                None,
            ),
            ("get masked", vec!["get", selector], None),
            (
                "rotate",
                vec!["rotate", selector, "--secret-stdin"],
                Some("fake-new-value\n"),
            ),
            ("rm", vec!["rm", selector, "--yes"], None),
            (
                "purge live",
                vec!["purge", selector, "--yes", "--live"],
                None,
            ),
        ];
        for (label, args, input) in invocations {
            let output = match input {
                Some(input) => fixture.run_with_input(&args, input),
                None => fixture.run(&args),
            };
            let label = format!("{label} {selector:?}");
            assert_refusal_without_private_text(&output, &label);
            assert!(
                err(&output).contains("exact title or id"),
                "{label}: {}",
                err(&output)
            );
        }
    }
    let listed = fixture.run(&["get", "payments"]);
    assert!(err(&listed).contains("\"Alpha\""), "{}", err(&listed));
    assert!(err(&listed).contains(&alpha), "{}", err(&listed));
    assert_eq!(fixture.bytes(), before, "the vault must not change");
    let journal = fixture.journal_text();
    for action in ["reveal-requested", "rotate", "remove", "purge"] {
        assert!(!journal.contains(action), "{action} in {journal}");
    }
}

#[test]
fn an_empty_or_blank_selector_matches_nothing_even_in_a_vault_with_one_key() {
    let single = [Key {
        title: "Only",
        var: "ONLY_KEY",
        secret: "fake-secret-only",
        notes: None,
        url: None,
    }];
    let fixture = Fixture::new(&single);
    for selector in ["", " ", "   "] {
        for args in [
            vec!["get", selector, "--reveal"],
            vec!["get", selector, "--field", "secret"],
            vec!["rm", selector, "--yes"],
        ] {
            let output = fixture.run(&args);
            assert!(!output.status.success(), "{args:?} accepted");
            assert!(
                !out(&output).contains("fake-secret-only")
                    && !err(&output).contains("fake-secret-only"),
                "{args:?}"
            );
        }
    }
    assert!(out(&fixture.run(&["list"])).contains("Only"));
}

#[test]
fn an_id_or_an_exact_title_in_any_case_still_selects_the_key() {
    let fixture = Fixture::new(&KEYS);
    let alpha = fixture.id_of("Alpha");
    for selector in [alpha.as_str(), "Alpha", "alpha", "ALPHA"] {
        let output = fixture.run(&["get", selector, "--field", "secret"]);
        assert!(output.status.success(), "{selector}: {}", err(&output));
        assert_eq!(out(&output), "fake-secret-alpha\n", "{selector}");
    }
    let staging = fixture.run(&["rm", "alpha staging", "--yes"]);
    assert!(staging.status.success(), "{}", err(&staging));
    let listing = out(&fixture.run(&["list"]));
    assert!(!listing.contains("Alpha Staging"), "{listing}");
    assert!(listing.contains("Alpha "), "{listing}");
}

#[test]
fn two_keys_with_the_same_exact_title_are_refused_by_title_and_listed_by_id() {
    let fixture = Fixture::new(&KEYS);
    let twin = fixture.id_of("Twin");
    let upper = fixture.id_of("TWIN");
    let before = fixture.bytes();
    for args in [
        vec!["get", "twin", "--reveal"],
        vec!["get", "twin", "--field", "secret"],
        vec!["rm", "twin", "--yes"],
    ] {
        let output = fixture.run(&args);
        assert_refusal_without_private_text(&output, &format!("{args:?}"));
        let text = err(&output);
        assert!(text.contains("exact title of 2 keys"), "{text}");
        assert!(text.contains(&twin) && text.contains(&upper), "{text}");
    }
    assert_eq!(fixture.bytes(), before);
    let by_id = fixture.run(&["get", &upper, "--field", "secret"]);
    assert_eq!(out(&by_id), "fake-secret-twin-two\n");
}

#[test]
fn a_partial_word_cannot_restore_purge_or_show_the_history_of_a_recycled_key() {
    let fixture = Fixture::new(&KEYS);
    assert!(fixture.run(&["rm", "Gamma", "--yes"]).status.success());
    let before = fixture.bytes();
    for args in [
        vec!["restore", "gam"],
        vec!["purge", "gam", "--yes"],
        vec!["history", "gam"],
        vec!["restore", "alpha.example"],
    ] {
        let output = fixture.run(&args);
        assert_refusal_without_private_text(&output, &format!("{args:?}"));
    }
    assert_eq!(fixture.bytes(), before);
    assert!(fixture.run(&["restore", "gamma"]).status.success());
}

/// FW-04: a recycled key is selected by its id in any letter case, by the same rule as a live
/// key. Its history, its restore and its purge take the id in upper case and in mixed case.
#[test]
fn a_recycled_key_is_selected_by_its_id_in_any_letter_case() {
    let fixture = Fixture::new(&KEYS);
    let gamma = fixture.id_of("Gamma");
    let alpha = fixture.id_of("Alpha");
    for id in [&gamma, &alpha] {
        assert!(fixture.run(&["rm", id, "--yes"]).status.success());
    }
    let upper = gamma.to_uppercase();
    let mut letters = 0;
    let mixed: String = alpha
        .chars()
        .map(|ch| {
            letters += usize::from(ch.is_ascii_alphabetic());
            if letters % 2 == 1 {
                ch.to_ascii_uppercase()
            } else {
                ch
            }
        })
        .collect();
    assert!(
        mixed.chars().any(|ch| ch.is_ascii_uppercase())
            && mixed.chars().any(|ch| ch.is_ascii_lowercase()),
        "{mixed}"
    );

    let history = fixture.run(&["history", &upper]);
    assert!(history.status.success(), "{}", err(&history));
    assert!(out(&history).contains(&gamma), "{}", out(&history));
    let restored = fixture.run(&["restore", &upper]);
    assert!(restored.status.success(), "{}", err(&restored));
    let get = fixture.run(&["get", "Gamma", "--field", "secret"]);
    assert_eq!(out(&get), "fake-secret-gamma\n");
    let purged = fixture.run(&["purge", &mixed, "--yes"]);
    assert!(purged.status.success(), "{}", err(&purged));
    let bin = fixture.run(&["bin"]);
    assert!(
        out(&bin).contains("The Recycle Bin is empty."),
        "{}",
        out(&bin)
    );
}

// ---------------------------------------------------------------------- FR-17: letter case

const UNICODE_KEYS: [Key; 4] = [
    Key {
        title: "Éclair2",
        var: "ECLAIR_KEY",
        secret: "fake-secret-eclair",
        notes: None,
        url: None,
    },
    Key {
        title: "Straße",
        var: "STRASSE_KEY",
        secret: "fake-secret-strasse",
        notes: None,
        url: None,
    },
    Key {
        title: "Ünï",
        var: "UNI_ONE_KEY",
        secret: "fake-secret-uni-one",
        notes: None,
        url: None,
    },
    Key {
        title: "ÜNÏ",
        var: "UNI_TWO_KEY",
        secret: "fake-secret-uni-two",
        notes: None,
        url: None,
    },
];

/// FR-17: a title that has a letter outside ASCII is selected in any letter case, on live and
/// on recycled keys. Two titles that differ only by case are still ambiguous.
#[test]
fn a_title_with_non_ascii_letters_is_selected_in_any_case_and_case_twins_are_ambiguous() {
    let fixture = Fixture::new(&UNICODE_KEYS);
    for selector in ["Éclair2", "éclair2", "ÉCLAIR2", "éCLAIR2"] {
        let output = fixture.run(&["get", selector, "--field", "secret"]);
        assert!(output.status.success(), "{selector}: {}", err(&output));
        assert_eq!(out(&output), "fake-secret-eclair\n", "{selector}");
    }
    let output = fixture.run(&["get", "straße", "--field", "secret"]);
    assert_eq!(out(&output), "fake-secret-strasse\n");
    // A different word is not a match: `ss` is not `ß`.
    let output = fixture.run(&["get", "STRASSE", "--field", "secret"]);
    assert!(!output.status.success());

    let before = fixture.bytes();
    for selector in ["ünï", "Ünï", "ÜNÏ"] {
        let output = fixture.run(&["get", selector, "--field", "secret"]);
        assert!(!output.status.success(), "{selector}");
        assert!(
            err(&output).contains("exact title of 2 keys"),
            "{selector}: {}",
            err(&output)
        );
    }
    assert_eq!(fixture.bytes(), before);

    // Recycled keys use the same rule.
    assert!(fixture.run(&["rm", "éclair2", "--yes"]).status.success());
    let history = fixture.run(&["history", "ÉCLAIR2"]);
    assert!(history.status.success(), "{}", err(&history));
    let restored = fixture.run(&["restore", "éCLAIR2"]);
    assert!(restored.status.success(), "{}", err(&restored));
    let get = fixture.run(&["get", "Éclair2", "--field", "secret"]);
    assert_eq!(out(&get), "fake-secret-eclair\n");
    for title in ["Ünï", "ÜNÏ"] {
        let id = fixture.id_of(title);
        assert!(fixture.run(&["rm", &id, "--yes"]).status.success());
    }
    let before = fixture.bytes();
    let twins = fixture.run(&["restore", "ünï"]);
    assert!(!twins.status.success());
    assert!(
        err(&twins).contains("exact title of 2 keys"),
        "{}",
        err(&twins)
    );
    assert_eq!(fixture.bytes(), before);
}

/// Core review 1, finding 10, through the binary: the reviewer's final sigma. A key titled with
/// `ς` is selected by `Σ` and `σ`, live and recycled. Nothing is written by a read.
#[test]
fn a_final_sigma_title_is_selected_by_its_capital_live_and_recycled() {
    let fixture = Fixture::new(&[Key {
        title: "Λόγος",
        var: "LOGOS_KEY",
        secret: "fake-secret-logos",
        notes: None,
        url: None,
    }]);
    let before = fixture.bytes();
    for selector in ["ΛΌΓΟΣ", "λόγοσ", "λόγος"] {
        let output = fixture.run(&["get", selector, "--field", "secret"]);
        assert!(output.status.success(), "{selector}: {}", err(&output));
        assert_eq!(out(&output), "fake-secret-logos\n", "{selector}");
    }
    assert_eq!(fixture.bytes(), before);
    assert!(fixture.run(&["rm", "ΛΌΓΟΣ", "--yes"]).status.success());
    let restored = fixture.run(&["restore", "λόγοσ"]);
    assert!(restored.status.success(), "{}", err(&restored));
}
