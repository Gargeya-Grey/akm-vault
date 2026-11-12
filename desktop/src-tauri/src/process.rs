//! A process that is created suspended, so that its start is one short act under the session
//! gate (`docs/design/commit-point.md`). The slow part, `CreateProcessW`, runs before the
//! commit and outside the gate. The commit resumes the first thread. A refusal terminates the
//! process before it ran one instruction of its program.

use std::{
    ffi::OsStr,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
};

use windows_sys::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    Globalization::{CompareStringOrdinal, CSTR_EQUAL, CSTR_LESS_THAN},
    System::Threading::{
        CreateProcessW, ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_NEW_CONSOLE,
        CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW,
    },
};
use zeroize::Zeroizing;

use crate::{error::CommandError, session::Held};

/// How long a revoke waits for a terminated process to end.
const TERMINATE_WAIT_MS: u32 = 5_000;

/// The exit code of a process that AKM terminated before it started.
const NOT_STARTED: u32 = 1;

/// The console of the child.
#[derive(Clone, Copy)]
pub enum Console {
    /// A new console window: the terminal that the user works in.
    New,
    /// No window, for a test.
    #[cfg(test)]
    Hidden,
}

impl Console {
    fn flags(self) -> u32 {
        match self {
            Self::New => CREATE_NEW_CONSOLE,
            #[cfg(test)]
            Self::Hidden => windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        }
    }
}

/// A UTF-16 environment block for `CreateProcessW`. It is allocated once at its final size, so
/// no reallocation leaves a copy of a value behind, and it is zeroed on drop.
pub struct EnvironmentBlock(Zeroizing<Vec<u16>>);

enum Value<'a> {
    Inherited(&'a OsStr),
    Selected(&'a str),
}

impl Value<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Inherited(value) => value.encode_wide().count(),
            Self::Selected(value) => value.encode_utf16().count(),
        }
    }
}

impl EnvironmentBlock {
    /// The inherited variables without each name that `removed` matches, and the selected
    /// variables. A selected variable replaces an inherited one of the same name, in any
    /// case. The block is sorted by name, ignoring case, as Windows expects.
    pub fn new<'a>(
        inherited: impl IntoIterator<Item = (&'a OsStr, &'a OsStr)>,
        selected: &'a [(String, String)],
        removed: impl Fn(&OsStr) -> bool,
    ) -> Self {
        let selected: Vec<(Vec<u16>, Value<'a>)> = selected
            .iter()
            .filter(|(name, _)| !removed(OsStr::new(name)))
            .map(|(name, value)| (name.encode_utf16().collect(), Value::Selected(value)))
            .collect();
        let mut entries: Vec<(Vec<u16>, Value<'a>)> = inherited
            .into_iter()
            .filter(|(name, _)| !removed(name))
            .map(|(name, value)| {
                (
                    name.encode_wide().collect::<Vec<u16>>(),
                    Value::Inherited(value),
                )
            })
            .filter(|(name, _)| !selected.iter().any(|(chosen, _)| same_name(name, chosen)))
            .collect();
        entries.extend(selected);
        entries.sort_by(|(left, _), (right, _)| ordinal_ignoring_case(left, right));

        let strings: usize = entries
            .iter()
            .map(|(name, value)| name.len() + 1 + value.len() + 1)
            .sum();
        // An empty block is two zeros. Otherwise each string ends in a zero, and one more ends
        // the block.
        let length = (strings + 1).max(2);
        let mut block = Zeroizing::new(Vec::with_capacity(length));
        for (name, value) in &entries {
            block.extend_from_slice(name);
            block.push(u16::from(b'='));
            match value {
                Value::Inherited(value) => block.extend(value.encode_wide()),
                Value::Selected(value) => block.extend(value.encode_utf16()),
            }
            block.push(0);
        }
        block.resize(length, 0);
        debug_assert_eq!(block.len(), length);
        Self(block)
    }

    /// The `NAME=value` strings of the block, for a test.
    #[cfg(test)]
    pub fn strings(&self) -> Vec<String> {
        self.0
            .split(|unit| *unit == 0)
            .filter(|string| !string.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    #[cfg(test)]
    pub fn units(&self) -> &[u16] {
        &self.0
    }
}

fn same_name(left: &[u16], right: &[u16]) -> bool {
    compare(left, right) == CSTR_EQUAL
}

fn ordinal_ignoring_case(left: &[u16], right: &[u16]) -> std::cmp::Ordering {
    match compare(left, right) {
        CSTR_LESS_THAN => std::cmp::Ordering::Less,
        CSTR_EQUAL => std::cmp::Ordering::Equal,
        _ => std::cmp::Ordering::Greater,
    }
}

fn compare(left: &[u16], right: &[u16]) -> i32 {
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return 0;
    };
    // SAFETY: both pointers are valid for the lengths that are passed.
    unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) }
}

/// What the child is.
pub struct Spec<'a> {
    /// An absolute path. It is also the first word of the command line.
    pub program: &'a Path,
    /// The rest of the command line. A launch passes `/d` and nothing else.
    pub arguments: &'a str,
    pub folder: &'a Path,
    pub environment: &'a EnvironmentBlock,
    pub console: Console,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Suspended,
    Resumed,
    Ended,
}

/// A process whose first thread has not run yet. Dropped while still suspended, it is
/// terminated: a launch that ends early never leaves a process with the values behind.
pub struct Suspended {
    process: OwnedHandle,
    thread: OwnedHandle,
    state: State,
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain([0]).collect()
}

impl Suspended {
    /// Creates the process suspended. It inherits no handle of AKM. The environment block is
    /// the only environment it gets.
    pub fn create(spec: &Spec<'_>) -> Result<Self, CommandError> {
        let failed = || {
            CommandError::new(
                "launch",
                format!(
                    "The terminal could not be started. ({})",
                    std::io::Error::last_os_error()
                ),
            )
        };
        let program = wide(spec.program.as_os_str());
        let mut line = OsStr::new("\"").to_os_string();
        line.push(spec.program.as_os_str());
        line.push("\" ");
        line.push(spec.arguments);
        let mut line = wide(&line);
        let folder = wide(spec.folder.as_os_str());
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..STARTUPINFOW::default()
        };
        let mut created = PROCESS_INFORMATION::default();
        // SAFETY: every string is zero-terminated and lives across the call, the command line
        // is writable, the environment block is a Unicode block that ends in two zeros, and
        // `created` is written by the call.
        let ok = unsafe {
            CreateProcessW(
                program.as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | spec.console.flags(),
                spec.environment.0.as_ptr().cast(),
                folder.as_ptr(),
                &startup,
                &mut created,
            )
        };
        if ok == 0 {
            return Err(failed());
        }
        // SAFETY: the call succeeded, so both handles are open and this process owns them.
        let (process, thread) = unsafe {
            (
                OwnedHandle::from_raw_handle(created.hProcess as _),
                OwnedHandle::from_raw_handle(created.hThread as _),
            )
        };
        Ok(Self {
            process,
            thread,
            state: State::Suspended,
        })
    }

    /// Starts the first thread. Only an act under the session gate can call it.
    pub fn resume(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
        if self.state != State::Suspended {
            return Err(CommandError::internal(
                "The terminal was not waiting to start.",
            ));
        }
        // SAFETY: the handle is open while `self` lives.
        let previous = unsafe { ResumeThread(self.thread.as_raw_handle() as HANDLE) };
        match previous {
            // 1: it was suspended once and runs now. 0: it was already running.
            0 | 1 => {
                self.state = State::Resumed;
                Ok(())
            }
            _ => Err(CommandError::new(
                "launch",
                "The terminal could not be started.",
            )),
        }
    }

    /// Ends a process that has not started, and waits a bounded time for it to end. An error is
    /// a fixed text for an application warning.
    pub fn terminate(&mut self) -> Result<(), String> {
        if self.state != State::Suspended {
            return Ok(());
        }
        self.state = State::Ended;
        let handle = self.process.as_raw_handle() as HANDLE;
        // SAFETY: the handle is open while `self` lives.
        let ended = unsafe {
            TerminateProcess(handle, NOT_STARTED) != 0
                && WaitForSingleObject(handle, TERMINATE_WAIT_MS) == WAIT_OBJECT_0
        };
        if ended {
            Ok(())
        } else {
            Err("AKM prepared a terminal, did not start it, and could not confirm that Windows ended it. Close any empty terminal window, or end cmd.exe in Task Manager.".into())
        }
    }

    /// Waits for the process to end and gives its exit code, for a test.
    #[cfg(test)]
    pub fn wait(&self, milliseconds: u32) -> Option<u32> {
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;
        let handle = self.process.as_raw_handle() as HANDLE;
        let mut code = 0u32;
        // SAFETY: the handle is open while `self` lives, and `code` is written by the call.
        unsafe {
            (WaitForSingleObject(handle, milliseconds) == WAIT_OBJECT_0
                && GetExitCodeProcess(handle, &mut code) != 0)
                .then_some(code)
        }
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn block(inherited: &[(&str, &str)], selected: &[(&str, &str)]) -> EnvironmentBlock {
        let inherited: Vec<(OsString, OsString)> = inherited
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .collect();
        let selected: Vec<(String, String)> = selected
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        EnvironmentBlock::new(
            inherited
                .iter()
                .map(|(n, v)| (n.as_os_str(), v.as_os_str())),
            &selected,
            |name| {
                name.to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with("AKM_")
            },
        )
    }

    #[test]
    fn the_block_holds_the_selected_variables_and_no_removed_name_and_ends_in_two_zeros() {
        let block = block(
            &[
                ("Path", "C:\\Windows"),
                ("AKM_PASSWORD", "fake-inherited"),
                ("token", "fake-inherited-token"),
                ("=C:", "C:\\work"),
            ],
            &[
                ("TOKEN", "fake-token-value"),
                ("AKM_JOURNAL_ROOT", "fake-leak"),
                ("OPENAI_API_KEY", "fake-openai-value"),
            ],
        );
        assert_eq!(
            block.strings(),
            [
                "=C:=C:\\work",
                "OPENAI_API_KEY=fake-openai-value",
                "Path=C:\\Windows",
                "TOKEN=fake-token-value",
            ],
            "sorted ignoring case; a selected name replaces the inherited one in any case"
        );
        let units = block.units();
        assert_eq!(&units[units.len() - 2..], [0, 0]);
        assert_ne!(units[units.len() - 3], 0);
    }

    /// The real Win32 calls, with no window. A suspended child runs nothing. Resumed, it writes
    /// its marker. Terminated, it never does, and it ends with the code of a process that did
    /// not start.
    #[test]
    fn a_resumed_child_writes_its_marker_and_a_terminated_child_never_does() {
        let folder = tempfile::tempdir().unwrap();
        let shell = crate::launch::system_command_prompt().unwrap();
        let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let environment = EnvironmentBlock::new(
            inherited
                .iter()
                .map(|(n, v)| (n.as_os_str(), v.as_os_str())),
            &[],
            |_| false,
        );
        for resume in [true, false] {
            let marker = folder.path().join(if resume {
                "resumed.txt"
            } else {
                "terminated.txt"
            });
            let arguments = format!("/d /c echo started>\"{}\"", marker.display());
            let mut child = Suspended::create(&Spec {
                program: &shell,
                arguments: &arguments,
                folder: folder.path(),
                environment: &environment,
                console: Console::Hidden,
            })
            .unwrap();
            assert_eq!(child.wait(500), None, "a suspended child ended");
            assert!(!marker.exists(), "a suspended child ran");
            if resume {
                child.resume(&Held::for_test()).unwrap();
                assert_eq!(child.wait(30_000), Some(0));
                assert!(marker.exists(), "the resumed child did not run");
                assert!(child.terminate().is_ok(), "a started child is not ended");
            } else {
                child.terminate().unwrap();
                assert_eq!(child.wait(0), Some(NOT_STARTED));
                assert!(
                    child.resume(&Held::for_test()).is_err(),
                    "an ended child resumed"
                );
                std::thread::sleep(std::time::Duration::from_millis(500));
                assert!(!marker.exists(), "the terminated child ran");
            }
        }
    }

    /// A child dropped while suspended is terminated: an early return never leaves one behind.
    #[test]
    fn a_suspended_child_that_is_dropped_is_terminated() {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetExitCodeProcess};
        let folder = tempfile::tempdir().unwrap();
        let shell = crate::launch::system_command_prompt().unwrap();
        let environment = EnvironmentBlock::new(std::iter::empty(), &[], |_| false);
        let child = Suspended::create(&Spec {
            program: &shell,
            arguments: "/d /c exit 0",
            folder: folder.path(),
            environment: &environment,
            console: Console::Hidden,
        })
        .unwrap();
        let mut kept: HANDLE = std::ptr::null_mut();
        // SAFETY: both handles are open, and `kept` is written by the call.
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.process.as_raw_handle() as HANDLE,
                GetCurrentProcess(),
                &mut kept,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        assert_ne!(duplicated, 0);
        // SAFETY: the duplicate is open and this test owns it.
        let kept = unsafe { OwnedHandle::from_raw_handle(kept as _) };
        drop(child);
        let mut code = 0u32;
        // SAFETY: the handle is open, and `code` is written by the calls.
        unsafe {
            assert_eq!(
                WaitForSingleObject(kept.as_raw_handle() as HANDLE, 10_000),
                WAIT_OBJECT_0
            );
            assert_ne!(
                GetExitCodeProcess(kept.as_raw_handle() as HANDLE, &mut code),
                0
            );
        }
        assert_eq!(code, NOT_STARTED);
    }

    #[test]
    fn an_empty_block_is_two_zeros() {
        assert_eq!(block(&[], &[]).units(), [0, 0]);
        assert_eq!(block(&[("AKM_X", "1")], &[]).units(), [0, 0]);
    }
}
