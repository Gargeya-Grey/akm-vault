//! The fake Win32 clipboard API (unit 1h, design D8). The real publisher and the real cleanup
//! run over it, in the session tests and in `tests/ipc_dispatch.rs`.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use zeroize::Zeroize;

use crate::{
    clipboard::{
        windows::{
            clear, publish, ClipboardApi, TextFormats, TextMatch, PRIVATE_FORMATS, UNICODE_TEXT,
        },
        ClipboardBackend, Publication,
    },
    error::CommandError,
};

pub(crate) const FAKE_TEXT: &str = "AKM fake transaction secret";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fault {
    Register(usize),
    Allocate(usize),
    Owner,
    Open(usize),
    Empty(usize),
    /// `EmptyClipboard` fails but still changes the sequence; the formats stay.
    EmptyChanged(usize),
    Set(usize),
    TextRead,
    ZeroRead(u32),
    Sequence,
    /// The nth sample of the sequence reads zero.
    SequenceCall(usize),
    Encode,
    /// The nth `CloseClipboard` fails. The fake clipboard is released all the same.
    CloseFail(usize),
    /// The nth `DestroyWindow` fails and leaves the owner window.
    DestroyFail(usize),
}

pub(crate) struct FakeState {
    pub(crate) open: bool,
    pub(crate) open_with_owner: bool,
    pub(crate) owner_alive: bool,
    pub(crate) changed: bool,
    pub(crate) synthesized: usize,
    pub(crate) formats: BTreeMap<u32, Vec<u8>>,
    pub(crate) sequence: u32,
    pub(crate) race_at_capture: bool,
    pub(crate) pending_user_copy: Option<Vec<u8>>,
    pub(crate) blocked_copies: usize,
    pub(crate) user_sequence: Option<u32>,
    pub(crate) clears: usize,
    pub(crate) registered_names: Vec<String>,
    pub(crate) open_calls: usize,
    pub(crate) opens: usize,
    pub(crate) closes: usize,
    pub(crate) destroyed_owners: usize,
    pub(crate) leaked_owners: usize,
    pub(crate) sequence_calls: usize,
    pub(crate) close_calls: usize,
    pub(crate) destroy_calls: usize,
    /// The next `read_text` lets a delayed-rendering owner replace the content with a newer
    /// copy of the same text, inside the open interval, after the read answered.
    pub(crate) render_on_read: bool,
    pub(crate) allocations: usize,
    pub(crate) client_frees: usize,
    pub(crate) transfers: usize,
    pub(crate) system_frees: usize,
    pub(crate) empty_calls: usize,
    pub(crate) set_calls: usize,
    pub(crate) faults: Vec<Fault>,
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
            leaked_owners: 0,
            sequence_calls: 0,
            close_calls: 0,
            destroy_calls: 0,
            render_on_read: false,
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

    pub(crate) fn user_copy(&mut self, text: Vec<u8>) {
        assert!(!self.open);
        self.system_frees += self.formats.len();
        self.formats.clear();
        self.formats.insert(UNICODE_TEXT, text);
        self.sequence += 1;
        self.user_sequence = Some(self.sequence);
    }

    pub(crate) fn text(&self) -> String {
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

pub(crate) struct FakeApi(pub(crate) Arc<Mutex<FakeState>>);
pub(crate) struct FakeOwner(Arc<Mutex<FakeState>>);
impl Drop for FakeOwner {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap();
        assert!(!state.open, "the owner must outlive CloseClipboard");
        state.owner_alive = false;
        state.destroyed_owners += 1;
    }
}

pub(crate) struct FakeMemory {
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

    fn destroy(&mut self, owner: FakeOwner) -> Result<(), CommandError> {
        let mut state = self.0.lock().unwrap();
        state.destroy_calls += 1;
        let call = state.destroy_calls;
        if state.fail(Fault::DestroyFail(call)) {
            state.leaked_owners += 1;
            std::mem::forget(owner);
            return Err(CommandError::new(
                "clipboard",
                "injected DestroyWindow failure",
            ));
        }
        drop(state);
        drop(owner);
        Ok(())
    }

    fn encode(&mut self, text: &str) -> Result<TextFormats, CommandError> {
        if self.0.lock().unwrap().fail(Fault::Encode) {
            return Err(CommandError::new("clipboard", "injected encoding failure"));
        }
        // A fixed locale, so no test depends on the machine's language.
        TextFormats::new(text, 0x0409)
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

    fn close(&mut self) -> Result<(), CommandError> {
        let mut state = self.0.lock().unwrap();
        assert!(state.open);
        state.close_calls += 1;
        let call = state.close_calls;
        let failed = state.fail(Fault::CloseFail(call));
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
        if failed {
            return Err(CommandError::new(
                "clipboard",
                "injected CloseClipboard failure",
            ));
        }
        Ok(())
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
        if state.fail(Fault::EmptyChanged(call)) {
            state.sequence += 1;
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
        // Not the publisher's sample before its first empty (an owned open with no
        // change yet): that sample only detects a failed empty's effect.
        state.sequence_calls += 1;
        let call = state.sequence_calls;
        if state.fail(Fault::SequenceCall(call)) {
            return 0;
        }
        let before_first_empty = state.open_with_owner && !state.changed;
        if !before_first_empty && state.fail(Fault::Sequence) {
            return 0;
        }
        state.sequence
    }

    fn read_text(&mut self, expected: &str) -> TextMatch {
        let mut state = self.0.lock().unwrap();
        assert!(state.open);
        if state.fail(Fault::TextRead) || !state.formats.contains_key(&UNICODE_TEXT) {
            return TextMatch::Unreadable;
        }
        let answer = if state.text() == expected {
            TextMatch::Same
        } else {
            TextMatch::Different
        };
        if state.render_on_read {
            state.render_on_read = false;
            let text = state.formats.get(&UNICODE_TEXT).unwrap().clone();
            state.system_frees += state.formats.len();
            state.formats.clear();
            state.formats.insert(UNICODE_TEXT, text);
            state.sequence += 1;
            state.user_sequence = Some(state.sequence);
        }
        answer
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

pub(crate) struct TransactionBackend(pub(crate) FakeApi);
impl ClipboardBackend for TransactionBackend {
    fn set_private(&mut self, text: &str) -> Publication {
        publish(&mut self.0, text, false)
    }

    fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
        clear(&mut self.0, sequence, text)
    }
}
