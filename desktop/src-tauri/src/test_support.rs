use std::path::{Path, PathBuf};

use akm_core::audit::AuditLog;
use akm_core::journal::{JournalRoot, VaultEvidence};
use akm_core::{KeyDisclosure, Vault};

/// A vault's journal in the Cargo-provided test root, with the path of its journal file
/// (bound, and not created until the first record).
pub(crate) struct Journal {
    log: AuditLog,
    path: PathBuf,
}

impl Journal {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The lock file that a journal append takes.
    pub(crate) fn lock_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// The journal ID, which names the chain that a record must continue.
    pub(crate) fn id(&self) -> String {
        let name = self
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        name.trim_start_matches("j-")
            .trim_end_matches(".jsonl")
            .to_string()
    }
}

impl std::ops::Deref for Journal {
    type Target = AuditLog;
    fn deref(&self) -> &AuditLog {
        &self.log
    }
}

pub(crate) fn journal_of(vault: &Path) -> Journal {
    let log = AuditLog::for_vault(vault);
    let path = log.create_journal_path().unwrap();
    Journal { log, path }
}

/// The bound of a wait for an event that must happen. Each test vault uses the production
/// key derivation, so a full run under load can be several times slower than one test
/// alone. A wait that proves that an event is prompt, or that it does not happen, keeps its
/// own short bound (class Q).
pub(crate) const PATIENCE: std::time::Duration = std::time::Duration::from_secs(120);

/// A vault's journal in a root of the test's own, so the test does not share the Cargo
/// root with other tests (P2R2-06).
pub(crate) fn journal_in(vault: &Path, root: JournalRoot) -> Journal {
    let log = AuditLog::new(root, VaultEvidence::from_file(vault));
    let path = log.create_journal_path().unwrap();
    Journal { log, path }
}

/// The root of the test's own journals, inside its temporary folder.
pub(crate) fn own_root(directory: &Path) -> JournalRoot {
    JournalRoot::at(directory.join("journal"))
}

/// The canonical form of a path. A temporary folder can hold a short name such as
/// `RUNNER~1`, so a test compares canonical paths (P2R2-06).
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

/// A stored secret, read through a recorded disclosure request.
pub(crate) fn secret_of(vault: &Vault, id: &str) -> String {
    let key = vault.get_by_id(id).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
        .unwrap();
    vault.secret(id, &proof).unwrap().to_string()
}

pub(crate) fn secret_titled(vault: &Vault, title: &str) -> String {
    secret_of(vault, &vault.find(title).unwrap().id)
}

/// The variables of one scope with values, through a recorded export request.
pub(crate) fn env_of(vault: &Vault, project: &str, environment: &str) -> Vec<(String, String)> {
    let plan = vault
        .plan_env(Some(project), Some(environment), false)
        .unwrap();
    let proof = vault
        .disclose_env(akm_core::EnvDisclosure::Export, &plan, Some("test"))
        .unwrap();
    vault.env_vars(&plan, &proof).unwrap()
}

/// Class Q. A wait for an event that must happen passes `PATIENCE`: a timed receive whose
/// statement unwraps or expects the event, and a deadline. The test reads the test code of
/// every source file and of the integration tests, so a new file is read as well. Its own
/// search texts are built with `concat!`, so this source does not hold them.
#[test]
fn class_q_every_wait_for_an_event_that_must_happen_uses_the_shared_patience() {
    const RECEIVE: &str = concat!("recv_timeout", "(");
    const DEADLINE: &str = concat!("Instant::now()", " + ");
    const TEST_MODULE: &str = concat!("mod tests", " {");
    const TEST_ONLY: [&str; 2] = ["test_support.rs", "clipboard_fake.rs"];
    let shared = |text: &str| {
        text.starts_with("PATIENCE") || text.starts_with("crate::test_support::PATIENCE")
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut faults = Vec::new();
    for folder in ["src", "tests"] {
        for entry in std::fs::read_dir(root.join(folder)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let whole = folder == "tests" || TEST_ONLY.iter().any(|name| path.ends_with(name));
            let tests = if whole {
                text.as_str()
            } else {
                text.find(TEST_MODULE).map_or("", |start| &text[start..])
            };
            let mut found = |rest: &str, what: &str| {
                let start: String = rest.chars().take(40).collect();
                faults.push(format!("{}: {what} {start}", path.display()));
            };
            for (at, _) in tests.match_indices(RECEIVE) {
                let rest = &tests[at + RECEIVE.len()..];
                let statement = &rest[..rest.find(';').unwrap_or(rest.len())];
                let required = statement.contains(".unwrap()") || statement.contains(".expect(");
                if required && !shared(rest) {
                    found(rest, "a required receive with its own bound");
                }
            }
            for (at, _) in tests.match_indices(DEADLINE) {
                let rest = &tests[at + DEADLINE.len()..];
                if !shared(rest) {
                    found(rest, "a deadline with its own bound");
                }
            }
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}
