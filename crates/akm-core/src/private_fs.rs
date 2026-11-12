//! Private files for the journal folder (unit 1b, design D4).
//!
//! Every folder and file that the journal uses is created with a protected security
//! descriptor (owner and one allow ACE: the current user) and is checked again on every
//! open: no link (reparse point), owner is the current user or Administrators, and the
//! DACL has only allow ACEs for the current user. Nothing here repairs or falls back.

use std::fs::File;
use std::path::Path;

use crate::error::{Error, Result};

fn refuse(message: impl std::fmt::Display) -> Error {
    Error::Journal(message.to_string())
}

/// How a journal file is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMode {
    /// Existing file, read only. `None` when it does not exist.
    Read,
    /// Existing lock file. `None` when it does not exist (a read never creates one).
    LockExisting,
    /// Create if missing, append only.
    Append,
    /// Create if missing, for a lock.
    LockCreate,
    /// Create a new file, read and write. Fails if it exists.
    CreateNew,
}

/// The pure part of the storage rule: a local disk that keeps Windows permissions.
pub(crate) fn storage_allows(
    drive_type: u32,
    volume_flags: u32,
) -> std::result::Result<(), &'static str> {
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_RAMDISK: u32 = 6;
    const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
    if !matches!(drive_type, DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_RAMDISK) {
        return Err("the folder is not on a local disk");
    }
    if volume_flags & FILE_PERSISTENT_ACLS == 0 {
        return Err("the volume cannot keep Windows permissions (persistent ACLs are required)");
    }
    Ok(())
}

/// Test switch: pretend that the file system cannot keep private permissions (FR-05).
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Mode {
        Normal,
        /// The call that creates the file with the descriptor fails.
        CreateFails,
        /// The file is created, but the file system ignores the descriptor (FAT, exFAT).
        DescriptorIgnored,
    }

    thread_local! {
        static MODE: Cell<Mode> = const { Cell::new(Mode::Normal) };
    }

    pub(crate) fn set(mode: Mode) {
        MODE.with(|cell| cell.set(mode));
    }

    pub(crate) fn mode() -> Mode {
        MODE.with(Cell::get)
    }
}

/// Creates a new file beside a vault (a temporary vault, a backup) that only the current
/// user can use: the same protected descriptor as the journal files (FR-05, design D4).
///
/// The save must not fail because a file system cannot keep permissions (exFAT, a network
/// share). In that case the file is created the plain way and the second value says why it
/// is not private, so the caller can warn. A file that already exists is always an error.
pub(crate) fn create_new_private(path: &Path) -> std::io::Result<(File, Option<String>)> {
    use std::fs::OpenOptions;
    let plain = || OpenOptions::new().write(true).create_new(true).open(path);
    #[cfg(windows)]
    {
        let created = match () {
            #[cfg(test)]
            () if testing::mode() == testing::Mode::CreateFails => {
                Err(std::io::Error::other("test: the call failed"))
            }
            #[cfg(test)]
            () if testing::mode() == testing::Mode::DescriptorIgnored => plain(),
            () => imp::create_private(path),
        };
        match created {
            Ok(file) => {
                let note = imp::why_not_private(&file, path);
                Ok((file, note))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(error),
            Err(error) => Ok((
                plain()?,
                Some(format!(
                    "the file system refused private permissions ({error})"
                )),
            )),
        }
    }
    #[cfg(not(windows))]
    {
        Ok((plain()?, None))
    }
}

/// Test helper: the Windows permissions check of the journal files, run on any file.
#[cfg(all(windows, test))]
pub(crate) fn check_private(path: &Path) -> Result<()> {
    imp::check_private(path)
}

#[cfg(windows)]
pub(crate) use imp::{create_private_dir, file_id, move_new, open_dir, open_file, reject_links};

#[cfg(not(windows))]
pub(crate) use other::{create_private_dir, file_id, move_new, open_dir, open_file, reject_links};

#[cfg(not(windows))]
mod other {
    use super::*;

    fn unsupported() -> Error {
        refuse("the private journal folder requires Windows")
    }

    pub(crate) fn create_private_dir(_: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("requires Windows"))
    }

    pub(crate) fn open_dir(_: &Path) -> Result<Option<File>> {
        Err(unsupported())
    }

    pub(crate) fn open_file(_: &Path, _: OpenMode) -> Result<Option<File>> {
        Err(unsupported())
    }

    pub(crate) fn file_id(_: &Path) -> Option<String> {
        None
    }

    pub(crate) fn reject_links(_: &Path) -> Result<()> {
        Err(unsupported())
    }

    /// See the Windows `move_new`. A hard link fails when `to` exists, so the same rule holds.
    pub(crate) fn move_new(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::hard_link(from, to)?;
        std::fs::remove_file(from)
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::fs;
    use std::io;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        AclSizeInformation, EqualSid, GetAce, GetAclInformation, GetLengthSid, GetTokenInformation,
        TokenUser, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, GetDriveTypeW, GetFileInformationByHandle,
        GetVolumeInformationByHandleW, GetVolumePathNameW, MoveFileExW, BY_HANDLE_FILE_INFORMATION,
        CREATE_NEW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, MOVEFILE_WRITE_THROUGH, OPEN_ALWAYS,
        OPEN_EXISTING, READ_CONTROL,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use super::*;

    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

    struct Local(*mut c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    struct Token(HANDLE);
    impl Drop for Token {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    struct Principal {
        text: String,
        sid: Vec<u8>,
    }

    fn os_error(context: &str) -> Error {
        refuse(format!("{context}: {}", io::Error::last_os_error()))
    }

    /// Moves `from` to `to` only when no file is at `to` (core review 1, finding 8). The check
    /// and the move are one step of the file system, so a file that appears at `to` after AKM
    /// chose the name is never replaced: the move fails with `AlreadyExists`. This is
    /// `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`; `std::fs::rename` replaces.
    pub(crate) fn move_new(from: &Path, to: &Path) -> io::Result<()> {
        let encode = |path: &Path| -> io::Result<Vec<u16>> {
            wide(path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
        };
        let (from, to) = (encode(from)?, encode(to)?);
        // SAFETY: both buffers are NUL-terminated wide strings that live across the call.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn wide(path: &Path) -> Result<Vec<u16>> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if wide[..wide.len() - 1].contains(&0) {
            return Err(refuse("the path contains a NUL character"));
        }
        Ok(wide)
    }

    fn sid_text(sid: *mut c_void) -> Result<String> {
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return Err(os_error("cannot read a security identifier"));
        }
        let owned = Local(text.cast());
        let mut length = 0;
        unsafe {
            while *text.add(length) != 0 {
                length += 1;
            }
            let value = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
            drop(owned);
            Ok(value)
        }
    }

    fn principal() -> Result<&'static Principal> {
        static USER: OnceLock<std::result::Result<Principal, String>> = OnceLock::new();
        USER.get_or_init(|| unsafe {
            let mut token = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error().to_string());
            }
            let token = Token(token);
            let mut bytes = 0;
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut bytes);
            let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
            if GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            ) == 0
            {
                return Err(io::Error::last_os_error().to_string());
            }
            let sid = (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            let length = GetLengthSid(sid) as usize;
            let raw = std::slice::from_raw_parts(sid.cast::<u8>(), length).to_vec();
            let text = sid_text(sid).map_err(|error| error.to_string())?;
            Ok(Principal { text, sid: raw })
        })
        .as_ref()
        .map_err(|error| refuse(format!("cannot identify the current user: {error}")))
    }

    fn descriptor(directory: bool) -> Result<Local> {
        let user = &principal()?.text;
        let inherit = if directory { "OICI" } else { "" };
        let sddl: Vec<u16> = format!("O:{user}D:P(A;{inherit};FA;;;{user})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(os_error("cannot build the private permissions"));
        }
        Ok(Local(descriptor))
    }

    pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
        let wide = wide(path).map_err(io::Error::other)?;
        let descriptor = descriptor(true).map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn information(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }

    /// Link, owner and DACL checks on an open handle.
    fn verify(file: &File, path: &Path) -> Result<()> {
        let shown = path.display();
        let info = information(file).map_err(|error| refuse(format!("{shown}: {error}")))?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(refuse(format!(
                "{shown} is a link (reparse point); AKM does not follow links here"
            )));
        }
        let user = principal()?;
        let (mut owner, mut dacl, mut descriptor) = (null_mut(), null_mut::<ACL>(), null_mut());
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(refuse(format!(
                "cannot read the permissions of {shown}: {}",
                io::Error::from_raw_os_error(status as i32)
            )));
        }
        let _descriptor = Local(descriptor);
        let owner_is_user = !owner.is_null()
            && unsafe { EqualSid(owner, user.sid.as_ptr().cast_mut().cast()) } != 0;
        if !owner_is_user && (owner.is_null() || sid_text(owner)? != ADMINISTRATORS_SID) {
            return Err(refuse(format!(
                "permissions are not private: {shown} belongs to another account"
            )));
        }
        if dacl.is_null() {
            return Err(refuse(format!(
                "permissions are not private: {shown} has no access list, so every account can use it"
            )));
        }
        let mut sizes: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            GetAclInformation(
                dacl,
                std::ptr::from_mut(&mut sizes).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        {
            return Err(os_error("cannot read an access list"));
        }
        for index in 0..sizes.AceCount {
            let mut ace = null_mut();
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
                return Err(os_error("cannot read an access list entry"));
            }
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            if header.AceType != ACCESS_ALLOWED_ACE_TYPE {
                return Err(refuse(format!(
                    "permissions are not private: {shown} has a deny or special entry (type {})",
                    header.AceType
                )));
            }
            let sid = unsafe { (&raw const (*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart) }
                .cast_mut()
                .cast::<c_void>();
            if unsafe { EqualSid(sid, user.sid.as_ptr().cast_mut().cast()) } == 0 {
                return Err(refuse(format!(
                    "permissions are not private: {shown} also grants access to {}",
                    sid_text(sid)?
                )));
            }
        }
        Ok(())
    }

    fn check_storage(file: &File, path: &Path) -> Result<()> {
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            )
        } == 0
        {
            return Err(os_error("cannot read the volume"));
        }
        let wide_path = wide(path)?;
        let mut root = vec![0u16; 32768];
        let drive_type = unsafe {
            if GetVolumePathNameW(wide_path.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
                return Err(os_error("cannot find the volume"));
            }
            GetDriveTypeW(root.as_ptr())
        };
        storage_allows(drive_type, flags)
            .map_err(|reason| refuse(format!("{}: {reason}", path.display())))
    }

    fn open_raw(
        path: &Path,
        access: u32,
        share: u32,
        disposition: u32,
        flags: u32,
        create_file: bool,
    ) -> io::Result<File> {
        let wide = wide(path).map_err(io::Error::other)?;
        let descriptor = if create_file {
            Some(descriptor(false).map_err(io::Error::other)?)
        } else {
            None
        };
        let attributes = descriptor.as_ref().map(|descriptor| SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        });
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                share,
                attributes
                    .as_ref()
                    .map_or(std::ptr::null(), |attributes| attributes as *const _),
                disposition,
                flags,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    fn open_failed(path: &Path, error: io::Error) -> Error {
        match error.kind() {
            io::ErrorKind::PermissionDenied => {
                refuse(format!("access to {} was denied", path.display()))
            }
            _ => refuse(format!("cannot open {}: {error}", path.display())),
        }
    }

    /// Creates a new file with the private descriptor. Fails if the file exists.
    pub(crate) fn create_private(path: &Path) -> io::Result<File> {
        open_raw(
            path,
            FILE_GENERIC_WRITE | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            true,
        )
    }

    /// Why a file just created is not private, or `None` when it is. A file system that
    /// ignores the descriptor (FAT, exFAT) shows here.
    pub(crate) fn why_not_private(file: &File, path: &Path) -> Option<String> {
        match verify(file, path) {
            Ok(()) => None,
            Err(Error::Journal(reason)) => Some(reason),
            Err(other) => Some(other.to_string()),
        }
    }

    #[cfg(test)]
    pub(crate) fn check_private(path: &Path) -> Result<()> {
        let file = open_file(path, OpenMode::Read)?
            .ok_or_else(|| refuse(format!("{} does not exist", path.display())))?;
        verify(&file, path)
    }

    /// Opens an existing folder for use. `None` when it does not exist.
    pub(crate) fn open_dir(path: &Path) -> Result<Option<File>> {
        let opened = open_raw(
            path,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            false,
        );
        let file = match opened {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(open_failed(path, error)),
        };
        verify(&file, path)?;
        check_storage(&file, path)?;
        Ok(Some(file))
    }

    pub(crate) fn open_file(path: &Path, mode: OpenMode) -> Result<Option<File>> {
        let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
        let append_only = (FILE_GENERIC_WRITE & !FILE_WRITE_DATA) | FILE_APPEND_DATA;
        let (access, disposition, create) = match mode {
            OpenMode::Read => (FILE_GENERIC_READ, OPEN_EXISTING, false),
            OpenMode::LockExisting => {
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE, OPEN_EXISTING, false)
            }
            OpenMode::Append => (append_only, OPEN_ALWAYS, true),
            OpenMode::LockCreate => (FILE_GENERIC_READ | FILE_GENERIC_WRITE, OPEN_ALWAYS, true),
            OpenMode::CreateNew => (FILE_GENERIC_READ | FILE_GENERIC_WRITE, CREATE_NEW, true),
        };
        let opened = open_raw(
            path,
            access | READ_CONTROL,
            share,
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            create,
        );
        let file = match opened {
            Ok(file) => file,
            Err(error) if !create && error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(open_failed(path, error)),
        };
        verify(&file, path)?;
        Ok(Some(file))
    }

    /// Volume serial number and file index of an existing file, if the file system has them.
    pub(crate) fn file_id(path: &Path) -> Option<String> {
        let file = open_raw(
            path,
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            false,
        )
        .ok()?;
        let info = information(&file).ok()?;
        if info.nFileIndexHigh == 0 && info.nFileIndexLow == 0 {
            return None;
        }
        Some(format!(
            "{:08x}:{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }

    /// No part of an existing path may be a link (junction, symbolic link, mount point).
    pub(crate) fn reject_links(path: &Path) -> Result<()> {
        for part in path.ancestors() {
            if part.as_os_str().is_empty() {
                continue;
            }
            if let Ok(metadata) = fs::symlink_metadata(part) {
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(refuse(format!(
                        "{} is a link (reparse point); AKM does not follow links here",
                        part.display()
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::storage_allows;

    #[test]
    fn storage_rule_accepts_only_local_disks_that_keep_permissions() {
        const PERSISTENT: u32 = 0x0000_0008;
        for (drive, flags, ok) in [
            (3, PERSISTENT | 0x1, true),
            (2, PERSISTENT, true),
            (6, PERSISTENT, true),
            (4, PERSISTENT, false),
            (5, PERSISTENT, false),
            (1, PERSISTENT, false),
            (0, PERSISTENT, false),
            (3, 0x1 | 0x2, false),
            (3, 0, false),
        ] {
            assert_eq!(
                storage_allows(drive, flags).is_ok(),
                ok,
                "drive {drive}, flags {flags:#x}"
            );
        }
        assert!(storage_allows(4, PERSISTENT)
            .unwrap_err()
            .contains("local disk"));
        assert!(storage_allows(3, 0)
            .unwrap_err()
            .contains("persistent ACLs"));
    }
}
