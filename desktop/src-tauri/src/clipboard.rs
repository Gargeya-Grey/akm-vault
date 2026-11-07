use std::time::Duration;
use zeroize::Zeroizing;

use crate::error::CommandError;
pub(crate) use crate::publication::{LeaseEvidence, Outcome, Publication, Release};

pub const CLEAR_AFTER: Duration = Duration::from_secs(30);

/// Cleanup could not prove whether AKM's copy is still on the clipboard (unit 1a design,
/// rule 6). The text is history only: a kept warning outlives any retry, so it states
/// what happened and what to check, never a promise about later attempts.
pub fn uncertain_cleanup() -> CommandError {
    CommandError::new(
        "clipboardUncertain",
        "AKM could not confirm that its clipboard copy was cleared. Check the clipboard before you paste, and clear it if the copy is still there.",
    )
}

/// The error kind a backend returns when a failed cleanup empty changed the sequence.
pub const LOST: &str = "clipboardLost";

/// The error kind of a cleanup whose `CloseClipboard` failed: the clipboard may stay held.
pub const NOT_RELEASED: &str = "clipboardNotReleased";

pub fn not_released() -> CommandError {
    CommandError::new(
        NOT_RELEASED,
        "Windows did not confirm that AKM released the clipboard. Other programs may not be able to use it until AKM restarts.",
    )
}

// Each text states only what AKM observed. A lease is not evidence that its copy is still
// on the clipboard: the user can have replaced it (1A-A1-01).
const EARLIER_MAY_REMAIN: &str = "AKM's earlier copy may still be on the clipboard.";
const EMPTIED: &str =
    "The earlier clipboard content was removed, and the new value was not copied. Copy again.";
// After a failed rollback, the immediate cleanup's own result picks the text (1A-A1-02).
const RESIDUAL: &str = "AKM could not confirm that the partial copy was removed. Check the clipboard before you paste.";
const RESIDUAL_REMOVED: &str = "AKM then removed the partial copy. Copy again.";
const RESIDUAL_REPLACED: &str =
    "The clipboard changed after the partial copy, and AKM left the newer content in place. Copy again.";

fn with(error: CommandError, suffix: &str) -> CommandError {
    CommandError::new(&error.kind, format!("{} {suffix}", error.message))
}

/// A successful cleanup attempt.
enum Cleared {
    /// AKM emptied the clipboard while it still held AKM's copy.
    Removed,
    /// The clipboard had changed since AKM's copy, so AKM left it in place.
    Released,
}

pub struct CleanupFailure {
    pub lease_generation: u64,
    pub error: CommandError,
}

pub struct CopyFailure {
    pub error: CommandError,
    pub cleanup: Option<CleanupFailure>,
}

impl From<CommandError> for CopyFailure {
    fn from(error: CommandError) -> Self {
        Self {
            error,
            cleanup: None,
        }
    }
}

pub trait ClipboardBackend: Send {
    fn set_private(&mut self, text: &str) -> Publication;
    fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError>;
}

struct Lease {
    generation: u64,
    sequence: u32,
    text: Zeroizing<String>,
    expires: Duration,
}

/// Something that happened beside a copy or a cleanup and that the user should know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseNote {
    /// The hidden owner window of a copy could not be removed. The clipboard was not affected.
    OwnerWindowLeft,
}

impl LeaseNote {
    pub fn message(self) -> &'static str {
        match self {
            Self::OwnerWindowLeft => "AKM could not remove a hidden window it used to copy. The clipboard was not affected.",
        }
    }
}

pub struct ClipboardLease {
    backend: Box<dyn ClipboardBackend>,
    generation: u64,
    owned: Option<Lease>,
    notes: Vec<LeaseNote>,
    exit_attempted: bool,
}

impl ClipboardLease {
    pub fn new(backend: Box<dyn ClipboardBackend>) -> Self {
        Self {
            backend,
            generation: 0,
            owned: None,
            notes: Vec::new(),
            exit_attempted: false,
        }
    }

    /// The exit path made its one cleanup attempt, so `drop` makes none. A later copy
    /// starts a new attempt.
    pub fn exit_attempted(&mut self) {
        self.exit_attempted = true;
    }

    /// The notes that collected since the last call.
    pub fn take_notes(&mut self) -> Vec<LeaseNote> {
        std::mem::take(&mut self.notes)
    }

    // Opaque ownership identity only; no clipboard value or OS sequence escapes the lease.
    pub fn owned_generation(&self) -> Option<u64> {
        self.owned.as_ref().map(|lease| lease.generation)
    }

    #[cfg(test)]
    pub fn copy(&mut self, text: Zeroizing<String>, now: Duration) -> Result<u64, CommandError> {
        self.copy_reported(text, now)
            .map_err(|failure| failure.error)
    }

    pub fn copy_reported(
        &mut self,
        text: Zeroizing<String>,
        now: Duration,
    ) -> Result<u64, CopyFailure> {
        if text.contains('\0') {
            return Err(self
                .untouched(CommandError::new(
                    "clipboard",
                    "Clipboard text cannot contain a NUL character.",
                ))
                .into());
        }
        let publication = self.backend.set_private(&text);
        if publication.owner_leaked() {
            self.notes.push(LeaseNote::OwnerWindowLeft);
        }
        match publication.into_outcome() {
            Outcome::Untouched(error) => Err(self.untouched(error).into()),
            Outcome::Emptied(error) => {
                // The first empty removed any earlier AKM copy, so no lease remains to clean up.
                self.owned = None;
                Err(with(error, EMPTIED).into())
            }
            Outcome::EarlierCopyUncertain(error) => {
                // Contract 10: never adopt the changed sequence. The earlier lease ends here.
                let cleanup = self.owned.take().map(|lease| CleanupFailure {
                    lease_generation: lease.generation,
                    error: uncertain_cleanup(),
                });
                Err(CopyFailure {
                    error: with(error, &uncertain_cleanup().message),
                    cleanup,
                })
            }
            Outcome::ResidualUntracked(error) => {
                self.owned = None;
                self.generation = self.generation.wrapping_add(1);
                Err(CopyFailure {
                    error: with(error, &uncertain_cleanup().message),
                    cleanup: Some(CleanupFailure {
                        lease_generation: self.generation,
                        error: uncertain_cleanup(),
                    }),
                })
            }
            Outcome::Published(evidence) => {
                self.own(evidence, text, now + CLEAR_AFTER);
                Ok(self.generation)
            }
            Outcome::Residual(error, evidence) => {
                // A partial OS write still needs an owned lease if immediate cleanup finds a busy clipboard.
                self.own(evidence, text, now);
                let lease_generation = self.generation;
                let (suffix, cleanup) = match self.clear_observed() {
                    Ok(Cleared::Removed) => (RESIDUAL_REMOVED, None),
                    Ok(Cleared::Released) => (RESIDUAL_REPLACED, None),
                    Err(error) => (
                        RESIDUAL,
                        Some(CleanupFailure {
                            lease_generation,
                            error,
                        }),
                    ),
                };
                Err(CopyFailure {
                    error: with(error, suffix),
                    cleanup,
                })
            }
        }
    }

    fn untouched(&self, error: CommandError) -> CommandError {
        if self.owned.is_some() {
            with(error, EARLIER_MAY_REMAIN)
        } else {
            error
        }
    }

    fn own(&mut self, evidence: LeaseEvidence, text: Zeroizing<String>, expires: Duration) {
        self.generation = self.generation.wrapping_add(1);
        self.exit_attempted = false;
        self.owned = Some(Lease {
            generation: self.generation,
            sequence: evidence.sequence(),
            text,
            expires,
        });
    }

    pub fn clear(&mut self) -> Result<(), CommandError> {
        self.clear_observed().map(|_| ())
    }

    /// One cleanup attempt, with what it observed. The lease ends unless the attempt
    /// failed with the sequence unchanged, which keeps it for a retry.
    fn clear_observed(&mut self) -> Result<Cleared, CommandError> {
        let Some(lease) = &self.owned else {
            return Ok(Cleared::Released);
        };
        // Any cleanup failure (busy, refused) leaves the copy possibly in place.
        match self.backend.clear_matching(lease.sequence, &lease.text) {
            Err(error) => {
                if error.kind == NOT_RELEASED {
                    // The clipboard may stay held; no later attempt can be trusted either.
                    self.owned = None;
                    return Err(error);
                }
                if error.kind == LOST {
                    // A failed empty changed the sequence: never adopt it (contract 10).
                    self.owned = None;
                }
                Err(uncertain_cleanup())
            }
            Ok(removed) => {
                self.owned = None;
                Ok(if removed {
                    Cleared::Removed
                } else {
                    Cleared::Released
                })
            }
        }
    }

    pub fn tick(&mut self, now: Duration) -> Result<(), CommandError> {
        if let Some(lease) = &self.owned {
            self.expire(lease.generation, now)?;
        }
        Ok(())
    }

    pub fn expire(&mut self, generation: u64, now: Duration) -> Result<(), CommandError> {
        if self
            .owned
            .as_ref()
            .is_some_and(|lease| lease.generation == generation && now >= lease.expires)
        {
            self.clear()?;
        }
        Ok(())
    }
}

impl Drop for ClipboardLease {
    fn drop(&mut self) {
        if !self.exit_attempted {
            let _ = self.clear();
        }
    }
}

pub struct NativeClipboard;

#[cfg(windows)]
pub(crate) mod windows {
    use super::*;
    use crate::publication::{self, Rollback};
    use windows_sys::Win32::{
        Foundation::{GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HGLOBAL, HWND},
        Globalization::{
            GetLocaleInfoW, GetSystemDefaultLCID, WideCharToMultiByte, LOCALE_IDEFAULTANSICODEPAGE,
            LOCALE_IDEFAULTCODEPAGE, LOCALE_RETURN_NUMBER,
        },
        System::{
            DataExchange::{
                CloseClipboard, CountClipboardFormats, EmptyClipboard, GetClipboardData,
                GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
                SetClipboardData,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{
                GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
            },
        },
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE},
    };
    use zeroize::Zeroize;

    pub(crate) const ANSI_TEXT: u32 = 1;
    pub(crate) const OEM_TEXT: u32 = 7;
    pub(crate) const UNICODE_TEXT: u32 = 13;
    pub(crate) const TEXT_LOCALE: u32 = 16;
    pub(crate) const PRIVATE_FORMATS: [&str; 3] = [
        "CanIncludeInClipboardHistory",
        "CanUploadToCloudClipboard",
        "ExcludeClipboardContentFromMonitorProcessing",
    ];

    // This boundary models individual Win32 calls, not a second publication algorithm.
    pub(crate) trait ClipboardApi {
        type Owner;
        type Memory;

        fn owner(&mut self) -> Result<Self::Owner, CommandError>;
        /// `DestroyWindow`. A failure leaves a hidden window, and never touches the clipboard.
        fn destroy(&mut self, owner: Self::Owner) -> Result<(), CommandError>;
        fn register(&mut self, name: &str) -> Result<u32, CommandError>;
        fn encode(&mut self, text: &str) -> Result<TextFormats, CommandError>;
        fn allocate(&mut self, bytes: &[u8]) -> Result<Self::Memory, CommandError>;
        fn open(&mut self, owner: Option<&Self::Owner>) -> Result<(), CommandError>;
        /// `CloseClipboard`. A failure can leave the clipboard held.
        fn close(&mut self) -> Result<(), CommandError>;
        fn is_empty(&mut self) -> bool;
        fn empty(&mut self) -> Result<(), CommandError>;
        fn set(&mut self, format: u32, memory: &mut Self::Memory) -> Result<(), CommandError>;
        fn sequence(&mut self) -> u32;
        fn read_text(&mut self, expected: &str) -> TextMatch;
        fn matches_zero(&mut self, format: u32) -> bool;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum TextMatch {
        Same,
        Different,
        // GetClipboardData or GlobalLock failed: no evidence either way.
        Unreadable,
    }

    pub(crate) struct Open<'a, A: ClipboardApi> {
        pub(crate) api: &'a mut A,
        closed: bool,
        _owner: Option<&'a A::Owner>,
        _thread: std::marker::PhantomData<*mut ()>,
    }
    impl<'a, A: ClipboardApi> Open<'a, A> {
        pub(crate) fn acquire(
            api: &'a mut A,
            owner: Option<&'a A::Owner>,
        ) -> Result<Self, CommandError> {
            api.open(owner)?;
            Ok(Self::opened(api, owner))
        }

        /// A guard for a clipboard that `api.open` already opened.
        fn opened(api: &'a mut A, owner: Option<&'a A::Owner>) -> Self {
            Self {
                api,
                closed: false,
                _owner: owner,
                _thread: std::marker::PhantomData,
            }
        }

        /// The checked end of the open interval (design D5).
        pub(crate) fn close(mut self) -> Result<(), CommandError> {
            self.closed = true;
            self.api.close()
        }
    }
    impl<A: ClipboardApi> Drop for Open<'_, A> {
        // Only reached by unwinding: every normal path calls `close` and reads its result.
        fn drop(&mut self) {
            if !self.closed {
                let _ = self.api.close();
            }
        }
    }

    pub(crate) struct TextFormats {
        pub(crate) locale: u32,
        pub(crate) values: [(u32, Zeroizing<Vec<u8>>); 3],
    }

    impl TextFormats {
        pub(crate) fn new(text: &str, locale: u32) -> Result<Self, CommandError> {
            fn code_page(locale: u32, kind: u32) -> Result<u32, CommandError> {
                let mut page = 0u32;
                let count = unsafe {
                    GetLocaleInfoW(
                        locale,
                        kind | LOCALE_RETURN_NUMBER,
                        (&mut page as *mut u32).cast(),
                        2,
                    )
                };
                if count != 2 || page <= 1 {
                    return Err(CommandError::new(
                        "clipboard",
                        "Windows could not determine the clipboard locale's text encodings.",
                    ));
                }
                Ok(page)
            }

            fn multibyte(wide: &[u16], page: u32) -> Result<Zeroizing<Vec<u8>>, CommandError> {
                let error =
                    || CommandError::new("clipboard", "Windows could not encode clipboard text.");
                let count = i32::try_from(wide.len()).map_err(|_| error())?;
                // Include the NUL even for an empty value. Legacy formats may be lossy;
                // CF_UNICODETEXT below always retains the original UTF-16 units.
                let size = unsafe {
                    WideCharToMultiByte(
                        page,
                        0,
                        wide.as_ptr(),
                        count,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                    )
                };
                if size <= 0 {
                    return Err(error());
                }
                let mut bytes = Zeroizing::new(vec![0; size as usize]);
                let written = unsafe {
                    WideCharToMultiByte(
                        page,
                        0,
                        wide.as_ptr(),
                        count,
                        bytes.as_mut_ptr(),
                        size,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                    )
                };
                if written != size {
                    return Err(error());
                }
                Ok(bytes)
            }

            let wide = Zeroizing::new(text.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
            let ansi = multibyte(&wide, code_page(locale, LOCALE_IDEFAULTANSICODEPAGE)?)?;
            let oem = multibyte(&wide, code_page(locale, LOCALE_IDEFAULTCODEPAGE)?)?;
            Ok(Self {
                locale,
                values: [
                    (
                        UNICODE_TEXT,
                        Zeroizing::new(wide.iter().flat_map(|unit| unit.to_ne_bytes()).collect()),
                    ),
                    (ANSI_TEXT, ansi),
                    (OEM_TEXT, oem),
                ],
            })
        }
    }

    pub(crate) fn publish<A: ClipboardApi>(
        api: &mut A,
        text: &str,
        require_empty: bool,
    ) -> Publication {
        let tx = publication::begin();
        macro_rules! check {
            ($tx:ident, $result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(error) => return $tx.fail(error),
                }
            };
        }
        let mut exclusions = Vec::with_capacity(PRIVATE_FORMATS.len());
        for name in PRIVATE_FORMATS {
            let format = check!(tx, api.register(name));
            exclusions.push((format, check!(tx, api.allocate(&0u32.to_ne_bytes()))));
        }
        // CF_LOCALE explicitly describes both legacy encodings, independent of the input language.
        let encoded = check!(tx, api.encode(text));
        let frame = [
            (UNICODE_TEXT, check!(tx, api.allocate(&0u16.to_ne_bytes()))),
            (ANSI_TEXT, check!(tx, api.allocate(&[0]))),
            (OEM_TEXT, check!(tx, api.allocate(&[0]))),
            (
                TEXT_LOCALE,
                check!(tx, api.allocate(&encoded.locale.to_ne_bytes())),
            ),
        ];
        let mut values = Vec::with_capacity(encoded.values.len());
        for (format, bytes) in &encoded.values {
            values.push((*format, check!(tx, api.allocate(bytes))));
        }
        let owner = check!(tx, api.owner());
        if let Err(error) = api.open(Some(&owner)) {
            let release = Release {
                close_failed: false,
                owner_leaked: api.destroy(owner).is_err(),
            };
            return tx.fail(error).released(release);
        }
        let open = Open::opened(api, Some(&owner));
        let publication =
            publish_open(open.api, text, require_empty, tx, exclusions, frame, values);
        let close_failed = open.close().is_err();
        let owner_leaked = api.destroy(owner).is_err();
        publication.released(Release {
            close_failed,
            owner_leaked,
        })
    }

    type Pending<A> = Vec<(u32, <A as ClipboardApi>::Memory)>;

    fn publish_open<A: ClipboardApi>(
        api: &mut A,
        text: &str,
        require_empty: bool,
        tx: publication::Preparing,
        mut exclusions: Pending<A>,
        mut frame: [(u32, A::Memory); 4],
        mut values: Pending<A>,
    ) -> Publication {
        // Sampled before any change, so a failed empty can be compared in this open interval.
        let tx = tx.opened(api.sequence());
        if require_empty && !api.is_empty() {
            return tx.fail(CommandError::new(
                "clipboard",
                "Refusing to overwrite the user's clipboard.",
            ));
        }
        if let Err(error) = api.empty() {
            let after = api.sequence();
            return tx.empty_failed(error, after);
        }
        let tx = tx.emptied();
        // Never publish text if an exclusion could not be installed.
        for (format, memory) in &mut exclusions {
            if let Err(error) = api.set(*format, memory) {
                return tx.fail(error, || roll_back(api));
            }
        }
        if exclusions
            .iter()
            .any(|(format, _)| !api.matches_zero(*format))
        {
            return tx.fail(
                CommandError::new(
                    "clipboard",
                    "Windows did not apply clipboard history/cloud/monitoring exclusions. The copy was cancelled.",
                ),
                || roll_back(api),
            );
        }
        // CloseClipboard adds missing text/locale formats and changes the sequence.
        // Install the complete, empty format set before any secret bytes. Even a later
        // failed SetClipboardData plus a failed rollback then leaves no synthesis work.
        // Unicode is first so consumers enumerating formats prefer the lossless value.
        for (format, memory) in &mut frame {
            if let Err(error) = api.set(*format, memory) {
                return tx.fail(error, || roll_back(api));
            }
        }
        let tx = tx.writing();
        for (format, memory) in &mut values {
            if let Err(error) = api.set(*format, memory) {
                return tx.fail(error, || roll_back(api));
            }
        }
        // No other writer can publish between our SetClipboardData and this ownership sample.
        let verified = api.read_text(text) == TextMatch::Same;
        match std::num::NonZeroU32::new(api.sequence()) {
            Some(sequence) if verified => tx.published(sequence),
            _ => tx.fail(
                CommandError::new("clipboard", "Windows did not confirm the copy."),
                || roll_back(api),
            ),
        }
    }

    /// The second `EmptyClipboard`, with the sequence sampled before and after it.
    fn roll_back<A: ClipboardApi>(api: &mut A) -> Rollback {
        let before = api.sequence();
        let emptied = api.empty().is_ok();
        let after = api.sequence();
        Rollback::new(emptied, before, after)
    }

    pub(crate) fn clear<A: ClipboardApi>(
        api: &mut A,
        sequence: u32,
        text: &str,
    ) -> Result<bool, CommandError> {
        let open = Open::acquire(api, None)?;
        // The comparison and clear share one OS clipboard lock.
        let result = clear_open(open.api, sequence, text);
        match (open.close(), result) {
            (Ok(()), result) => result,
            // A failed close can leave the clipboard held, whatever the cleanup found.
            (Err(_), Ok(_)) => Err(not_released()),
            (Err(_), Err(error)) => Err(error),
        }
    }

    fn clear_open<A: ClipboardApi>(
        api: &mut A,
        sequence: u32,
        text: &str,
    ) -> Result<bool, CommandError> {
        if sequence == 0 {
            // Only a rolled-back publication reaches here with zero; the lease rejects the rest.
            return Ok(false);
        }
        let current = api.sequence();
        if current == 0 {
            // Zero means no clipboard access, not a newer writer.
            return Err(uncertain_cleanup());
        }
        if current != sequence {
            // Another writer published, possibly the same text: preserve it.
            return Ok(false);
        }
        match api.read_text(text) {
            TextMatch::Same => {
                // Reading can let a delayed-rendering owner set data inside this open
                // interval, so the sequence is checked again right before the empty (1H-03).
                match api.sequence() {
                    0 => return Err(uncertain_cleanup()),
                    now if now != sequence => return Ok(false),
                    _ => {}
                }
                if let Err(error) = api.empty() {
                    if api.sequence() != sequence {
                        // The failed empty changed the sequence: its effect is unknown.
                        return Err(CommandError::new(LOST, error.message));
                    }
                    return Err(error);
                }
                Ok(true)
            }
            TextMatch::Different | TextMatch::Unreadable => Err(uncertain_cleanup()),
        }
    }

    struct Win32;

    struct Owner(HWND);
    impl Drop for Owner {
        // Only reached by unwinding: `destroy` reads the result on every normal path.
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    struct Memory {
        handle: HGLOBAL,
        size: usize,
    }
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe {
                    let data = GlobalLock(self.handle).cast::<u8>();
                    if !data.is_null() {
                        std::slice::from_raw_parts_mut(data, self.size).zeroize();
                        GlobalUnlock(self.handle);
                    }
                    GlobalFree(self.handle);
                }
            }
        }
    }

    impl ClipboardApi for Win32 {
        type Owner = Owner;
        type Memory = Memory;

        fn owner(&mut self) -> Result<Owner, CommandError> {
            let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
            // A built-in class needs no custom window procedure or app-window mutation.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not create a clipboard owner.",
                ));
            }
            Ok(Owner(hwnd))
        }

        fn destroy(&mut self, owner: Owner) -> Result<(), CommandError> {
            let destroyed = unsafe { DestroyWindow(owner.0) } != 0;
            std::mem::forget(owner);
            if !destroyed {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not remove the clipboard owner window.",
                ));
            }
            Ok(())
        }

        fn encode(&mut self, text: &str) -> Result<TextFormats, CommandError> {
            TextFormats::new(text, unsafe { GetSystemDefaultLCID() })
        }

        fn register(&mut self, name: &str) -> Result<u32, CommandError> {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
            if format == 0 {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not register clipboard exclusions.",
                ));
            }
            Ok(format)
        }

        fn allocate(&mut self, bytes: &[u8]) -> Result<Memory, CommandError> {
            let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) };
            if handle.is_null() {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not allocate clipboard memory.",
                ));
            }
            let memory = Memory {
                handle,
                size: bytes.len(),
            };
            unsafe {
                let data = GlobalLock(handle).cast::<u8>();
                if data.is_null() {
                    return Err(CommandError::new(
                        "clipboard",
                        "Windows could not lock clipboard memory.",
                    ));
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
                GlobalUnlock(handle);
            }
            Ok(memory)
        }

        fn open(&mut self, owner: Option<&Owner>) -> Result<(), CommandError> {
            if unsafe { OpenClipboard(owner.map_or(std::ptr::null_mut(), |owner| owner.0)) } == 0 {
                return Err(CommandError::new(
                    "clipboard",
                    "The clipboard is busy. Try copying again.",
                ));
            }
            Ok(())
        }

        fn close(&mut self) -> Result<(), CommandError> {
            if unsafe { CloseClipboard() } == 0 {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not close the clipboard.",
                ));
            }
            Ok(())
        }

        fn is_empty(&mut self) -> bool {
            unsafe {
                SetLastError(ERROR_SUCCESS);
                CountClipboardFormats() == 0 && GetLastError() == ERROR_SUCCESS
            }
        }

        fn empty(&mut self) -> Result<(), CommandError> {
            if unsafe { EmptyClipboard() } == 0 {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not clear the clipboard.",
                ));
            }
            Ok(())
        }

        fn set(&mut self, format: u32, memory: &mut Memory) -> Result<(), CommandError> {
            if unsafe { SetClipboardData(format, memory.handle) }.is_null() {
                return Err(CommandError::new(
                    "clipboard",
                    "Windows could not publish private clipboard data.",
                ));
            }
            // Only a successful SetClipboardData transfers this movable allocation to Windows.
            memory.handle = std::ptr::null_mut();
            Ok(())
        }

        fn sequence(&mut self) -> u32 {
            unsafe { GetClipboardSequenceNumber() }
        }

        fn read_text(&mut self, expected: &str) -> TextMatch {
            // No pointer outlives the exclusive Windows clipboard-open interval.
            unsafe {
                let handle = GetClipboardData(UNICODE_TEXT);
                if handle.is_null() {
                    return TextMatch::Unreadable;
                }
                let count = GlobalSize(handle) / 2;
                let ptr = GlobalLock(handle).cast::<u16>();
                if ptr.is_null() {
                    return TextMatch::Unreadable;
                }
                let units = std::slice::from_raw_parts(ptr, count);
                let matches = units
                    .iter()
                    .position(|unit| *unit == 0)
                    .is_some_and(|end| units[..end].iter().copied().eq(expected.encode_utf16()));
                GlobalUnlock(handle);
                if matches {
                    TextMatch::Same
                } else {
                    TextMatch::Different
                }
            }
        }

        fn matches_zero(&mut self, format: u32) -> bool {
            unsafe {
                let handle = GetClipboardData(format);
                if handle.is_null() || GlobalSize(handle) < 4 {
                    return false;
                }
                let data = GlobalLock(handle).cast::<u32>();
                if data.is_null() {
                    return false;
                }
                let value = data.read_unaligned();
                GlobalUnlock(handle);
                value == 0
            }
        }
    }

    impl ClipboardBackend for NativeClipboard {
        fn set_private(&mut self, text: &str) -> Publication {
            publish(&mut Win32, text, false)
        }

        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            clear(&mut Win32, sequence, text)
        }
    }

    #[test]
    #[ignore = "opt-in only; requires AKM_TEST_REAL_CLIPBOARD=1 and an empty clipboard"]
    fn actual_windows_private_formats_are_present() {
        const FAKE_VALUE: &str = "AKM fake clipboard verification secret";
        assert_eq!(std::env::var("AKM_TEST_REAL_CLIPBOARD").as_deref(), Ok("1"));
        // Check the precondition inside the production transaction's open interval.
        let seq = publish(&mut Win32, FAKE_VALUE, true).published_for_test();
        let mut backend = NativeClipboard;
        let result = std::panic::catch_unwind(|| {
            let mut api = Win32;
            let open = Open::acquire(&mut api, None).unwrap();
            assert_eq!(
                open.api.sequence(),
                seq,
                "the captured sequence must survive owner destruction"
            );
            assert_eq!(open.api.read_text(FAKE_VALUE), TextMatch::Same);
            assert_eq!(unsafe { CountClipboardFormats() }, 7);
            for format in [ANSI_TEXT, OEM_TEXT, TEXT_LOCALE] {
                unsafe {
                    let handle = GetClipboardData(format);
                    assert!(!handle.is_null(), "Missing text format {format}");
                    let bytes = if format == TEXT_LOCALE {
                        GetSystemDefaultLCID().to_ne_bytes().to_vec()
                    } else {
                        FAKE_VALUE.bytes().chain(Some(0)).collect()
                    };
                    assert!(GlobalSize(handle) >= bytes.len());
                    let data = GlobalLock(handle).cast::<u8>();
                    assert!(!data.is_null());
                    let matches = std::slice::from_raw_parts(data, bytes.len()) == bytes;
                    GlobalUnlock(handle);
                    assert!(matches, "Incorrect text format {format}");
                }
            }
            for name in [
                "ExcludeClipboardContentFromMonitorProcessing",
                "CanUploadToCloudClipboard",
                "CanIncludeInClipboardHistory",
            ] {
                let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                unsafe {
                    let format = RegisterClipboardFormatW(wide.as_ptr());
                    assert_ne!(format, 0);
                    let handle = GetClipboardData(format);
                    assert!(!handle.is_null(), "Missing {name}");
                    assert!(GlobalSize(handle) >= 4);
                    let data = GlobalLock(handle).cast::<u32>();
                    assert!(!data.is_null());
                    let value = data.read_unaligned();
                    GlobalUnlock(handle);
                    assert_eq!(value, 0, "Incorrect {name}");
                }
            }
            assert_eq!(
                open.api.sequence(),
                seq,
                "reading must not synthesize formats"
            );
        });
        let cleanup = backend.clear_matching(seq, FAKE_VALUE);
        if !matches!(cleanup, Ok(true)) {
            eprintln!(
                "Cleanup could not be confirmed for the exact fake value {FAKE_VALUE:?}. \
                 No newer clipboard sequence was adopted or cleared."
            );
        }
        result.unwrap();
        assert!(
            matches!(cleanup, Ok(true)),
            "the publication's captured sequence must authorize cleanup: {cleanup:?}"
        );
    }

    /// Publisher and cleanup tests over the fake Win32 API in `clipboard_fake`.
    #[cfg(test)]
    pub(crate) mod transaction_tests {
        use super::*;
        use crate::clipboard_fake::*;
        use std::sync::{Arc, Mutex};

        #[test]
        fn captured_lease_survives_windows_close_synthesis_and_owner_destruction() {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let mut backend = TransactionBackend(FakeApi(state.clone()));
            let published = backend.set_private(FAKE_TEXT).published_for_test();
            {
                let state = state.lock().unwrap();
                assert_eq!(
                    published, state.sequence,
                    "the sequence captured under OpenClipboard must still identify this publication after close"
                );
                assert_eq!(state.text(), FAKE_TEXT);
                assert!(!state.owner_alive);
            }
            assert!(backend.clear_matching(published, FAKE_TEXT).unwrap());
            assert_eq!(state.lock().unwrap().text(), "");
        }

        #[test]
        fn empty_and_unicode_values_keep_exact_unicode_and_stable_cleanup_ownership() {
            for text in ["", "\u{03bb}\u{1f512}", "e\u{301}\r\nfake"] {
                let state = Arc::new(Mutex::new(FakeState::default()));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                let published = backend.set_private(text).published_for_test();
                {
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), text);
                    assert_eq!(state.sequence, published);
                    assert_eq!(state.synthesized, 0);
                    assert_eq!(state.formats.len(), 7);
                    if text.is_empty() {
                        assert_eq!(state.formats[&UNICODE_TEXT], [0, 0]);
                        assert_eq!(state.formats[&ANSI_TEXT], [0]);
                        assert_eq!(state.formats[&OEM_TEXT], [0]);
                    }
                }
                assert!(backend.clear_matching(published, text).unwrap());
                assert!(state.lock().unwrap().formats.is_empty());
            }
        }

        #[test]
        fn native_conversion_uses_the_published_locales_ansi_and_oem_code_pages() {
            let latin = TextFormats::new("caf\u{e9}", 0x0409).unwrap();
            assert_eq!(latin.locale, 0x0409);
            assert_eq!(
                *latin.values[0].1,
                [b'c', 0, b'a', 0, b'f', 0, 0xe9, 0, 0, 0]
            );
            assert_eq!(*latin.values[1].1, [b'c', b'a', b'f', 0xe9, 0]);
            assert_eq!(*latin.values[2].1, [b'c', b'a', b'f', 0x82, 0]);

            let cyrillic = TextFormats::new("\u{042f}", 0x0419).unwrap();
            assert_eq!(cyrillic.locale, 0x0419);
            assert_eq!(*cyrillic.values[0].1, [0x2f, 0x04, 0, 0]);
            assert_eq!(*cyrillic.values[1].1, [0xdf, 0]);
            assert_eq!(*cyrillic.values[2].1, [0x9f, 0]);
        }

        #[test]
        fn real_test_precondition_refuses_unrelated_formats_without_releasing_the_open_interval() {
            let mut data = FakeState::default();
            data.formats.insert(0xc100, b"unrelated user data".to_vec());
            let state = Arc::new(Mutex::new(data));
            let mut api = FakeApi(state.clone());
            let error = publish(&mut api, FAKE_TEXT, true).error_for_test();
            assert_eq!(error.message, "Refusing to overwrite the user's clipboard.");
            let state = state.lock().unwrap();
            assert_eq!(state.formats[&0xc100], b"unrelated user data");
            assert_eq!(state.sequence, 100);
            assert_eq!((state.empty_calls, state.set_calls), (0, 0));
            assert_eq!(
                (state.opens, state.closes, state.destroyed_owners),
                (1, 1, 1)
            );
            assert_eq!((state.allocations, state.client_frees), (10, 10));
        }

        #[test]
        fn identical_user_copy_between_publication_and_capture_is_not_owned_or_cleared() {
            let state = Arc::new(Mutex::new(FakeState {
                race_at_capture: true,
                ..FakeState::default()
            }));
            let mut backend = TransactionBackend(FakeApi(state.clone()));
            let published = backend.set_private(FAKE_TEXT).published_for_test();
            assert_eq!(
                published, 111,
                "must capture AKM's publication, not the user's"
            );
            assert_eq!(state.lock().unwrap().blocked_copies, 1);
            assert_eq!(state.lock().unwrap().user_sequence, Some(112));
            assert!(!backend.clear_matching(published, FAKE_TEXT).unwrap());
            let state = state.lock().unwrap();
            assert_eq!(state.text(), FAKE_TEXT);
            assert_eq!(state.clears, 0);
        }

        #[test]
        fn failed_set_clipboard_data_never_exposes_text_and_reclaims_partial_formats() {
            for fail_at in 1..=10 {
                let state = Arc::new(Mutex::new(FakeState {
                    faults: vec![Fault::Set(fail_at)],
                    ..FakeState::default()
                }));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                let error = backend.set_private(FAKE_TEXT).error_for_test();
                assert_eq!(error.message, "injected SetClipboardData failure");
                let state = state.lock().unwrap();
                assert_eq!(state.text(), "");
                assert!(
                    state.formats.is_empty(),
                    "partial formats must be removed after SetClipboardData {fail_at} fails"
                );
                assert_eq!(state.allocations, 10);
                assert_eq!(state.transfers, fail_at - 1);
                assert_eq!(state.client_frees, 11 - fail_at);
                assert_eq!(state.system_frees, fail_at - 1);
                assert_eq!(
                    (state.opens, state.closes, state.destroyed_owners),
                    (1, 1, 1)
                );
            }
        }

        #[test]
        fn incomplete_empty_format_setup_cannot_expose_a_secret_even_if_rollback_fails() {
            for fail_at in 1..=8 {
                let state = Arc::new(Mutex::new(FakeState {
                    faults: vec![Fault::Set(fail_at), Fault::Empty(2)],
                    ..FakeState::default()
                }));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                assert_eq!(
                    backend.set_private(FAKE_TEXT).error_for_test().kind,
                    "clipboard"
                );
                let state = state.lock().unwrap();
                assert_eq!(state.text(), "");
                for format in [UNICODE_TEXT, ANSI_TEXT, OEM_TEXT] {
                    if let Some(bytes) = state.formats.get(&format) {
                        assert!(bytes.iter().all(|byte| *byte == 0));
                    }
                }
                assert_eq!(state.transfers, fail_at - 1);
                assert_eq!(state.client_frees, 11 - fail_at);
                assert_eq!(
                    (state.opens, state.closes, state.destroyed_owners),
                    (1, 1, 1)
                );
            }
        }

        #[test]
        fn unreadable_exclusions_fail_before_text_publication_even_if_rollback_fails() {
            for format in 0xc000..0xc003 {
                for rollback_fails in [false, true] {
                    let mut faults = vec![Fault::ZeroRead(format)];
                    if rollback_fails {
                        faults.push(Fault::Empty(2));
                    }
                    let state = Arc::new(Mutex::new(FakeState {
                        faults,
                        ..FakeState::default()
                    }));
                    let mut backend = TransactionBackend(FakeApi(state.clone()));
                    assert!(
                        backend.set_private(FAKE_TEXT).name_for_test() == "Emptied",
                        "an unverified exclusion must not publish text"
                    );
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), "");
                    assert_eq!(state.set_calls, 3);
                    assert_eq!(state.client_frees, 7);
                    assert_eq!(state.formats.len(), if rollback_fails { 3 } else { 0 });
                    assert_eq!(
                        (state.opens, state.closes, state.destroyed_owners),
                        (1, 1, 1)
                    );
                }
            }
        }

        #[test]
        fn unverifiable_text_is_removed_before_the_publication_transaction_closes() {
            for fault in [Fault::TextRead, Fault::Sequence] {
                let state = Arc::new(Mutex::new(FakeState {
                    faults: vec![fault],
                    ..FakeState::default()
                }));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                // Rolled back in the publication's own open interval: no lease evidence remains.
                assert_eq!(backend.set_private(FAKE_TEXT).name_for_test(), "Emptied");
                let state = state.lock().unwrap();
                assert_eq!(
                    state.text(),
                    "",
                    "failure must roll back while still holding OpenClipboard"
                );
                assert_eq!(state.clears, 1);
                assert_eq!(state.system_frees, 10);
                assert_eq!(state.client_frees, 0);
                assert_eq!(
                    (state.opens, state.closes, state.destroyed_owners),
                    (1, 1, 1)
                );
            }
        }

        #[test]
        fn success_publishes_utf16_and_all_three_zero_exclusions_in_one_interval() {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let mut backend = TransactionBackend(FakeApi(state.clone()));
            let published = backend
                .set_private("\u{03bb}\u{1f512}")
                .published_for_test();
            assert_eq!(published, 111);
            {
                let state = state.lock().unwrap();
                assert_eq!(
                    state.registered_names,
                    [
                        "CanIncludeInClipboardHistory",
                        "CanUploadToCloudClipboard",
                        "ExcludeClipboardContentFromMonitorProcessing",
                    ]
                );
                assert_eq!(state.formats.len(), 7);
                for format in 0xc000..0xc003 {
                    assert_eq!(state.formats[&format], [0, 0, 0, 0]);
                }
                assert_eq!(
                    state.formats[&13],
                    [0xbb, 0x03, 0x3d, 0xd8, 0x12, 0xdd, 0, 0]
                );
                assert_eq!(
                    (state.allocations, state.transfers, state.client_frees),
                    (10, 10, 0)
                );
                assert_eq!(state.system_frees, 3);
                assert_eq!(state.synthesized, 0);
                assert_eq!(
                    (state.opens, state.closes, state.destroyed_owners),
                    (1, 1, 1)
                );
                assert!(!state.owner_alive);
            }
            // An unchanged sequence with other text is no proof of a newer owner (1A-R5-02).
            assert_eq!(
                backend
                    .clear_matching(published, "different fake text")
                    .unwrap_err()
                    .kind,
                "clipboardUncertain"
            );
            assert_eq!(state.lock().unwrap().text(), "\u{03bb}\u{1f512}");
            assert!(backend
                .clear_matching(published, "\u{03bb}\u{1f512}")
                .unwrap());
            let state = state.lock().unwrap();
            assert_eq!(state.text(), "");
            assert_eq!(state.system_frees, 10);
            assert_eq!(state.client_frees, 0);
        }

        #[test]
        fn preparation_and_initial_open_failures_preserve_the_existing_clipboard() {
            for fault in [
                Fault::Register(1),
                Fault::Register(2),
                Fault::Register(3),
                Fault::Allocate(1),
                Fault::Allocate(2),
                Fault::Allocate(3),
                Fault::Allocate(4),
                Fault::Allocate(5),
                Fault::Allocate(6),
                Fault::Allocate(7),
                Fault::Allocate(8),
                Fault::Allocate(9),
                Fault::Allocate(10),
                Fault::Owner,
                Fault::Open(1),
                Fault::Empty(1),
            ] {
                let mut data = FakeState {
                    faults: vec![fault],
                    ..FakeState::default()
                };
                data.formats
                    .insert(13, vec![b'u', 0, b's', 0, b'e', 0, b'r', 0, 0, 0]);
                let state = Arc::new(Mutex::new(data));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                let error = backend.set_private(FAKE_TEXT).error_for_test();
                assert_eq!(error.kind, "clipboard");
                assert!(!error.message.contains(FAKE_TEXT));
                let state = state.lock().unwrap();
                assert_eq!(state.sequence, 100);
                assert_eq!(state.text(), "user");
                assert_eq!(state.transfers, 0);
                assert_eq!(state.system_frees, 0);
                assert_eq!(state.allocations, state.client_frees);
                assert!(
                    state.open_calls <= 1,
                    "busy OpenClipboard must not retry under the session gate"
                );
                assert_eq!(state.opens, state.closes);
                assert!(!state.owner_alive);
            }
        }

        #[test]
        fn a_failed_publication_retains_its_lease_until_tick_or_drop_can_clean_up() {
            for (fault, sequence, transfers, client_frees) in [
                (Fault::TextRead, 111, 10, 0),
                (Fault::Set(9), 109, 8, 2),
                (Fault::Set(10), 110, 9, 1),
            ] {
                for cleanup_on_drop in [false, true] {
                    let state = Arc::new(Mutex::new(FakeState {
                        faults: vec![fault, Fault::Empty(2), Fault::Open(2)],
                        ..FakeState::default()
                    }));
                    let mut lease =
                        ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
                    let error = lease
                        .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                        .unwrap_err();
                    assert_eq!(error.kind, "clipboard");
                    assert!(!error.message.contains(FAKE_TEXT));
                    {
                        let state = state.lock().unwrap();
                        assert_eq!(state.sequence, sequence);
                        assert_eq!(state.text(), FAKE_TEXT);
                        assert_eq!(state.formats.len(), 7);
                        assert_eq!(state.synthesized, 0);
                        assert_eq!((state.open_calls, state.opens, state.closes), (2, 1, 1));
                        assert_eq!(state.clears, 0);
                    }
                    if !cleanup_on_drop {
                        lease.tick(Duration::ZERO).unwrap();
                    }
                    drop(lease);
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), "");
                    assert_eq!(state.clears, 1);
                    assert_eq!((state.open_calls, state.opens, state.closes), (3, 2, 2));
                    assert_eq!(state.system_frees, transfers);
                    assert_eq!(state.client_frees, client_frees);
                }
            }
        }

        #[test]
        fn failed_publication_cleanup_preserves_a_newer_identical_user_copy() {
            for (fault, user_sequence_if_retained, user_sequence_if_rolled_back) in [
                (Fault::TextRead, 112, 113),
                (Fault::Set(9), 110, 111),
                (Fault::Set(10), 111, 112),
            ] {
                for rollback_fails in [false, true] {
                    let mut faults = vec![fault];
                    if rollback_fails {
                        faults.push(Fault::Empty(2));
                    }
                    let state = Arc::new(Mutex::new(FakeState {
                        faults,
                        race_at_capture: true,
                        ..FakeState::default()
                    }));
                    let mut lease =
                        ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
                    assert!(lease
                        .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                        .is_err());
                    lease.tick(CLEAR_AFTER).unwrap();
                    drop(lease);
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), FAKE_TEXT);
                    assert_eq!(state.blocked_copies, 1);
                    assert_eq!(
                        state.user_sequence,
                        Some(if rollback_fails {
                            user_sequence_if_retained
                        } else {
                            user_sequence_if_rolled_back
                        })
                    );
                    assert_eq!(state.clears, if rollback_fails { 0 } else { 1 });
                    // A rolled-back publication owns nothing, so it never reopens (1A-R6-03).
                    let opens = if rollback_fails { 2 } else { 1 };
                    assert_eq!(
                        (state.open_calls, state.opens, state.closes),
                        (opens, opens, opens)
                    );
                }
            }
        }

        // 1A-R5-02: an unreadable or zero-sequence cleanup is uncertain, never a newer owner.
        #[test]
        fn unreadable_or_zero_sequence_cleanup_keeps_the_lease_and_reports_uncertainty() {
            for fault in [Fault::TextRead, Fault::Sequence] {
                let state = Arc::new(Mutex::new(FakeState::default()));
                let mut lease =
                    ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
                let owned = lease
                    .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                    .unwrap();
                state.lock().unwrap().faults.push(fault);
                let error = lease.tick(CLEAR_AFTER).unwrap_err();
                assert_eq!(error.kind, "clipboardUncertain");
                assert!(!error.message.contains(FAKE_TEXT));
                assert_eq!(lease.owned_generation(), Some(owned));
                {
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), FAKE_TEXT);
                    assert_eq!(state.clears, 0);
                }
                lease.tick(CLEAR_AFTER).unwrap();
                assert_eq!(lease.owned_generation(), None);
                assert_eq!(state.lock().unwrap().text(), "");
            }
        }

        #[test]
        fn same_sequence_with_different_text_is_uncertain_until_a_real_change() {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let mut lease =
                ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
            let owned = lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            let other = vec![b'o', 0, b't', 0, b'h', 0, b'e', 0, b'r', 0, 0, 0];
            state
                .lock()
                .unwrap()
                .formats
                .insert(UNICODE_TEXT, other.clone());
            assert_eq!(
                lease.tick(CLEAR_AFTER).unwrap_err().kind,
                "clipboardUncertain"
            );
            assert_eq!(lease.owned_generation(), Some(owned));
            state.lock().unwrap().user_copy(other);
            lease.tick(CLEAR_AFTER).unwrap();
            assert_eq!(lease.owned_generation(), None);
            let state = state.lock().unwrap();
            assert_eq!(state.text(), "other");
            assert_eq!(state.clears, 0);
        }

        #[test]
        fn a_zero_sequence_publication_that_could_not_roll_back_reports_final_uncertainty() {
            for rollback_fails in [true, false] {
                let mut faults = vec![Fault::Sequence];
                if rollback_fails {
                    // The rollback samples the sequence again; zero there too means untracked.
                    faults.extend([Fault::Sequence, Fault::Empty(2)]);
                }
                let state = Arc::new(Mutex::new(FakeState {
                    faults,
                    ..FakeState::default()
                }));
                let mut lease =
                    ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
                let failure = lease
                    .copy_reported(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                    .err()
                    .unwrap();
                assert_eq!(failure.error.kind, "clipboard");
                assert_eq!(lease.owned_generation(), None);
                let state = state.lock().unwrap();
                if rollback_fails {
                    let cleanup = failure.cleanup.expect("an unverifiable copy is reported");
                    assert_eq!(cleanup.error.kind, "clipboardUncertain");
                    assert!(!cleanup.error.message.contains(FAKE_TEXT));
                    assert_eq!(
                        state.text(),
                        FAKE_TEXT,
                        "a zero sequence never authorizes a clear"
                    );
                    assert_eq!(state.clears, 0);
                } else {
                    assert!(failure.cleanup.is_none());
                    assert_eq!(state.text(), "");
                }
            }
        }

        // 1A-R6-03: a publication that rolled back owns nothing, so a busy reopen is no failure.
        #[test]
        fn a_rolled_back_publication_keeps_no_lease_even_when_the_next_open_is_busy() {
            for fault in [Fault::Sequence, Fault::TextRead] {
                let state = Arc::new(Mutex::new(FakeState {
                    faults: vec![fault, Fault::Open(2)],
                    ..FakeState::default()
                }));
                let mut lease =
                    ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
                let failure = lease
                    .copy_reported(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                    .err()
                    .unwrap();
                assert_eq!(failure.error.kind, "clipboard");
                assert!(failure.cleanup.is_none());
                assert_eq!(lease.owned_generation(), None);
                let state = state.lock().unwrap();
                assert_eq!(state.text(), "");
                assert_eq!(state.clears, 1);
            }
        }

        #[test]
        fn partial_publication_with_an_unreadable_cleanup_keeps_its_lease_and_reports_it() {
            let state = Arc::new(Mutex::new(FakeState {
                // A failed set rolls back at once (no read-back); the immediate cleanup reads.
                faults: vec![Fault::Set(10), Fault::Empty(2), Fault::TextRead],
                ..FakeState::default()
            }));
            let mut lease =
                ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
            let failure = lease
                .copy_reported(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .err()
                .unwrap();
            let cleanup = failure.cleanup.expect("the uncertain cleanup is reported");
            assert_eq!(cleanup.error.kind, "clipboardUncertain");
            assert_eq!(lease.owned_generation(), Some(cleanup.lease_generation));
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
            lease.tick(Duration::ZERO).unwrap();
            assert_eq!(lease.owned_generation(), None);
            assert_eq!(state.lock().unwrap().text(), "");
        }

        #[test]
        fn native_transaction_lease_expires_without_freeing_windows_owned_memory_twice() {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let mut lease =
                ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
            lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            lease.tick(Duration::from_secs(29)).unwrap();
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
            lease.tick(Duration::from_secs(30)).unwrap();
            drop(lease);
            let state = state.lock().unwrap();
            assert_eq!(state.text(), "");
            assert_eq!(state.clears, 1);
            assert_eq!(
                (
                    state.allocations,
                    state.transfers,
                    state.client_frees,
                    state.system_frees
                ),
                (10, 10, 0, 10)
            );
            assert_eq!((state.open_calls, state.opens, state.closes), (2, 2, 2));
        }

        // Unit 1h: delayed rendering, release results and the encoding seam.

        const NOT_RELEASED_TEXT: &str = "Windows did not confirm that AKM released the clipboard.";

        fn fresh() -> (Arc<Mutex<FakeState>>, TransactionBackend) {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let backend = TransactionBackend(FakeApi(state.clone()));
            (state, backend)
        }

        // 1H-03: the sequence is checked again inside the open interval, right before the empty.
        #[test]
        fn cleanup_never_empties_a_newer_copy_that_a_delayed_render_sets_inside_its_open_interval()
        {
            let (state, mut backend) = fresh();
            let published = backend.set_private(FAKE_TEXT).published_for_test();
            let clears_before = state.lock().unwrap().clears;
            state.lock().unwrap().render_on_read = true;
            assert!(!backend.clear_matching(published, FAKE_TEXT).unwrap());
            let state = state.lock().unwrap();
            assert_eq!(state.text(), FAKE_TEXT, "the newer identical copy stays");
            assert_eq!(state.clears, clears_before, "nothing was emptied");
            assert!(state.sequence > published);
            assert_eq!(state.user_sequence, Some(state.sequence));
            assert_eq!((state.opens, state.closes), (2, 2));
        }

        #[test]
        fn a_lease_that_met_a_delayed_render_ends_without_a_warning_or_an_empty() {
            let (state, backend) = fresh();
            let mut lease = ClipboardLease::new(Box::new(backend));
            lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            let clears_before = state.lock().unwrap().clears;
            state.lock().unwrap().render_on_read = true;
            lease.clear().unwrap();
            assert_eq!(lease.owned_generation(), None);
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
            assert_eq!(state.lock().unwrap().clears, clears_before);
        }

        #[test]
        fn a_sequence_that_turns_zero_before_the_empty_is_uncertain_and_empties_nothing() {
            let (state, mut backend) = fresh();
            let published = backend.set_private(FAKE_TEXT).published_for_test();
            // The first sample in the cleanup passes; the recheck reads zero.
            let base = state.lock().unwrap().sequence_calls;
            state.lock().unwrap().faults = vec![Fault::SequenceCall(base + 2)];
            let error = backend.clear_matching(published, FAKE_TEXT).unwrap_err();
            assert_eq!(error.kind, "clipboardUncertain");
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
        }

        // 1H-06: the phase reached and the failed release decide the outcome (design D5).
        fn phases() -> Vec<(&'static str, Vec<Fault>, &'static str, &'static str)> {
            // Columns: row, faults before the release fault, outcome with a clean release,
            // outcome after a failed CloseClipboard.
            vec![
                (
                    "empty fails, sequence unchanged",
                    vec![Fault::Empty(1)],
                    "Untouched",
                    "Untouched",
                ),
                (
                    "empty fails, sequence changed",
                    vec![Fault::EmptyChanged(1)],
                    "EarlierCopyUncertain",
                    "EarlierCopyUncertain",
                ),
                (
                    "exclusion set fails",
                    vec![Fault::Set(1)],
                    "Emptied",
                    "Emptied",
                ),
                ("published", vec![], "Published", "ResidualUntracked"),
                (
                    "rollback fails, sequence unchanged",
                    vec![Fault::Set(9), Fault::Empty(2)],
                    "Residual",
                    "ResidualUntracked",
                ),
                (
                    "rollback fails, sequence changed",
                    vec![Fault::Set(9), Fault::EmptyChanged(2)],
                    "ResidualUntracked",
                    "ResidualUntracked",
                ),
            ]
        }

        #[test]
        fn a_failed_close_changes_the_publication_outcome_as_the_release_table_says() {
            for (row, faults, clean, closed) in phases() {
                let (state, mut backend) = fresh();
                state.lock().unwrap().faults = faults.clone();
                let publication = backend.set_private(FAKE_TEXT);
                assert_eq!(publication.name_for_test(), clean, "{row}: clean release");

                let (state, mut backend) = fresh();
                let mut with_fault = faults.clone();
                with_fault.push(Fault::CloseFail(1));
                state.lock().unwrap().faults = with_fault;
                let publication = backend.set_private(FAKE_TEXT);
                assert_eq!(publication.name_for_test(), closed, "{row}: failed close");
                assert!(
                    publication
                        .error_for_test()
                        .message
                        .contains(NOT_RELEASED_TEXT),
                    "{row}"
                );
                let state = state.lock().unwrap();
                assert!(state.faults.is_empty(), "{row}: every fault was reached");
                assert_eq!((state.opens, state.closes), (1, 1), "{row}");
            }
        }

        #[test]
        fn a_failed_owner_destruction_never_changes_the_outcome_in_any_phase() {
            for (row, faults, clean, _) in phases() {
                let (state, mut backend) = fresh();
                let mut with_fault = faults.clone();
                with_fault.push(Fault::DestroyFail(1));
                state.lock().unwrap().faults = with_fault;
                let publication = backend.set_private(FAKE_TEXT);
                assert!(publication.owner_leaked(), "{row}");
                assert_eq!(publication.name_for_test(), clean, "{row}");
                let state = state.lock().unwrap();
                assert!(state.faults.is_empty(), "{row}: every fault was reached");
                assert_eq!(
                    (state.leaked_owners, state.destroyed_owners),
                    (1, 0),
                    "{row}"
                );
                assert_eq!((state.opens, state.closes), (1, 1), "{row}");
            }
        }

        #[test]
        fn a_published_copy_whose_close_failed_leaves_no_lease_and_one_uncertainty() {
            let (state, backend) = fresh();
            let mut lease = ClipboardLease::new(Box::new(backend));
            state.lock().unwrap().faults = vec![Fault::CloseFail(1)];
            let failure = lease
                .copy_reported(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .err()
                .unwrap();
            assert!(failure.error.message.starts_with(NOT_RELEASED_TEXT));
            assert!(failure
                .error
                .message
                .ends_with(&uncertain_cleanup().message));
            assert_eq!(
                failure.cleanup.unwrap().error.message,
                uncertain_cleanup().message
            );
            assert_eq!(
                lease.owned_generation(),
                None,
                "a sequence taken before a failed close is not evidence"
            );
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
        }

        #[test]
        fn a_failed_owner_destruction_keeps_the_publication_and_the_lease_and_leaves_one_note() {
            let (state, backend) = fresh();
            let mut lease = ClipboardLease::new(Box::new(backend));
            state.lock().unwrap().faults = vec![Fault::DestroyFail(1)];
            lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            assert!(lease.owned_generation().is_some());
            assert_eq!(lease.take_notes(), vec![LeaseNote::OwnerWindowLeft]);
            assert!(lease.take_notes().is_empty());
            assert_eq!(state.lock().unwrap().leaked_owners, 1);
            lease.tick(Duration::from_secs(30)).unwrap();
            assert_eq!(state.lock().unwrap().text(), "");
            assert!(lease.take_notes().is_empty(), "the note is not repeated");
        }

        #[test]
        fn a_failed_close_after_cleanup_ends_the_lease_with_the_not_released_error() {
            for newer_copy in [false, true] {
                let (state, backend) = fresh();
                let mut lease = ClipboardLease::new(Box::new(backend));
                lease
                    .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                    .unwrap();
                if newer_copy {
                    state.lock().unwrap().render_on_read = true;
                }
                // Close 1 ended the copy; close 2 ends the cleanup.
                state.lock().unwrap().faults = vec![Fault::CloseFail(2)];
                let error = lease.clear().unwrap_err();
                assert_eq!(error.kind, NOT_RELEASED, "newer_copy={newer_copy}");
                assert_eq!(error.message, not_released().message);
                assert_eq!(lease.owned_generation(), None, "newer_copy={newer_copy}");
                assert_eq!(
                    state.lock().unwrap().text(),
                    if newer_copy { FAKE_TEXT } else { "" }
                );
            }
        }

        #[test]
        fn a_failed_close_after_an_uncertain_cleanup_keeps_the_uncertain_result_and_the_lease() {
            let (state, backend) = fresh();
            let mut lease = ClipboardLease::new(Box::new(backend));
            lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            state.lock().unwrap().faults = vec![Fault::TextRead, Fault::CloseFail(2)];
            let error = lease.clear().unwrap_err();
            assert_eq!(error.kind, "clipboardUncertain");
            assert!(lease.owned_generation().is_some(), "retry stays possible");
        }

        // 1H-07: the encoding is a call of the API, so it can fail and be observed.
        #[test]
        fn an_encoding_failure_changes_nothing_and_frees_what_it_allocated() {
            let (state, mut backend) = fresh();
            state.lock().unwrap().faults = vec![Fault::Encode];
            let error = backend.set_private(FAKE_TEXT).error_for_test();
            assert_eq!(error.message, "injected encoding failure");
            let state = state.lock().unwrap();
            assert_eq!((state.opens, state.sequence), (0, 100));
            assert!(state.formats.is_empty());
            assert_eq!(state.allocations, state.client_frees);
        }

        #[test]
        fn an_encoding_failure_keeps_an_earlier_lease_and_says_so() {
            let (state, backend) = fresh();
            let mut lease = ClipboardLease::new(Box::new(backend));
            let first = lease
                .copy(Zeroizing::new(FAKE_TEXT.into()), Duration::ZERO)
                .unwrap();
            state.lock().unwrap().faults = vec![Fault::Encode];
            let error = lease
                .copy(Zeroizing::new("fake-second".into()), Duration::ZERO)
                .unwrap_err();
            assert!(error.message.ends_with(EARLIER_MAY_REMAIN));
            assert_eq!(lease.owned_generation(), Some(first));
            assert_eq!(state.lock().unwrap().text(), FAKE_TEXT);
        }

        #[test]
        fn the_fake_encodes_with_a_fixed_locale_so_no_test_depends_on_the_machine() {
            let (state, mut backend) = fresh();
            backend.set_private("caf\u{e9}").published_for_test();
            let state = state.lock().unwrap();
            assert_eq!(state.formats[&TEXT_LOCALE], [9, 4, 0, 0]);
            assert_eq!(state.formats[&ANSI_TEXT], [b'c', b'a', b'f', 0xe9, 0]);
        }
    }
}

#[cfg(not(windows))]
impl ClipboardBackend for NativeClipboard {
    fn set_private(&mut self, _text: &str) -> Result<CopyAttempt, CommandError> {
        Err(CommandError::new(
            "clipboard",
            "Private clipboard is implemented for Windows only.",
        ))
    }
    fn clear_matching(&mut self, _sequence: u32, _text: &str) -> Result<bool, CommandError> {
        Ok(false)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct FakeData {
        pub sequence: u32,
        pub text: String,
        pub clears: usize,
    }
    pub struct FakeClipboard(pub Arc<Mutex<FakeData>>);
    impl ClipboardBackend for FakeClipboard {
        fn set_private(&mut self, text: &str) -> Publication {
            let mut data = self.0.lock().unwrap();
            data.sequence += 1;
            data.text = text.into();
            Publication::for_test(Outcome::Published(LeaseEvidence::for_test(data.sequence)))
        }
        fn clear_matching(&mut self, seq: u32, text: &str) -> Result<bool, CommandError> {
            let mut data = self.0.lock().unwrap();
            if data.sequence != seq || data.text != text {
                return Ok(false);
            }
            data.text.clear();
            data.clears += 1;
            Ok(true)
        }
    }

    #[test]
    fn old_expiry_cannot_clear_a_new_copy_even_of_the_same_secret() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let mut lease = ClipboardLease::new(Box::new(FakeClipboard(data.clone())));
        let old = lease
            .copy(Zeroizing::new("fake".into()), Duration::ZERO)
            .unwrap();
        lease
            .copy(Zeroizing::new("fake".into()), Duration::from_secs(20))
            .unwrap();
        lease.expire(old, Duration::from_secs(60)).unwrap();
        assert_eq!(data.lock().unwrap().text, "fake");
        lease.tick(Duration::from_secs(50)).unwrap();
        assert_eq!(data.lock().unwrap().clears, 1);
    }

    #[test]
    fn a_later_users_clipboard_is_preserved_even_when_its_text_matches() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let mut lease = ClipboardLease::new(Box::new(FakeClipboard(data.clone())));
        lease
            .copy(Zeroizing::new("fake".into()), Duration::ZERO)
            .unwrap();
        data.lock().unwrap().sequence += 1;
        lease.clear().unwrap();
        assert_eq!(data.lock().unwrap().text, "fake");
        assert_eq!(data.lock().unwrap().clears, 0);
    }

    #[test]
    fn matching_sequence_alone_does_not_authorize_clear() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let mut lease = ClipboardLease::new(Box::new(FakeClipboard(data.clone())));
        lease
            .copy(Zeroizing::new("fake".into()), Duration::ZERO)
            .unwrap();
        data.lock().unwrap().text = "user text".into();
        lease.clear().unwrap();
        assert_eq!(data.lock().unwrap().text, "user text");
    }

    struct PartialFailure {
        fake: FakeClipboard,
        busy_once: bool,
    }
    impl ClipboardBackend for PartialFailure {
        fn set_private(&mut self, text: &str) -> Publication {
            // A partial write whose rollback failed: the value may remain under this sequence.
            let sequence = self.fake.set_private(text).published_for_test();
            Publication::for_test(Outcome::Residual(
                CommandError::new("clipboard", "injected exclusion failure"),
                LeaseEvidence::for_test(sequence),
            ))
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            if self.busy_once {
                self.busy_once = false;
                return Err(CommandError::new("clipboard", "injected busy clipboard"));
            }
            self.fake.clear_matching(sequence, text)
        }
    }

    /// Scripted results for the lease state table. Each call takes the next scripted result.
    struct Scripted {
        publish: Arc<Mutex<Vec<Outcome>>>,
        clears: Arc<Mutex<Vec<Result<bool, CommandError>>>>,
        publish_calls: Arc<Mutex<usize>>,
        clear_calls: Arc<Mutex<usize>>,
    }
    impl ClipboardBackend for Scripted {
        fn set_private(&mut self, _text: &str) -> Publication {
            *self.publish_calls.lock().unwrap() += 1;
            Publication::for_test(self.publish.lock().unwrap().remove(0))
        }
        fn clear_matching(&mut self, _sequence: u32, _text: &str) -> Result<bool, CommandError> {
            *self.clear_calls.lock().unwrap() += 1;
            // Unscripted: the lease's own Drop at the end of a case, after the assertions.
            let mut clears = self.clears.lock().unwrap();
            if clears.is_empty() {
                Ok(true)
            } else {
                clears.remove(0)
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Event {
        CopyOk,
        CopyRefused,
        CopyNul,
        CopyEarlierUncertain,
        CopyEmptied,
        CopyResidual,
        CopyResidualUntracked,
        TickDue,
        TickEarly,
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Owner {
        Nothing,
        Prior,
        New,
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Clear {
        Cleared,
        NotOurs,
        Busy,
        Uncertain,
        Lost,
    }

    fn scripted_clear(clear: Clear) -> Result<bool, CommandError> {
        match clear {
            Clear::Cleared => Ok(true),
            Clear::NotOurs => Ok(false),
            Clear::Busy => Err(CommandError::new(
                "clipboard",
                "The clipboard is busy. Try copying again.",
            )),
            Clear::Uncertain => Err(uncertain_cleanup()),
            Clear::Lost => Err(CommandError::new(LOST, "injected EmptyClipboard failure")),
        }
    }

    /// A copy row: event, scripted cleanup, owner after, returned error kind, reported cleanup kind, cleanup calls.
    type CopyRow = (
        Event,
        Option<Clear>,
        Owner,
        Option<&'static str>,
        Option<&'static str>,
        usize,
    );
    /// A tick row: event, scripted cleanup, owner after, reported cleanup kind, cleanup calls.
    type TickRow = (Event, Option<Clear>, Owner, Option<&'static str>, usize);

    // 1A-R7-02 and 1A-R7-03: every lease transition, from no lease and from an owned lease.
    // Columns: event, scripted cleanup result, owner after, error kind returned, cleanup
    // failure kind reported, cleanup calls.
    #[test]
    fn the_lease_state_table_holds_for_every_event_from_every_state() {
        use Clear::*;
        use Event::*;
        use Owner::*;
        let rows: Vec<CopyRow> = vec![
            (CopyOk, None, New, None, None, 0),
            (CopyRefused, None, Prior, Some("clipboard"), None, 0),
            (CopyNul, None, Prior, Some("clipboard"), None, 0),
            // The publication emptied the clipboard first, so an earlier lease owns nothing.
            (CopyEmptied, None, Nothing, Some("clipboard"), None, 0),
            // A failed first empty changed the sequence: the earlier lease ends with a warning.
            (
                CopyEarlierUncertain,
                None,
                Nothing,
                Some("clipboard"),
                Some("clipboardUncertain"),
                0,
            ),
            (
                CopyResidual,
                Some(Cleared),
                Nothing,
                Some("clipboard"),
                None,
                1,
            ),
            (
                CopyResidual,
                Some(NotOurs),
                Nothing,
                Some("clipboard"),
                None,
                1,
            ),
            (
                CopyResidual,
                Some(Busy),
                New,
                Some("clipboard"),
                Some("clipboardUncertain"),
                1,
            ),
            (
                CopyResidual,
                Some(Uncertain),
                New,
                Some("clipboard"),
                Some("clipboardUncertain"),
                1,
            ),
            (
                CopyResidual,
                Some(Lost),
                Nothing,
                Some("clipboard"),
                Some("clipboardUncertain"),
                1,
            ),
            (
                CopyResidualUntracked,
                None,
                Nothing,
                Some("clipboard"),
                Some("clipboardUncertain"),
                0,
            ),
        ];
        let ticks: Vec<TickRow> = vec![
            (TickDue, Some(Cleared), Nothing, None, 1),
            (TickDue, Some(NotOurs), Nothing, None, 1),
            (TickDue, Some(Busy), Prior, Some("clipboardUncertain"), 1),
            (
                TickDue,
                Some(Uncertain),
                Prior,
                Some("clipboardUncertain"),
                1,
            ),
            (TickDue, Some(Lost), Nothing, Some("clipboardUncertain"), 1),
            (TickEarly, None, Prior, None, 0),
        ];
        let mut cases = Vec::new();
        for prior in [false, true] {
            for row in &rows {
                // With no earlier lease, a changed failed empty has no lease to end or warn about.
                let cleanup = if !prior && matches!(row.0, CopyEarlierUncertain) {
                    None
                } else {
                    row.4
                };
                cases.push((
                    prior,
                    row.0,
                    row.1,
                    if !prior && row.2 == Prior {
                        Nothing
                    } else {
                        row.2
                    },
                    row.3,
                    cleanup,
                    row.5,
                ));
            }
            for row in &ticks {
                let calls = if prior { row.4 } else { 0 };
                let owner = if prior { row.2 } else { Nothing };
                let error = if prior { row.3 } else { None };
                cases.push((
                    prior,
                    row.0,
                    if prior { row.1 } else { None },
                    owner,
                    error,
                    None,
                    calls,
                ));
            }
        }
        for (prior, event, clear, owner, error, cleanup, calls) in cases {
            let case = format!("prior={prior} {event:?} {clear:?}");
            let publish = Arc::new(Mutex::new(Vec::new()));
            let clears = Arc::new(Mutex::new(Vec::new()));
            let clear_calls = Arc::new(Mutex::new(0));
            let mut lease = ClipboardLease::new(Box::new(Scripted {
                publish: publish.clone(),
                clears: clears.clone(),
                publish_calls: Arc::new(Mutex::new(0)),
                clear_calls: clear_calls.clone(),
            }));
            let before = if prior {
                publish
                    .lock()
                    .unwrap()
                    .push(Outcome::Published(LeaseEvidence::for_test(3)));
                Some(
                    lease
                        .copy(Zeroizing::new("fake-prior".into()), Duration::ZERO)
                        .unwrap(),
                )
            } else {
                None
            };
            if let Some(clear) = clear {
                clears.lock().unwrap().push(scripted_clear(clear));
            }
            let text = || Zeroizing::new("fake-table-value".to_string());
            let (returned, reported) = match event {
                CopyOk => {
                    publish
                        .lock()
                        .unwrap()
                        .push(Outcome::Published(LeaseEvidence::for_test(9)));
                    (lease.copy_reported(text(), Duration::ZERO).err(), None)
                }
                CopyRefused => {
                    publish
                        .lock()
                        .unwrap()
                        .push(Outcome::Untouched(CommandError::new(
                            "clipboard",
                            "fake refused open",
                        )));
                    (lease.copy_reported(text(), Duration::ZERO).err(), None)
                }
                CopyNul => (
                    lease
                        .copy_reported(Zeroizing::new("fake\0nul".into()), Duration::ZERO)
                        .err(),
                    None,
                ),
                CopyEarlierUncertain | CopyEmptied | CopyResidual | CopyResidualUntracked => {
                    let error = CommandError::new("clipboard", "fake partial publication");
                    publish.lock().unwrap().push(match event {
                        CopyEarlierUncertain => Outcome::EarlierCopyUncertain(error),
                        CopyEmptied => Outcome::Emptied(error),
                        CopyResidual => Outcome::Residual(error, LeaseEvidence::for_test(7)),
                        _ => Outcome::ResidualUntracked(error),
                    });
                    (lease.copy_reported(text(), Duration::ZERO).err(), None)
                }
                TickDue => (None, lease.tick(CLEAR_AFTER).err()),
                TickEarly => (None, lease.tick(Duration::from_secs(1)).err()),
            };
            let (returned_kind, cleanup_kind) = match returned {
                Some(failure) => (
                    Some(failure.error.kind.clone()),
                    failure.cleanup.map(|cleanup| cleanup.error),
                ),
                None => (None, None),
            };
            let cleanup_error = cleanup_kind.or(reported);
            let after = match (lease.owned_generation(), before) {
                (None, _) => Nothing,
                (Some(now), Some(then)) if now == then => Prior,
                (Some(_), _) => New,
            };
            assert_eq!(after, owner, "{case}");
            let (expected_returned, expected_cleanup) = match event {
                TickDue | TickEarly => (None, error),
                _ => (error, cleanup),
            };
            assert_eq!(returned_kind.as_deref(), expected_returned, "{case}");
            assert_eq!(
                cleanup_error.as_ref().map(|e| e.kind.as_str()),
                expected_cleanup,
                "{case}"
            );
            if let Some(error) = &cleanup_error {
                // History only: a kept warning never promises a later retry or a cause.
                assert_eq!(error.message, uncertain_cleanup().message, "{case}");
                assert!(
                    !error.message.contains("try again") && !error.message.contains("will"),
                    "{case}"
                );
                assert!(!error.message.contains("fake-"), "{case}");
            }
            assert_eq!(*clear_calls.lock().unwrap(), calls, "{case}");
        }
    }

    #[test]
    fn partial_copy_failure_keeps_a_lease_until_busy_clipboard_cleanup_can_retry() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let mut lease = ClipboardLease::new(Box::new(PartialFailure {
            fake: FakeClipboard(data.clone()),
            busy_once: true,
        }));
        assert!(lease
            .copy(Zeroizing::new("fake".into()), Duration::ZERO)
            .is_err());
        assert_eq!(data.lock().unwrap().text, "fake");
        lease.tick(Duration::from_secs(1)).unwrap();
        assert_eq!(data.lock().unwrap().text, "");
        assert_eq!(data.lock().unwrap().clears, 1);
    }
}
