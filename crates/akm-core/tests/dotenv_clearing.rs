//! FV-10, the core part: `dotenv::line` leaves no copy of a value in memory that it frees
//! without clearing it. Its callers clear the line that it returns (`Zeroizing`), so every
//! other buffer that held the value is an uncleared copy: an intermediate string, or the old
//! buffer of a string that grew. This test binary has its own allocator, which counts each
//! freed buffer that still holds the fake value. It has one test, so no other thread
//! allocates the value.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use zeroize::Zeroizing;

const MARK: &[u8] = b"fake-dotenv-marker";

struct Counting;

static UNCLEARED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the buffer is still allocated, with `layout.size()` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, layout.size()) };
        if bytes.windows(MARK.len()).any(|window| window == MARK) {
            UNCLEARED.fetch_add(1, Ordering::SeqCst);
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn a_dotenv_line_leaves_no_uncleared_copy_of_its_value() {
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
        UNCLEARED.store(0, Ordering::SeqCst);
        drop(Zeroizing::new(akm_core::dotenv::line("FAKE_NAME", value)));
        assert_eq!(
            UNCLEARED.load(Ordering::SeqCst),
            0,
            "{case}: a buffer that held the value was freed uncleared"
        );
    }
}
