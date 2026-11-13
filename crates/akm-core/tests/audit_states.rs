use akm_core::audit::AuditState;
use akm_core::audit::{ChainFile, Damage};

#[test]
fn unknown_legacy_action_names_remain_literal_in_consistent_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    for action in ["__proto__", "constructor", "toString"] {
        log.append(action, None, None).unwrap();
    }
    let snapshot = log.snapshot(10).unwrap();
    assert_eq!(snapshot.state, AuditState::Consistent);
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|e| e.action.as_str())
            .collect::<Vec<_>>(),
        ["__proto__", "constructor", "toString"]
    );
}

#[test]
fn blank_records_require_json_whitespace_and_terminated_boundaries_for_every_byte() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    log.append("add-completed", None, None).unwrap();
    let good = std::fs::read(log.path()).unwrap();
    for prefix in [b"".as_slice(), good.as_slice()] {
        for byte in 0u8..=255 {
            for terminated in [false, true] {
                let mut bytes = prefix.to_vec();
                bytes.push(byte);
                if terminated {
                    bytes.push(b'\n');
                }
                std::fs::write(log.path(), &bytes).unwrap();
                let safe = byte == b'\n' || (terminated && matches!(byte, b' ' | b'\t' | b'\r'));
                let result = log.append("reveal-requested", None, None);
                assert_eq!(
                    result.is_ok(),
                    safe,
                    "byte={byte:#04x}, terminated={terminated}, prefix={}",
                    prefix.len()
                );
                if safe {
                    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Consistent);
                } else {
                    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
                    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Corrupt);
                }
            }
        }
    }
}

#[test]
fn unterminated_json_is_not_an_appendable_or_consistent_record() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    log.append("add-completed", None, None).unwrap();
    let mut bytes = std::fs::read(log.path()).unwrap();
    assert_eq!(bytes.pop(), Some(b'\n'));
    std::fs::write(log.path(), &bytes).unwrap();
    assert!(log.append("reveal-requested", None, None).is_err());
    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
    let report = log.snapshot(10).unwrap();
    assert_eq!(report.state, AuditState::Corrupt);
    assert_eq!(report.problem_line, Some(1));
    assert_eq!(report.broken_at, Some(1));
}

#[test]
fn append_errors_never_echo_malformed_json_values() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    let bytes = b"{\"seq\":\"FAKE_JOURNAL_MARKER\"}\n";
    std::fs::write(log.path(), bytes).unwrap();
    let error = log
        .append("reveal-requested", None, None)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("FAKE_JOURNAL_MARKER"), "{error}");
    assert!(error.contains("line 1"), "{error}");
    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
}

#[test]
fn audit_missing_and_empty_files_cannot_claim_verification() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    assert!(
        log.verify().is_err(),
        "a missing journal was treated as verified"
    );
    std::fs::write(log.path(), b"").unwrap();
    assert!(
        log.verify().is_err(),
        "an empty journal was treated as verified"
    );
}

#[test]
fn audit_snapshot_distinguishes_absent_empty_and_consistent_without_modifying_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Absent);
    std::fs::write(log.path(), b"\n \r\n").unwrap();
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Empty);
    log.append("copy-requested", Some("fake-key-id"), None)
        .unwrap();
    let bytes = std::fs::read(log.path()).unwrap();
    let snapshot = log.snapshot(10).unwrap();
    assert_eq!(snapshot.state, AuditState::Consistent);
    assert_eq!(snapshot.total_entries, 1);
    assert_eq!(snapshot.entries[0].action, "copy-requested");
    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
    std::fs::remove_file(log.path()).unwrap();
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Absent);
}

#[test]
fn audit_damage_is_structured_and_display_limit_does_not_skip_verification() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    for _ in 0..3 {
        log.append("copy-requested", None, None).unwrap();
    }
    let good = std::fs::read(log.path()).unwrap();
    let limited = log.snapshot(1).unwrap();
    assert_eq!(limited.total_entries, 3);
    assert_eq!(limited.entries[0].seq, 3);
    for suffix in [b"{\"secret\":\"fake-unterminated".as_slice(), b"not-json\n"] {
        let bytes = [good.as_slice(), suffix].concat();
        std::fs::write(log.path(), &bytes).unwrap();
        let snapshot = log.snapshot(1).unwrap();
        assert_eq!(snapshot.state, AuditState::Corrupt);
        assert_eq!(snapshot.problem_line, Some(4));
        assert_eq!(snapshot.broken_at, None);
        assert_eq!(snapshot.total_entries, 3);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
    }
    let text = String::from_utf8(good)
        .unwrap()
        .replacen("copy-requested", "copy-altered", 1);
    std::fs::write(log.path(), &text).unwrap();
    let snapshot = log.snapshot(1).unwrap();
    assert_eq!(snapshot.state, AuditState::Corrupt);
    assert_eq!(snapshot.problem_line, Some(1));
    assert_eq!(snapshot.broken_at, Some(1));
    assert_eq!(snapshot.total_entries, 0);
    assert_eq!(std::fs::read_to_string(log.path()).unwrap(), text);
}

#[test]
fn audit_snapshot_rows_and_verification_stay_consistent_during_concurrent_appends() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.jsonl");
    let log = ChainFile::at(&path, "fake-journal");
    log.append("add-completed", None, None).unwrap();
    let worker = std::thread::spawn(move || {
        let log = ChainFile::at(&path, "fake-journal");
        for _ in 0..30 {
            log.append("copy-requested", None, None).unwrap();
        }
    });
    for _ in 0..30 {
        let snapshot = log.snapshot(5).unwrap();
        assert_eq!(snapshot.state, AuditState::Consistent);
        assert_eq!(
            snapshot.entries.last().unwrap().seq as usize,
            snapshot.total_entries
        );
        assert_eq!(snapshot.entries.len(), snapshot.total_entries.min(5));
    }
    worker.join().unwrap();
    assert_eq!(log.snapshot(0).unwrap().total_entries, 31);
}

// 1A-R5-06: a damaged record keeps its declared sequence only when it is unambiguous.
#[test]
fn a_damaged_record_reports_only_an_unambiguous_declared_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    let cases: [(&str, Option<u64>); 16] = [
        (
            r#"{"seq":7,"ts":"bad","detail":"fake-hidden-detail"}"#,
            Some(7),
        ),
        (r#"{"seq":0,"ts":"bad"}"#, Some(0)),
        (
            r#"{"ts":"bad","seq":9007199254740993}"#,
            Some(9_007_199_254_740_993),
        ),
        (r#"{"seq":18446744073709551615,"ts":"bad"}"#, Some(u64::MAX)),
        (r#"{"seq":18446744073709551616,"ts":"bad"}"#, None),
        (r#"{"seq":7,"seq":8,"ts":"bad"}"#, None),
        (r#"{"seq":7,"s\u0065q":8,"ts":"bad"}"#, None),
        (r#"{"seq":"7","ts":"bad"}"#, None),
        (r#"{"seq":7.0,"ts":"bad"}"#, None),
        (r#"{"seq":7e0,"ts":"bad"}"#, None),
        (r#"{"seq":-1,"ts":"bad"}"#, None),
        (r#"{"ts":"bad"}"#, None),
        (r#"{"inner":{"seq":3},"ts":"bad"}"#, None),
        (r#"[{"seq":7}]"#, None),
        (r#"{"seq":7,"ts":"bad""#, None),
        ("not-json", None),
    ];
    for (line, declared) in cases {
        let bytes = format!("{line}\n");
        std::fs::write(log.path(), &bytes).unwrap();
        let snapshot = log.snapshot(10).unwrap();
        assert_eq!(snapshot.state, AuditState::Corrupt, "{line}");
        assert_eq!(snapshot.problem_line, Some(1), "{line}");
        assert_eq!(snapshot.broken_at, declared, "{line}");
        assert_eq!(std::fs::read_to_string(log.path()).unwrap(), bytes);
    }
}

#[test]
fn each_kind_of_damage_is_named_and_a_healthy_chain_names_none() {
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    for _ in 0..3 {
        log.append("copy-requested", None, None).unwrap();
    }
    let good = String::from_utf8(std::fs::read(log.path()).unwrap()).unwrap();
    assert_eq!(log.snapshot(10).unwrap().damage, None);
    let lines: Vec<&str> = good.split_inclusive('\n').collect();
    let cases: [(&str, String, Damage, usize); 4] = [
        (
            "an unterminated last line",
            format!("{good}{{\"seq\":4"),
            Damage::TruncatedTail,
            4,
        ),
        (
            "a terminated line that is not a record",
            format!("{good}not-json\n"),
            Damage::MalformedJson,
            4,
        ),
        (
            "a record dropped from the middle",
            format!("{}{}", lines[0], lines[2]),
            Damage::BrokenLink,
            2,
        ),
        (
            "an edited action",
            good.replacen("copy-requested", "copy-altered", 1),
            Damage::HashMismatch,
            1,
        ),
    ];
    for (name, bytes, kind, line) in cases {
        std::fs::write(log.path(), &bytes).unwrap();
        let snapshot = log.snapshot(10).unwrap();
        assert_eq!(snapshot.state, AuditState::Corrupt, "{name}");
        assert_eq!(snapshot.damage, Some(kind), "{name}");
        assert_eq!(snapshot.problem_line, Some(line), "{name}");
        let refused = log.append("reveal-requested", None, None);
        assert!(refused.is_err(), "{name}");
        assert_eq!(
            std::fs::read(log.path()).unwrap(),
            bytes.as_bytes(),
            "{name}"
        );
    }
}

/// CORE-R2-06 sweep: the journal reader reads a line of at most `MAX_RECORD_BYTES`, with its
/// line end. A longer line is no record that AKM writes: the journal reads as damaged at that
/// line, the reader holds no more than the bound, and the bytes stay. The writer refuses a
/// record that the reader would not take, so AKM never writes a line that it cannot read.
#[test]
fn a_journal_line_longer_than_the_record_bound_is_damage_and_never_written() {
    use akm_core::audit::MAX_RECORD_BYTES;
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    log.append("add-completed", None, None).unwrap();
    let one = std::fs::read(log.path()).unwrap().len();
    // A detail that makes the second line exactly the bound, with its line end.
    let fits = "d".repeat(MAX_RECORD_BYTES - one - 64);
    log.append("add-completed", None, Some(&fits)).unwrap();
    let lines: Vec<usize> = std::fs::read(log.path())
        .unwrap()
        .split_inclusive(|byte| *byte == b'\n')
        .map(<[u8]>::len)
        .collect();
    // The time stamp text varies by a few bytes, so the record stays 40 bytes under the bound.
    let pad = MAX_RECORD_BYTES - lines[1] - 40;
    let exact = format!("{fits}{}", "d".repeat(pad));
    let directory = tempfile::tempdir().unwrap();
    let log = ChainFile::at(directory.path().join("journal.jsonl"), "fake-journal");
    log.append("add-completed", None, None).unwrap();
    log.append("add-completed", None, Some(&exact)).unwrap();
    let snapshot = log.snapshot(10).unwrap();
    assert_eq!(snapshot.state, AuditState::Consistent);
    assert_eq!(snapshot.total_entries, 2);

    let refused = log.append(
        "add-completed",
        None,
        Some(&format!("{exact}{}", "d".repeat(80))),
    );
    assert!(refused.is_err(), "a record past the bound is never written");
    assert_eq!(log.snapshot(10).unwrap().total_entries, 2);

    // A valid third record, padded with JSON white space inside its object: the fields and
    // the hash are unchanged, so only the length makes it no record.
    log.append("reveal-requested", None, None).unwrap();
    let bytes = std::fs::read(log.path()).unwrap();
    let third = bytes[..bytes.len() - 1]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .unwrap()
        + 1;
    assert_eq!(bytes[third], b'{');
    let mut padded = bytes[..=third].to_vec();
    padded.resize(padded.len() + MAX_RECORD_BYTES, b' ');
    padded.extend_from_slice(&bytes[third + 1..]);
    std::fs::write(log.path(), &padded).unwrap();
    let snapshot = log.snapshot(10).unwrap();
    assert_eq!(
        snapshot.state,
        AuditState::Corrupt,
        "a padded record past the bound"
    );
    assert_eq!(snapshot.problem_line, Some(3));
    assert_eq!(snapshot.damage, Some(Damage::MalformedJson));
    assert_eq!(std::fs::read(log.path()).unwrap(), padded, "the bytes stay");
}
