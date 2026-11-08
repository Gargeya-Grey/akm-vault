//! Unit 3, finding M5 and 3-SA-02: the external dotenv profile, pinned with fixtures that a
//! second reader (python-dotenv) also read, and the size limits.
//! The fixtures and `expected.json` come from `tests/fixtures/dotenv/generate.py`.

use std::path::PathBuf;

use akm_core::dotenv::{self, MAX_BYTES, MAX_ENTRIES, MAX_LINE_BYTES};
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dotenv")
}

fn expected() -> Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("expected.json")).unwrap()).unwrap()
}

fn pairs(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_string(),
                pair[1].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn text_of(file: &str) -> String {
    String::from_utf8(std::fs::read(fixtures().join(file)).unwrap()).unwrap()
}

#[test]
fn every_fixture_parses_as_the_profile_says() {
    let document = expected();
    let cases = document["cases"].as_array().unwrap();
    assert!(cases.len() >= 30, "the fixture table lost cases");
    for case in cases {
        let file = case["file"].as_str().unwrap();
        let result = dotenv::parse(&text_of(file));
        if let Some(entries) = case["akm"].get("entries") {
            assert_eq!(result.unwrap(), pairs(entries), "{file}");
        } else {
            let fragment = case["akm"]["error"].as_str().unwrap();
            let message = result.unwrap_err().to_string();
            assert!(message.contains(fragment), "{file}: {message}");
        }
    }
}

#[test]
fn every_difference_from_the_reference_reader_is_written_down_and_no_note_is_stale() {
    let document = expected();
    assert_eq!(document["reader"]["name"], "python-dotenv");
    for case in document["cases"].as_array().unwrap() {
        let file = case["file"].as_str().unwrap();
        let same = case["akm"].get("entries") == Some(&case["reader"]["entries"])
            && case["reader"]["skipped"] == 0;
        let noted = case["note"].is_string();
        assert_eq!(
            noted, !same,
            "{file}: a case differs from the reference reader exactly when it has a note"
        );
    }
}

#[test]
fn the_lines_that_the_writer_makes_are_the_lines_the_reference_reader_read_back() {
    let document = expected();
    let writer = document["writer"].as_array().unwrap();
    assert!(writer.len() >= 12);
    for item in writer {
        let value = item["value"].as_str().unwrap();
        let line = item["line"].as_str().unwrap();
        assert_eq!(dotenv::line("A", value), line, "{value:?}");
        assert_eq!(
            dotenv::parse(&format!("{line}\n")).unwrap(),
            vec![("A".to_string(), value.to_string())],
            "{line:?}"
        );
    }
}

#[test]
fn file_content_is_never_interpolated_or_run() {
    assert!(
        std::env::var_os("USERPROFILE").is_some() || std::env::var_os("HOME").is_some(),
        "the premise needs a defined variable"
    );
    let parsed =
        dotenv::parse("A=${USERPROFILE}${HOME}\nB=\"%USERPROFILE%\"\nC='$(whoami)'\nD=`whoami`\n")
            .unwrap();
    assert_eq!(
        parsed,
        vec![
            ("A".into(), "${USERPROFILE}${HOME}".into()),
            ("B".into(), "%USERPROFILE%".into()),
            ("C".into(), "$(whoami)".into()),
            ("D".into(), "`whoami`".into()),
        ]
    );
}

#[test]
fn an_error_names_the_line_and_never_repeats_the_content() {
    for text in [
        "sk-fake-secret-value/abc\n",
        "sk-fake-secret-value/abc=xyz\n",
        "A=1\nsk-fake-secret-value\n",
        "A=\"sk-fake-secret-value never closed\nB=2\n",
        "A=\"sk-fake-secret-value\" trailing sk-fake-secret-value\n",
        "A='sk-fake-secret-value'sk-fake-secret-value\n",
        "bad-name-sk-fake-secret-value=1\n",
    ] {
        let message = dotenv::parse(text).unwrap_err().to_string();
        assert!(message.contains("line "), "{message}");
        assert!(!message.contains("sk-fake"), "{message}");
    }
}

#[test]
fn a_plain_name_is_named_when_it_is_the_fault() {
    let message = dotenv::parse("lower=1\n").unwrap_err().to_string();
    assert!(message.contains("lower"), "{message}");
}

#[test]
fn the_size_limits_are_exact_and_refuse_with_a_clear_message() {
    assert_eq!(MAX_BYTES, 256 * 1024);
    assert_eq!(MAX_LINE_BYTES, 16 * 1024);
    assert_eq!(MAX_ENTRIES, 500);

    let long_value = "x".repeat(MAX_LINE_BYTES - "A=".len());
    assert_eq!(
        dotenv::parse(&format!("A={long_value}\n")).unwrap()[0]
            .1
            .len(),
        long_value.len()
    );
    let message = dotenv::parse(&format!("A={long_value}y\n"))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("line 1") && message.contains("16 KiB"),
        "{message}"
    );

    let at_limit: String = (0..MAX_ENTRIES).map(|i| format!("V{i}=1\n")).collect();
    assert_eq!(dotenv::parse(&at_limit).unwrap().len(), MAX_ENTRIES);
    let over = format!("{at_limit}EXTRA=1\n");
    let message = dotenv::parse(&over).unwrap_err().to_string();
    assert!(message.contains("500 entries"), "{message}");

    let line = format!("K={}\n", "y".repeat(1000));
    let mut big = String::new();
    while big.len() + line.len() <= MAX_BYTES {
        big.push_str(&format!("# {}\n", "c".repeat(990)));
    }
    assert!(big.len() <= MAX_BYTES);
    assert!(dotenv::parse(&big).unwrap().is_empty());
    big.push_str(&"#".repeat(MAX_BYTES - big.len() + 1));
    let message = dotenv::parse(&big).unwrap_err().to_string();
    assert!(message.contains("256 KiB"), "{message}");
}

#[test]
fn a_multiline_value_counts_each_physical_line_against_the_line_limit() {
    let line = "y".repeat(MAX_LINE_BYTES / 2);
    let value = format!("{line}\n{line}\n{line}");
    let parsed = dotenv::parse(&format!("PEM=\"{value}\"\n")).unwrap();
    assert_eq!(parsed[0].1, value);
    let too_long = "y".repeat(MAX_LINE_BYTES + 1);
    let message = dotenv::parse(&format!("PEM=\"a\n{too_long}\n\"\n"))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("line 2") && message.contains("16 KiB"),
        "{message}"
    );
}
