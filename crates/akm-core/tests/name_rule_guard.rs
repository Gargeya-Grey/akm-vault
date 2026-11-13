//! CORE-R1-02 and CORE-R2-02, class sweep: `model::reserved_class` is the only rule that
//! decides from a variable name how AKM treats it. AKM has no credential class, so a rule
//! that treats a name with a credential word apart is a second rule too. This guard fails
//! when a second rule appears in the Rust sources of the CLI, the core or the desktop.
//!
//! How it finds one: it reads every `.rs` file under `crates/akm-core/src`,
//! `crates/akm-cli/src` and `desktop/src-tauri/src`, except `model.rs`, without the blocks
//! of its `#[cfg(test)]` modules and without comments. It takes each string literal that
//! holds only ASCII upper-case letters, digits and underscores (the shape of a variable
//! name or of a part of one, such as `"AKM_"` or `"_TOKEN"`). A literal that
//! `reserved_class` reserves, or that holds a credential word as a whole token, is a rule,
//! unless it is the
//! argument of a read of the process environment (`var(`, `var_os(`, `env!(`, a clap
//! `env = `), or the allow list below names its file and literal with the reason why it is
//! no rule. It also fails on a function with the word "reserved" or "credential" in its
//! name. A rule that builds its names from lower-case text, or from bytes, is outside its
//! reach.

use std::path::{Path, PathBuf};

use akm_core::model::reserved_class;

/// Words that a second rule could use to treat a name as a credential. The core uses none of
/// them (CORE-R2-02); this list exists only so that the guard can find such a rule.
const CREDENTIAL_WORDS: &[&str] = &[
    "KEY",
    "KEYS",
    "APIKEY",
    "TOKEN",
    "TOKENS",
    "SECRET",
    "SECRETS",
    "PASSWORD",
    "PASSWD",
    "PASSPHRASE",
    "PASS",
    "CREDENTIAL",
    "CREDENTIALS",
];

/// Why a literal is a name rule, or `None` when it is not one.
fn rule_of(literal: &str) -> Option<String> {
    if let Some(class) = reserved_class(literal) {
        return Some(format!("{literal:?} is reserved ({class:?})"));
    }
    literal
        .split('_')
        .any(|token| CREDENTIAL_WORDS.contains(&token))
        .then(|| format!("{literal:?} holds a credential word"))
}

/// (file under the workspace root, literal, why it is no second rule).
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "crates/akm-core/src/journal.rs",
        "AKM_JOURNAL_ROOT",
        "the name of a setting that AKM reads; it decides nothing about a stored name",
    ),
    (
        "crates/akm-core/src/journal.rs",
        "LOCALAPPDATA",
        "the label of the folder in an error message",
    ),
    (
        "crates/akm-core/src/backups.rs",
        "AKM_BACKUPS",
        "a KDBX custom-data key, not a variable name",
    ),
    (
        "crates/akm-cli/src/main.rs",
        "ENV",
        "a column heading of `akm list`",
    ),
    (
        "crates/akm-cli/src/child.rs",
        "AKM_",
        "is_akm_name, which removes inherited AKM_ variables from a child: the desktop test \
         the_akm_name_rule_mirrors_the_one_of_the_command_line pins its text, and the child.rs \
         test the_akm_name_rule_is_the_akm_internal_class_of_reserved_class holds it to \
         reserved_class",
    ),
    (
        "desktop/src-tauri/src/launch.rs",
        "AKM_",
        "is_akm_name, which removes inherited AKM_ variables from the terminal: the launch.rs \
         test the_akm_name_rule_mirrors_the_one_of_the_command_line pins its text to the one \
         of child.rs and holds it to reserved_class",
    ),
];

const ROOTS: &[&str] = &[
    "crates/akm-core/src",
    "crates/akm-cli/src",
    "desktop/src-tauri/src",
];

const THE_ONE_RULE: &str = "crates/akm-core/src/model.rs";

/// Code just before a literal that makes it the name of a read of the process environment.
const ENVIRONMENT_READS: &[&str] = &["var(", "var_os(", "env!(", "option_env!(", "env ="];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn rust_files(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// One piece of Rust source: code, a comment, or a string literal with its text.
#[derive(Debug, PartialEq)]
enum Piece {
    Code(char),
    Comment,
    Literal(String),
}

fn pieces(source: &str) -> Vec<Piece> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let word_before =
        |at: usize| at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
    while at < chars.len() {
        match chars[at] {
            '/' if chars.get(at + 1) == Some(&'/') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                out.push(Piece::Comment);
            }
            '/' if chars.get(at + 1) == Some(&'*') => {
                at += 2;
                while at + 1 < chars.len() && !(chars[at] == '*' && chars[at + 1] == '/') {
                    at += 1;
                }
                at += 2;
                out.push(Piece::Comment);
            }
            '\'' if chars.get(at + 1) == Some(&'\\') => {
                at += 2;
                while at < chars.len() && chars[at] != '\'' {
                    at += 1;
                }
                at += 1;
                out.push(Piece::Code('\''));
            }
            '\'' if chars.get(at + 2) == Some(&'\'') => {
                at += 3;
                out.push(Piece::Code('\''));
            }
            'r' if matches!(chars.get(at + 1), Some('"') | Some('#')) && !word_before(at) => {
                let mut hashes = 0;
                let mut cursor = at + 1;
                while chars.get(cursor) == Some(&'#') {
                    hashes += 1;
                    cursor += 1;
                }
                if chars.get(cursor) != Some(&'"') {
                    out.push(Piece::Code('r'));
                    at += 1;
                    continue;
                }
                cursor += 1;
                let start = cursor;
                while cursor < chars.len()
                    && !(chars[cursor] == '"'
                        && (0..hashes).all(|offset| chars.get(cursor + 1 + offset) == Some(&'#')))
                {
                    cursor += 1;
                }
                out.push(Piece::Literal(
                    chars[start..cursor.min(chars.len())].iter().collect(),
                ));
                at = cursor + 1 + hashes;
            }
            '"' => {
                let mut cursor = at + 1;
                let mut literal = String::new();
                while cursor < chars.len() && chars[cursor] != '"' {
                    if chars[cursor] == '\\' {
                        cursor += 1;
                    }
                    if let Some(letter) = chars.get(cursor) {
                        literal.push(*letter);
                    }
                    cursor += 1;
                }
                out.push(Piece::Literal(literal));
                at = cursor + 1;
            }
            letter => {
                out.push(Piece::Code(letter));
                at += 1;
            }
        }
    }
    out
}

/// The pieces of a file without the blocks of its `#[cfg(test)]` modules.
fn production_pieces(source: &str) -> Vec<Piece> {
    let all = pieces(source);
    let code: String = all
        .iter()
        .map(|piece| match piece {
            Piece::Code(letter) => *letter,
            Piece::Comment | Piece::Literal(_) => ' ',
        })
        .collect();
    let mut drop = vec![false; all.len()];
    let mut search = 0;
    while let Some(found) = code[search..].find("#[cfg(test)]") {
        let start = search + found;
        search = start + 1;
        let rest = code[start + "#[cfg(test)]".len()..].trim_start();
        let mut item = rest;
        while item.starts_with("#[") {
            item = item[item.find(']').map_or(item.len(), |end| end + 1)..].trim_start();
        }
        let item = item
            .strip_prefix("pub")
            .map(|after| {
                let after = after.trim_start();
                if after.starts_with('(') {
                    after[after.find(')').map_or(after.len(), |end| end + 1)..].trim_start()
                } else {
                    after
                }
            })
            .unwrap_or(item);
        if !item.starts_with("mod ") {
            continue;
        }
        let item_at = code.len() - item.len();
        let Some(open) = code[item_at..]
            .find(['{', ';'])
            .map(|offset| item_at + offset)
        else {
            continue;
        };
        if code.as_bytes()[open] == b';' {
            continue;
        }
        let mut depth = 0;
        let mut close = open;
        for (index, letter) in code.char_indices().skip_while(|(index, _)| *index < open) {
            match letter {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        for flag in drop.iter_mut().take(close + 1).skip(start) {
            *flag = true;
        }
    }
    all.into_iter()
        .zip(drop)
        .filter(|(_, dropped)| !dropped)
        .map(|(piece, _)| piece)
        .collect()
}

/// Each literal with the code just before it.
fn literals_in_context(pieces: &[Piece]) -> Vec<(String, String)> {
    let mut before = String::new();
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Code(letter) => before.push(*letter),
            Piece::Comment => {}
            Piece::Literal(text) => {
                out.push((text.clone(), before.trim_end().to_string()));
                before.clear();
            }
        }
    }
    out
}

fn code_text(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Code(letter) => *letter,
            Piece::Comment | Piece::Literal(_) => ' ',
        })
        .collect()
}

fn shaped_like_a_name(literal: &str) -> bool {
    literal.len() >= 2
        && literal.chars().any(|letter| letter.is_ascii_uppercase())
        && literal
            .chars()
            .all(|letter| letter.is_ascii_uppercase() || letter.is_ascii_digit() || letter == '_')
}

fn function_names(code: &str) -> Vec<String> {
    code.split(|letter: char| !(letter.is_alphanumeric() || letter == '_'))
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[0] == "fn")
        .map(|pair| pair[1].to_string())
        .collect()
}

fn names_a_rule(function: &str) -> bool {
    function.split('_').any(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "reserved" | "credential" | "credentials"
        )
    })
}

/// Every (file, finding) of the scan, before the allow list.
fn scan() -> Vec<(String, String, String)> {
    let root = workspace();
    let mut files = Vec::new();
    for directory in ROOTS {
        rust_files(&root.join(directory), &mut files);
    }
    assert!(files.len() > 20, "the guard found too few source files");
    let mut out = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == THE_ONE_RULE {
            continue;
        }
        let production = production_pieces(&std::fs::read_to_string(&file).unwrap());
        for (literal, before) in literals_in_context(&production) {
            let read = ENVIRONMENT_READS.iter().any(|read| before.ends_with(read));
            if read || !shaped_like_a_name(&literal) {
                continue;
            }
            if let Some(finding) = rule_of(&literal) {
                out.push((relative.clone(), literal, finding));
            }
        }
        for function in function_names(&code_text(&production)) {
            if names_a_rule(&function) {
                out.push((relative.clone(), String::new(), format!("fn {function}")));
            }
        }
    }
    out
}

#[test]
fn no_variable_name_rule_exists_outside_model_rs() {
    let found: Vec<String> = scan()
        .into_iter()
        .filter(|(path, literal, _)| {
            !ALLOWED
                .iter()
                .any(|(allowed_path, allowed, _)| allowed_path == path && allowed == literal)
        })
        .map(|(path, _, finding)| format!("{path}: {finding}"))
        .collect();
    assert!(
        found.is_empty(),
        "a second variable-name rule; use model::reserved_class:\n{}",
        found.join("\n")
    );
}

/// Each row of the allow list still matches a finding, so the list holds no stale row
/// that could hide a later rule.
#[test]
fn every_row_of_the_allow_list_is_still_needed() {
    let found = scan();
    for (path, literal, reason) in ALLOWED {
        assert!(!reason.is_empty());
        assert!(
            found.iter().any(
                |(found_path, found_literal, _)| found_path == path && found_literal == literal
            ),
            "{path} no longer has {literal:?} as a finding; remove its row from ALLOWED"
        );
    }
}

/// The guard itself: it finds a planted rule in each shape it claims to read, and skips
/// comments, char literals, environment reads and test modules.
#[test]
fn the_guard_reads_literals_and_skips_comments_char_literals_reads_and_test_modules() {
    let source = "fn check(name: &str) -> bool {\n\
                  // \"HTTP_PROXY\" in a comment\n\
                  let quote = '\"';\n\
                  let _ = std::env::var_os(\"LOCALAPPDATA\");\n\
                  let _ = r#\"RUSTC_WRAPPER\"#;\n\
                  name.ends_with(\"_TOKEN\") || name == \"HOST_CC\" || name == \"PORT\"\n\
                  }\n\
                  #[cfg(test)]\n\
                  #[allow(dead_code)]\n\
                  pub(crate) mod tests {\n\
                  fn inner() { let _ = \"GIT_DIR\"; }\n\
                  }\n\
                  fn after() -> &'static str { \"GIT_SSH\" }\n";
    let literals = literals_in_context(&production_pieces(source));
    let texts: Vec<&str> = literals.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "LOCALAPPDATA",
            "RUSTC_WRAPPER",
            "_TOKEN",
            "HOST_CC",
            "PORT",
            "GIT_SSH"
        ]
    );
    assert!(ENVIRONMENT_READS
        .iter()
        .any(|read| literals[0].1.ends_with(read)));
    assert!(!ENVIRONMENT_READS
        .iter()
        .any(|read| literals[3].1.ends_with(read)));
    let rules: Vec<bool> = texts.iter().map(|text| rule_of(text).is_some()).collect();
    assert_eq!(rules, [true, true, true, true, false, true]);
    assert!(rule_of("_TOKEN").unwrap().contains("credential word"));
    assert!(rule_of("HOST_CC").unwrap().contains("reserved"));
    let functions = function_names("pub fn is_reserved_name() {} fn preserved() {}");
    assert_eq!(functions, ["is_reserved_name", "preserved"]);
    assert!(names_a_rule(&functions[0]));
    assert!(!names_a_rule(&functions[1]));
}
