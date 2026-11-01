use akm_core::audit::AuditLog;
use akm_core::audit::AuditState;

#[test]
fn unknown_legacy_action_names_remain_literal_in_consistent_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    for action in ["__proto__", "constructor", "toString"] {
        log.append(action, None, None, None, None, None).unwrap();
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
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    log.append("add-completed", None, None, None, None, None)
        .unwrap();
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
                let result = log.append("reveal-requested", None, None, None, None, None);
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
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    log.append("add-completed", None, None, None, None, None)
        .unwrap();
    let mut bytes = std::fs::read(log.path()).unwrap();
    assert_eq!(bytes.pop(), Some(b'\n'));
    std::fs::write(log.path(), &bytes).unwrap();
    assert!(log
        .append("reveal-requested", None, None, None, None, None)
        .is_err());
    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
    let report = log.snapshot(10).unwrap();
    assert_eq!(report.state, AuditState::Corrupt);
    assert_eq!(report.problem_line, Some(1));
    assert_eq!(report.broken_at, Some(1));
}

#[test]
fn append_errors_never_echo_malformed_json_values() {
    let directory = tempfile::tempdir().unwrap();
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    let bytes = b"{\"seq\":\"FAKE_JOURNAL_MARKER\"}\n";
    std::fs::write(log.path(), bytes).unwrap();
    let error = log
        .append("reveal-requested", None, None, None, None, None)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("FAKE_JOURNAL_MARKER"), "{error}");
    assert!(error.contains("line 1"), "{error}");
    assert_eq!(std::fs::read(log.path()).unwrap(), bytes);
}

#[test]
fn audit_missing_and_empty_files_cannot_claim_verification() {
    let directory = tempfile::tempdir().unwrap();
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
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
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Absent);
    std::fs::write(log.path(), b"\n \r\n").unwrap();
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Empty);
    log.append("copy-requested", None, Some("Fake"), None, None, None)
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
    let log = AuditLog::for_vault(&directory.path().join("fake.kdbx"));
    for _ in 0..3 {
        log.append("copy-requested", None, None, None, None, None)
            .unwrap();
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
    let path = directory.path().join("fake.kdbx");
    let log = AuditLog::for_vault(&path);
    log.append("add-completed", None, None, None, None, None)
        .unwrap();
    let worker = std::thread::spawn(move || {
        let log = AuditLog::for_vault(&path);
        for _ in 0..30 {
            log.append("copy-requested", None, None, None, None, None)
                .unwrap();
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
