//! FV-10 and FW-09, the core part: `dotenv::line`, `dotenv::parse` and the import preview leave
//! no copy of a value in memory that they free without clearing it. Their callers clear what
//! they pass in and what they get back (`Zeroizing`), so every other buffer that held the
//! value is an uncleared copy: an intermediate string, the old buffer of a string that grew,
//! or a partial result that an error drops. This test binary has its own allocator, which
//! counts each freed buffer that still holds the fake value while a test watches. The tests
//! take turns, so no other test allocates the value while one watches.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use akm_core::dotenv::{self, MAX_ENTRIES, MAX_LINE_BYTES};
use zeroize::Zeroizing;

const MARK: &[u8] = b"fake-dotenv-marker";

struct Counting;

static WATCH: AtomicBool = AtomicBool::new(false);
static UNCLEARED: AtomicUsize = AtomicUsize::new(0);
static TURN: Mutex<()> = Mutex::new(());

unsafe impl GlobalAlloc for Counting {
    // Every new buffer starts as zeros. A buffer from the system can still hold the bytes of
    // an earlier use past the length that its new owner writes, and a free of that buffer
    // would count as an uncleared copy of a value that its owner never held.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCH.load(Ordering::SeqCst) {
            // SAFETY: the buffer is still allocated, with `layout.size()` bytes.
            let bytes = unsafe { std::slice::from_raw_parts(pointer, layout.size()) };
            if bytes.windows(MARK.len()).any(|window| window == MARK) {
                UNCLEARED.fetch_add(1, Ordering::SeqCst);
            }
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The number of buffers that held the fake value and were freed uncleared while `run` ran.
fn uncleared(run: impl FnOnce()) -> usize {
    UNCLEARED.store(0, Ordering::SeqCst);
    WATCH.store(true, Ordering::SeqCst);
    run();
    WATCH.store(false, Ordering::SeqCst);
    UNCLEARED.load(Ordering::SeqCst)
}

#[test]
fn a_dotenv_line_leaves_no_uncleared_copy_of_its_value() {
    let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let digits = "0123456789".repeat(8);
    let cases = [
        ("safe", "fake-dotenv-marker".to_string()),
        ("safe, long", format!("fake-dotenv-marker-{digits}")),
        (
            "single quotes",
            "fake-dotenv-marker with spaces".to_string(),
        ),
        (
            "single quotes, long",
            format!("fake-dotenv-marker {digits}"),
        ),
        (
            "escapes",
            "fake-dotenv-marker\"with\\escapes\nand\rlines\tand tabs".to_string(),
        ),
        (
            "escapes, long",
            format!("fake-dotenv-marker {}{}", "\"".repeat(40), "\n".repeat(40)),
        ),
    ];
    for (case, value) in &cases {
        let freed = uncleared(|| drop(Zeroizing::new(dotenv::line("FAKE_NAME", value))));
        assert_eq!(
            freed, 0,
            "{case}: a buffer that held the value was freed uncleared"
        );
    }
}

/// FW-09: `parse` clears every buffer that held a value before it frees it, on every way out.
/// The documents hold the fake value in entries before an error, in the value that an error
/// rejects, or in a long quoted value that a parse accepts. The caller clears its text and the
/// entries that it gets.
#[test]
fn a_dotenv_parse_leaves_no_uncleared_copy_of_a_value_on_any_way_out() {
    let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let long = "x".repeat(MAX_LINE_BYTES + 1);
    let tail = "and a tail that makes the value longer than its first buffers".repeat(2);
    let cases = [
        (
            "a malformed line after two good lines",
            "FIRST=fake-dotenv-marker\nSECOND=\"fake-dotenv-marker two\"\nnot a line\n".to_string(),
        ),
        (
            "an invalid tail after a closing quote",
            format!("FIRST=\"fake-dotenv-marker {tail}\" junk\n"),
        ),
        (
            "an unterminated quote",
            format!("FIRST=\"fake-dotenv-marker\\t{tail}\n"),
        ),
        (
            "a line above the size limit after a good line",
            format!("FIRST=fake-dotenv-marker\nSECOND={long}\n"),
        ),
        (
            "an import above the entry limit",
            (0..=MAX_ENTRIES)
                .map(|index| format!("NAME_{index}=fake-dotenv-marker-{index}\n"))
                .collect(),
        ),
        (
            "a long quoted value that is accepted",
            format!("FIRST=\"fake-dotenv-marker\\n{tail}\"\n"),
        ),
    ];
    let mut failed = Vec::new();
    for (case, text) in cases {
        let text = Zeroizing::new(text);
        let freed = uncleared(|| match dotenv::parse(&text) {
            Ok(entries) => drop(Zeroizing::new(entries)),
            Err(error) => drop(error),
        });
        if freed > 0 {
            failed.push((case, freed));
        }
    }
    assert!(
        failed.is_empty(),
        "buffers that held a value were freed uncleared: {failed:?}"
    );
}

/// FW-09 sweep: a preview of an import plans the values that it would write and returns only
/// names and counts. The planned values are cleared when the plan is dropped.
#[test]
fn an_import_preview_leaves_no_uncleared_copy_of_a_value() {
    let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = tempfile::tempdir().unwrap();
    let mut vault = akm_core::Vault::create_in(
        directory.path().join("vault.kdbx"),
        "fake-preview-password",
        None,
        akm_core::journal::JournalRoot::at(directory.path().join("journal")),
    )
    .unwrap();
    let mut existing = akm_core::ApiKeyInput::new("Existing", "fake-old-value");
    existing.env_var = Some("EXISTING_KEY".into());
    existing.project = Some("demo".into());
    existing.environment = Some("dev".into());
    vault.add(existing).unwrap();
    let entries = Zeroizing::new(vec![
        (
            "EXISTING_KEY".to_string(),
            "fake-dotenv-marker-new".to_string(),
        ),
        (
            "ADDED_KEY".to_string(),
            "fake-dotenv-marker-added".to_string(),
        ),
    ]);

    let freed = uncleared(|| {
        let preview = vault
            .preview_import_env(&entries, Some("demo"), Some("dev"))
            .unwrap();
        assert_eq!((preview.added, preview.updated), (1, 1));
    });

    assert_eq!(freed, 0, "a planned value was freed uncleared");
}

/// D2A-03: the text of an export is one buffer of its exact length. A text that grows frees
/// each shorter buffer uncleared, with the values in it.
#[test]
fn a_dotenv_document_leaves_no_uncleared_copy_of_a_value() {
    let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let entries: Vec<(String, String)> = (0..40)
        .map(|index| {
            (
                format!("FAKE_NAME_{index}"),
                format!("fake-dotenv-marker {index}"),
            )
        })
        .collect();
    let freed = uncleared(|| drop(dotenv::document(&entries)));
    assert_eq!(freed, 0, "a buffer that held a value was freed uncleared");
    assert!(
        dotenv::document(&entries).starts_with("FAKE_NAME_0='fake-dotenv-marker 0'\nFAKE_NAME_1='")
    );
}
