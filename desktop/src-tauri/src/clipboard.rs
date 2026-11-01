use std::time::Duration;
use zeroize::Zeroizing;

use crate::error::CommandError;

pub const CLEAR_AFTER: Duration = Duration::from_secs(30);

pub struct CopyAttempt {
    pub sequence: u32,
    pub failure: Option<CommandError>,
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
    fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError>;
    fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError>;
}

struct Lease {
    generation: u64,
    sequence: u32,
    text: Zeroizing<String>,
    expires: Duration,
}

pub struct ClipboardLease {
    backend: Box<dyn ClipboardBackend>,
    generation: u64,
    owned: Option<Lease>,
}

impl ClipboardLease {
    pub fn new(backend: Box<dyn ClipboardBackend>) -> Self {
        Self {
            backend,
            generation: 0,
            owned: None,
        }
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
            return Err(CommandError::new(
                "clipboard",
                "Clipboard text cannot contain a NUL character.",
            )
            .into());
        }
        let attempt = self.backend.set_private(&text)?;
        self.generation = self.generation.wrapping_add(1);
        self.owned = Some(Lease {
            generation: self.generation,
            sequence: attempt.sequence,
            text,
            expires: if attempt.failure.is_some() {
                now
            } else {
                now + CLEAR_AFTER
            },
        });
        if let Some(error) = attempt.failure {
            // A partial OS write still needs an owned lease if immediate cleanup finds a busy clipboard.
            let lease_generation = self.generation;
            let cleanup = self.clear().err().map(|error| CleanupFailure {
                lease_generation,
                error,
            });
            return Err(CopyFailure { error, cleanup });
        }
        Ok(self.generation)
    }

    pub fn clear(&mut self) -> Result<(), CommandError> {
        if let Some(lease) = &self.owned {
            self.backend.clear_matching(lease.sequence, &lease.text)?;
        }
        self.owned = None;
        Ok(())
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
        let _ = self.clear();
    }
}

pub struct NativeClipboard;

#[cfg(windows)]
mod windows {
    use super::*;
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

    const ANSI_TEXT: u32 = 1;
    const OEM_TEXT: u32 = 7;
    const UNICODE_TEXT: u32 = 13;
    const TEXT_LOCALE: u32 = 16;
    const PRIVATE_FORMATS: [&str; 3] = [
        "CanIncludeInClipboardHistory",
        "CanUploadToCloudClipboard",
        "ExcludeClipboardContentFromMonitorProcessing",
    ];

    // This boundary models individual Win32 calls, not a second publication algorithm.
    trait ClipboardApi {
        type Owner;
        type Memory;

        fn owner(&mut self) -> Result<Self::Owner, CommandError>;
        fn register(&mut self, name: &str) -> Result<u32, CommandError>;
        fn allocate(&mut self, bytes: &[u8]) -> Result<Self::Memory, CommandError>;
        fn open(&mut self, owner: Option<&Self::Owner>) -> Result<(), CommandError>;
        fn close(&mut self);
        fn is_empty(&mut self) -> bool;
        fn empty(&mut self) -> Result<(), CommandError>;
        fn set(&mut self, format: u32, memory: &mut Self::Memory) -> Result<(), CommandError>;
        fn sequence(&mut self) -> u32;
        fn matches_text(&mut self, expected: &str) -> bool;
        fn matches_zero(&mut self, format: u32) -> bool;
    }

    struct Open<'a, A: ClipboardApi> {
        api: &'a mut A,
        _owner: Option<&'a A::Owner>,
        _thread: std::marker::PhantomData<*mut ()>,
    }
    impl<'a, A: ClipboardApi> Open<'a, A> {
        fn acquire(api: &'a mut A, owner: Option<&'a A::Owner>) -> Result<Self, CommandError> {
            api.open(owner)?;
            Ok(Self {
                api,
                _owner: owner,
                _thread: std::marker::PhantomData,
            })
        }
    }
    impl<A: ClipboardApi> Drop for Open<'_, A> {
        fn drop(&mut self) {
            self.api.close();
        }
    }

    struct TextFormats {
        locale: u32,
        values: [(u32, Zeroizing<Vec<u8>>); 3],
    }

    impl TextFormats {
        fn new(text: &str, locale: u32) -> Result<Self, CommandError> {
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

    fn publish<A: ClipboardApi>(
        api: &mut A,
        text: &str,
        require_empty: bool,
    ) -> Result<CopyAttempt, CommandError> {
        let mut exclusions = Vec::with_capacity(PRIVATE_FORMATS.len());
        for name in PRIVATE_FORMATS {
            exclusions.push((api.register(name)?, api.allocate(&0u32.to_ne_bytes())?));
        }
        // CF_LOCALE explicitly describes both legacy encodings, independent of the input language.
        let encoded = TextFormats::new(text, unsafe { GetSystemDefaultLCID() })?;
        let mut frame = [
            (UNICODE_TEXT, api.allocate(&0u16.to_ne_bytes())?),
            (ANSI_TEXT, api.allocate(&[0])?),
            (OEM_TEXT, api.allocate(&[0])?),
            (TEXT_LOCALE, api.allocate(&encoded.locale.to_ne_bytes())?),
        ];
        let mut values = Vec::with_capacity(encoded.values.len());
        for (format, bytes) in &encoded.values {
            values.push((*format, api.allocate(bytes)?));
        }
        let owner = api.owner()?;
        let open = Open::acquire(api, Some(&owner))?;
        if require_empty && !open.api.is_empty() {
            return Err(CommandError::new(
                "clipboard",
                "Refusing to overwrite the user's clipboard.",
            ));
        }
        open.api.empty()?;
        // Never publish text if an exclusion could not be installed.
        for (format, memory) in &mut exclusions {
            if let Err(error) = open.api.set(*format, memory) {
                let _ = open.api.empty();
                return Err(error);
            }
        }
        if exclusions
            .iter()
            .any(|(format, _)| !open.api.matches_zero(*format))
        {
            let _ = open.api.empty();
            return Err(CommandError::new(
                "clipboard",
                "Windows did not apply clipboard history/cloud/monitoring exclusions. The copy was cancelled.",
            ));
        }
        // CloseClipboard adds missing text/locale formats and changes the sequence.
        // Install the complete, empty format set before any secret bytes. Even a later
        // failed SetClipboardData plus a failed rollback then leaves no synthesis work.
        // Unicode is first so consumers enumerating formats prefer the lossless value.
        for (format, memory) in &mut frame {
            if let Err(error) = open.api.set(*format, memory) {
                let _ = open.api.empty();
                return Err(error);
            }
        }
        let mut failure = None;
        for (format, memory) in &mut values {
            if let Err(error) = open.api.set(*format, memory) {
                failure = Some(error);
                break;
            }
        }
        // No other writer can publish between our SetClipboardData and this ownership sample.
        let verified = open.api.matches_text(text);
        let sequence = open.api.sequence();
        if failure.is_none() && (sequence == 0 || !verified) {
            failure = Some(CommandError::new(
                "clipboard",
                "The clipboard changed while copying. Copy again.",
            ));
        }
        if failure.is_some() {
            // Retain the captured lease if Windows refuses this rollback and later cleanup is busy.
            let _ = open.api.empty();
        }
        Ok(CopyAttempt { sequence, failure })
    }

    fn clear<A: ClipboardApi>(
        api: &mut A,
        sequence: u32,
        text: &str,
    ) -> Result<bool, CommandError> {
        let open = Open::acquire(api, None)?;
        // The comparison and clear share one OS clipboard lock.
        if sequence == 0 || open.api.sequence() != sequence || !open.api.matches_text(text) {
            return Ok(false);
        }
        open.api.empty()?;
        Ok(true)
    }

    struct Win32;

    struct Owner(HWND);
    impl Drop for Owner {
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

        fn close(&mut self) {
            unsafe {
                CloseClipboard();
            }
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

        fn matches_text(&mut self, expected: &str) -> bool {
            // No pointer outlives the exclusive Windows clipboard-open interval.
            unsafe {
                let handle = GetClipboardData(UNICODE_TEXT);
                if handle.is_null() {
                    return false;
                }
                let count = GlobalSize(handle) / 2;
                let ptr = GlobalLock(handle).cast::<u16>();
                if ptr.is_null() {
                    return false;
                }
                let units = std::slice::from_raw_parts(ptr, count);
                let matches = units
                    .iter()
                    .position(|unit| *unit == 0)
                    .is_some_and(|end| units[..end].iter().copied().eq(expected.encode_utf16()));
                GlobalUnlock(handle);
                matches
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
        fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
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
        let attempt = publish(&mut Win32, FAKE_VALUE, true).unwrap();
        let mut backend = NativeClipboard;
        let seq = attempt.sequence;
        let result = std::panic::catch_unwind(|| {
            assert!(attempt.failure.is_none());
            let mut api = Win32;
            let open = Open::acquire(&mut api, None).unwrap();
            assert_eq!(
                open.api.sequence(),
                seq,
                "the captured sequence must survive owner destruction"
            );
            assert!(open.api.matches_text(FAKE_VALUE));
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

    #[cfg(test)]
    mod transaction_tests {
        use super::*;
        use std::{
            collections::BTreeMap,
            sync::{Arc, Mutex},
        };

        const FAKE_TEXT: &str = "AKM fake transaction secret";

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Fault {
            Register(usize),
            Allocate(usize),
            Owner,
            Open(usize),
            Empty(usize),
            Set(usize),
            TextRead,
            ZeroRead(u32),
            Sequence,
        }

        struct FakeState {
            open: bool,
            open_with_owner: bool,
            owner_alive: bool,
            changed: bool,
            synthesized: usize,
            formats: BTreeMap<u32, Vec<u8>>,
            sequence: u32,
            race_at_capture: bool,
            pending_user_copy: Option<Vec<u8>>,
            blocked_copies: usize,
            user_sequence: Option<u32>,
            clears: usize,
            registered_names: Vec<String>,
            open_calls: usize,
            opens: usize,
            closes: usize,
            destroyed_owners: usize,
            allocations: usize,
            client_frees: usize,
            transfers: usize,
            system_frees: usize,
            empty_calls: usize,
            set_calls: usize,
            faults: Vec<Fault>,
        }

        impl Default for FakeState {
            fn default() -> Self {
                Self {
                    open: false,
                    open_with_owner: false,
                    owner_alive: false,
                    changed: false,
                    synthesized: 0,
                    formats: BTreeMap::new(),
                    sequence: 100,
                    race_at_capture: false,
                    pending_user_copy: None,
                    blocked_copies: 0,
                    user_sequence: None,
                    clears: 0,
                    registered_names: Vec::new(),
                    open_calls: 0,
                    opens: 0,
                    closes: 0,
                    destroyed_owners: 0,
                    allocations: 0,
                    client_frees: 0,
                    transfers: 0,
                    system_frees: 0,
                    empty_calls: 0,
                    set_calls: 0,
                    faults: Vec::new(),
                }
            }
        }

        impl FakeState {
            fn fail(&mut self, fault: Fault) -> bool {
                if let Some(index) = self.faults.iter().position(|item| *item == fault) {
                    self.faults.remove(index);
                    return true;
                }
                false
            }

            fn user_copy(&mut self, text: Vec<u8>) {
                assert!(!self.open);
                self.system_frees += self.formats.len();
                self.formats.clear();
                self.formats.insert(UNICODE_TEXT, text);
                self.sequence += 1;
                self.user_sequence = Some(self.sequence);
            }

            fn text(&self) -> String {
                self.formats
                    .get(&UNICODE_TEXT)
                    .map_or_else(String::new, |bytes| {
                        let units: Vec<u16> = bytes
                            .chunks_exact(2)
                            .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
                            .take_while(|unit| *unit != 0)
                            .collect();
                        String::from_utf16(&units).unwrap()
                    })
            }
        }

        struct FakeApi(Arc<Mutex<FakeState>>);
        struct FakeOwner(Arc<Mutex<FakeState>>);
        impl Drop for FakeOwner {
            fn drop(&mut self) {
                let mut state = self.0.lock().unwrap();
                assert!(!state.open, "the owner must outlive CloseClipboard");
                state.owner_alive = false;
                state.destroyed_owners += 1;
            }
        }

        struct FakeMemory {
            bytes: Option<Vec<u8>>,
            state: Arc<Mutex<FakeState>>,
        }
        impl Drop for FakeMemory {
            fn drop(&mut self) {
                if let Some(bytes) = &mut self.bytes {
                    bytes.zeroize();
                    self.state.lock().unwrap().client_frees += 1;
                }
            }
        }

        impl ClipboardApi for FakeApi {
            type Owner = FakeOwner;
            type Memory = FakeMemory;

            fn owner(&mut self) -> Result<FakeOwner, CommandError> {
                let mut state = self.0.lock().unwrap();
                if state.fail(Fault::Owner) {
                    return Err(CommandError::new("clipboard", "injected owner failure"));
                }
                state.owner_alive = true;
                Ok(FakeOwner(self.0.clone()))
            }

            fn register(&mut self, name: &str) -> Result<u32, CommandError> {
                let mut state = self.0.lock().unwrap();
                state.registered_names.push(name.to_owned());
                let call = state.registered_names.len();
                if state.fail(Fault::Register(call)) {
                    return Err(CommandError::new(
                        "clipboard",
                        "injected registration failure",
                    ));
                }
                Ok(0xc000
                    + PRIVATE_FORMATS
                        .iter()
                        .position(|item| *item == name)
                        .unwrap() as u32)
            }

            fn allocate(&mut self, bytes: &[u8]) -> Result<Self::Memory, CommandError> {
                let mut state = self.0.lock().unwrap();
                let call = state.allocations + 1;
                if state.fail(Fault::Allocate(call)) {
                    return Err(CommandError::new(
                        "clipboard",
                        "injected allocation failure",
                    ));
                }
                state.allocations += 1;
                Ok(FakeMemory {
                    bytes: Some(bytes.to_vec()),
                    state: self.0.clone(),
                })
            }

            fn open(&mut self, owner: Option<&FakeOwner>) -> Result<(), CommandError> {
                let mut state = self.0.lock().unwrap();
                assert!(!state.open);
                state.open_calls += 1;
                let call = state.open_calls;
                if state.fail(Fault::Open(call)) {
                    return Err(CommandError::new("clipboard", "injected busy clipboard"));
                }
                if owner.is_some() {
                    assert!(state.owner_alive);
                }
                state.open = true;
                state.open_with_owner = owner.is_some();
                state.opens += 1;
                Ok(())
            }

            fn close(&mut self) {
                let mut state = self.0.lock().unwrap();
                assert!(state.open);
                // Windows adds missing text/locale formats on close, before releasing the owner.
                if state.changed
                    && [1, 7, 13]
                        .iter()
                        .any(|format| state.formats.contains_key(format))
                {
                    for (format, bytes) in [
                        (16, vec![9, 4, 0, 0]),
                        (1, vec![]),
                        (7, vec![]),
                        (13, vec![]),
                    ] {
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            state.formats.entry(format)
                        {
                            entry.insert(bytes);
                            state.synthesized += 1;
                            state.sequence += 1;
                        }
                    }
                }
                state.changed = false;
                state.open = false;
                state.closes += 1;
                if let Some(text) = state.pending_user_copy.take() {
                    state.user_copy(text);
                }
            }

            fn is_empty(&mut self) -> bool {
                let state = self.0.lock().unwrap();
                assert!(state.open);
                state.formats.is_empty()
            }

            fn empty(&mut self) -> Result<(), CommandError> {
                let mut state = self.0.lock().unwrap();
                assert!(state.open);
                state.empty_calls += 1;
                let call = state.empty_calls;
                if state.fail(Fault::Empty(call)) {
                    return Err(CommandError::new(
                        "clipboard",
                        "injected EmptyClipboard failure",
                    ));
                }
                if state.formats.contains_key(&UNICODE_TEXT) {
                    state.clears += 1;
                }
                state.system_frees += state.formats.len();
                state.formats.clear();
                state.sequence += 1;
                state.changed = true;
                Ok(())
            }

            fn set(&mut self, format: u32, memory: &mut Self::Memory) -> Result<(), CommandError> {
                let mut state = self.0.lock().unwrap();
                assert!(state.open && state.open_with_owner && state.owner_alive);
                state.set_calls += 1;
                let call = state.set_calls;
                if state.fail(Fault::Set(call)) {
                    return Err(CommandError::new(
                        "clipboard",
                        "injected SetClipboardData failure",
                    ));
                }
                if state
                    .formats
                    .insert(format, memory.bytes.take().unwrap())
                    .is_some()
                {
                    state.system_frees += 1;
                }
                state.transfers += 1;
                state.sequence += 1;
                state.changed = true;
                Ok(())
            }

            fn sequence(&mut self) -> u32 {
                let mut state = self.0.lock().unwrap();
                assert!(
                    state.open,
                    "never adopt a sequence after releasing the clipboard"
                );
                if state.race_at_capture && state.formats.contains_key(&UNICODE_TEXT) {
                    state.race_at_capture = false;
                    let text = state.formats.get(&UNICODE_TEXT).unwrap().clone();
                    state.blocked_copies += 1;
                    state.pending_user_copy = Some(text);
                }
                if state.fail(Fault::Sequence) {
                    return 0;
                }
                state.sequence
            }

            fn matches_text(&mut self, expected: &str) -> bool {
                let mut state = self.0.lock().unwrap();
                assert!(state.open);
                if state.fail(Fault::TextRead) {
                    return false;
                }
                state.formats.contains_key(&UNICODE_TEXT) && state.text() == expected
            }

            fn matches_zero(&mut self, format: u32) -> bool {
                let mut state = self.0.lock().unwrap();
                assert!(state.open);
                if state.fail(Fault::ZeroRead(format)) {
                    return false;
                }
                state
                    .formats
                    .get(&format)
                    .is_some_and(|bytes| bytes == &[0, 0, 0, 0])
            }
        }

        struct TransactionBackend(FakeApi);
        impl ClipboardBackend for TransactionBackend {
            fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
                publish(&mut self.0, text, false)
            }

            fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
                clear(&mut self.0, sequence, text)
            }
        }

        #[test]
        fn captured_lease_survives_windows_close_synthesis_and_owner_destruction() {
            let state = Arc::new(Mutex::new(FakeState::default()));
            let mut backend = TransactionBackend(FakeApi(state.clone()));
            let attempt = backend.set_private(FAKE_TEXT).unwrap();
            assert!(attempt.failure.is_none());
            {
                let state = state.lock().unwrap();
                assert_eq!(
                    attempt.sequence, state.sequence,
                    "the sequence captured under OpenClipboard must still identify this publication after close"
                );
                assert_eq!(state.text(), FAKE_TEXT);
                assert!(!state.owner_alive);
            }
            assert!(backend.clear_matching(attempt.sequence, FAKE_TEXT).unwrap());
            assert_eq!(state.lock().unwrap().text(), "");
        }

        #[test]
        fn empty_and_unicode_values_keep_exact_unicode_and_stable_cleanup_ownership() {
            for text in ["", "\u{03bb}\u{1f512}", "e\u{301}\r\nfake"] {
                let state = Arc::new(Mutex::new(FakeState::default()));
                let mut backend = TransactionBackend(FakeApi(state.clone()));
                let attempt = backend.set_private(text).unwrap();
                assert!(attempt.failure.is_none());
                {
                    let state = state.lock().unwrap();
                    assert_eq!(state.text(), text);
                    assert_eq!(state.sequence, attempt.sequence);
                    assert_eq!(state.synthesized, 0);
                    assert_eq!(state.formats.len(), 7);
                    if text.is_empty() {
                        assert_eq!(state.formats[&UNICODE_TEXT], [0, 0]);
                        assert_eq!(state.formats[&ANSI_TEXT], [0]);
                        assert_eq!(state.formats[&OEM_TEXT], [0]);
                    }
                }
                assert!(backend.clear_matching(attempt.sequence, text).unwrap());
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
            let error = publish(&mut api, FAKE_TEXT, true).err().unwrap();
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
            let attempt = backend.set_private(FAKE_TEXT).unwrap();
            assert_eq!(
                attempt.sequence, 111,
                "must capture AKM's publication, not the user's"
            );
            assert!(attempt.failure.is_none());
            assert_eq!(state.lock().unwrap().blocked_copies, 1);
            assert_eq!(state.lock().unwrap().user_sequence, Some(112));
            assert!(!backend.clear_matching(attempt.sequence, FAKE_TEXT).unwrap());
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
                let error = match backend.set_private(FAKE_TEXT) {
                    Err(error) => error,
                    Ok(attempt) => attempt.failure.expect("a failed write must not succeed"),
                };
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
                match backend.set_private(FAKE_TEXT) {
                    Err(error) => assert_eq!(error.kind, "clipboard"),
                    Ok(attempt) => assert!(attempt.failure.is_some()),
                }
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
                        backend.set_private(FAKE_TEXT).is_err(),
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
                let attempt = backend.set_private(FAKE_TEXT).unwrap();
                assert!(attempt.failure.is_some());
                assert_eq!(
                    attempt.sequence,
                    if fault == Fault::Sequence { 0 } else { 111 }
                );
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
            let attempt = backend.set_private("\u{03bb}\u{1f512}").unwrap();
            assert!(attempt.failure.is_none());
            assert_eq!(attempt.sequence, 111);
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
            assert!(!backend
                .clear_matching(attempt.sequence, "different fake text")
                .unwrap());
            assert!(backend
                .clear_matching(attempt.sequence, "\u{03bb}\u{1f512}")
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
                let error = backend.set_private(FAKE_TEXT).err().unwrap();
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
                    assert_eq!((state.open_calls, state.opens, state.closes), (2, 2, 2));
                }
            }
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
        fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
            let mut data = self.0.lock().unwrap();
            data.sequence += 1;
            data.text = text.into();
            Ok(CopyAttempt {
                sequence: data.sequence,
                failure: None,
            })
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
        fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
            let mut attempt = self.fake.set_private(text)?;
            attempt.failure = Some(CommandError::new("clipboard", "injected exclusion failure"));
            Ok(attempt)
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            if self.busy_once {
                self.busy_once = false;
                return Err(CommandError::new("clipboard", "injected busy clipboard"));
            }
            self.fake.clear_matching(sequence, text)
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
