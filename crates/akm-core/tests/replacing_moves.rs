//! Core review 1, finding 8 (FR-14, core): a replacing move to a name that an absence check
//! chose overwrites a file that another writer made in between. A move to a chosen name uses
//! `private_fs::move_new`, which refuses a taken name. This guard fails when the core or the
//! CLI gains a replacing move outside the three that replace their own file on purpose.

use std::path::PathBuf;

/// The replacing moves that stay, by file and by the function that holds them.
const ON_PURPOSE: [(&str, &str, &str); 3] = [
    (
        "crates/akm-core/src/vault.rs",
        "fn commit",
        "vault publication replaces the vault (contract 2), after the fingerprint check",
    ),
    (
        "crates/akm-core/src/vault.rs",
        "fn refresh_legacy_backup",
        "the .bak copy is replaced on purpose on every save (contract 2)",
    ),
    (
        "crates/akm-core/src/journal.rs",
        "fn store",
        "the mapping file has one fixed name that only AKM writes",
    ),
];

#[test]
fn every_replacing_move_replaces_its_own_file_on_purpose() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    let mut seen = Vec::new();
    for folder in ["crates/akm-core/src", "crates/akm-cli/src"] {
        let mut folders = vec![root.join(folder)];
        while let Some(folder) = folders.pop() {
            for item in std::fs::read_dir(&folder).unwrap() {
                let path = item.unwrap().path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap();
                let mut function = String::new();
                for (number, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap();
                    if let Some(at) = code.find("fn ") {
                        function = code[at..].split(['(', '<']).next().unwrap().to_string();
                    }
                    let replacing = code.contains("fs::rename(")
                        || code.contains("MOVEFILE_REPLACE_EXISTING")
                        || code.contains(".persist(");
                    if !replacing {
                        continue;
                    }
                    match ON_PURPOSE
                        .iter()
                        .find(|(file, owner, _)| name == *file && function == *owner)
                    {
                        Some(entry) => seen.push(entry.1),
                        None => found.push(format!("{name}:{}: {}", number + 1, code.trim())),
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a replacing move outside the list; move to a chosen name with private_fs::move_new:\n{}",
        found.join("\n")
    );
    for (_, owner, why) in ON_PURPOSE {
        assert!(
            seen.contains(&owner),
            "{owner} ({why}) no longer moves; update this list"
        );
    }
}
