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

#[cfg(test)]
thread_local! {
    static READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Counts the bytes that a bounded read takes from a file, so that a test can prove by a
/// count that a file past its bound is refused before it is read (CORE-R2-06).
pub(crate) fn count_read(bytes: u64) {
    #[cfg(test)]
    READ.with(|read| read.set(read.get().saturating_add(bytes)));
    let _ = bytes;
}

/// The bytes counted by [`count_read`] on this thread since the last call.
#[cfg(test)]
pub(crate) fn take_read_count() -> u64 {
    READ.with(|read| read.replace(0))
}

/// Reads the whole of an open file, or refuses it with `FileTooLarge` when it is longer than
/// `limit` bytes. The length comes from the handle before the read, and the read stops one
/// byte past the limit, so a file that grows during the read costs no more (CORE-R2-06).
pub(crate) fn read_bounded(file: &mut File, limit: u64) -> std::io::Result<Vec<u8>> {
    if file.metadata()?.len() > limit {
        return Err(too_large(limit));
    }
    read_limited(file, limit)
}

fn too_large(limit: u64) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::FileTooLarge,
        format!("the file is longer than the {limit} bytes that AKM reads"),
    )
}

/// Reads a stream to its end, or refuses it with `FileTooLarge` once it gives more than
/// `limit` bytes. It reads at most one byte past the limit, so a file that grows after its
/// length was checked costs no more (CORE-R2-06).
pub(crate) fn read_limited(reader: impl std::io::Read, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    count_read(bytes.len() as u64);
    if bytes.len() as u64 > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
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

/// The volume flag that says the file system keeps Windows permissions.
const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;

/// The pure part of the storage rule: a local disk that keeps Windows permissions.
pub(crate) fn storage_allows(
    drive_type: u32,
    volume_flags: u32,
) -> std::result::Result<(), &'static str> {
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_RAMDISK: u32 = 6;
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
        /// The call that creates the file with the descriptor fails, on a volume that keeps
        /// Windows permissions.
        CreateFails,
        /// The call fails, and the volume reports that it cannot keep Windows permissions.
        CreateFailsOnVolumeWithoutPermissions,
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
/// The save must not fail because a file system cannot keep permissions (FAT, exFAT). There
/// the file is created the plain way and the second value says why it is not private, so the
/// caller can warn. A failed private creation on a volume that keeps permissions, or on one
/// that AKM cannot read, fails: a plain file there would take the folder's permissions
/// (FV-21). A file that already exists is always an error.
pub(crate) fn create_new_private(path: &Path) -> std::io::Result<(File, Option<String>)> {
    use std::fs::OpenOptions;
    let plain = || OpenOptions::new().write(true).create_new(true).open(path);
    #[cfg(windows)]
    {
        #[cfg(test)]
        use testing::Mode;
        let created = match () {
            #[cfg(test)]
            () if matches!(
                testing::mode(),
                Mode::CreateFails | Mode::CreateFailsOnVolumeWithoutPermissions
            ) =>
            {
                Err(std::io::Error::other("test: the call failed"))
            }
            #[cfg(test)]
            () if testing::mode() == Mode::DescriptorIgnored => plain(),
            () => imp::create_private(path),
        };
        match created {
            Ok(file) => {
                let note = imp::why_not_private(&file, path);
                Ok((file, note))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(error),
            Err(error) => {
                let keeps = match () {
                    #[cfg(test)]
                    () if testing::mode() == Mode::CreateFailsOnVolumeWithoutPermissions => {
                        Some(false)
                    }
                    () => imp::volume_keeps_permissions(path),
                };
                if keeps != Some(false) {
                    return Err(std::io::Error::new(
                        error.kind(),
                        format!(
                            "{} could not be created private to your account: {error}",
                            path.display()
                        ),
                    ));
                }
                Ok((
                    plain()?,
                    Some(format!(
                        "the file system cannot keep Windows permissions ({error})"
                    )),
                ))
            }
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
pub(crate) use imp::{
    create_private_dir, file_id, link_count, open_dir, open_file, reject_links, remove_file_if,
};

#[cfg(not(windows))]
pub(crate) use other::{
    create_private_dir, file_id, link_count, open_dir, open_file, reject_links, remove_file_if,
};

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

    /// See the Windows `link_count`.
    pub(crate) fn link_count(file: &File) -> std::io::Result<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(u32::try_from(file.metadata()?.nlink()).unwrap_or(u32::MAX))
        }
        #[cfg(not(unix))]
        {
            let _ = file;
            Ok(1)
        }
    }

    pub(crate) fn reject_links(_: &Path) -> Result<()> {
        Err(unsupported())
    }

    /// See the Windows `remove_file_if`. Here the check and the delete are two steps, so a
    /// file that another writer puts at the path between them is deleted.
    pub(crate) fn remove_file_if(
        path: &Path,
        holds: impl FnOnce(&[u8]) -> bool,
    ) -> std::io::Result<bool> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() {
            return Ok(false);
        }
        let bytes = match read_bounded(&mut File::open(path)?, crate::vault::MAX_VAULT_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => return Ok(false),
            Err(error) => return Err(error),
        };
        if !holds(&bytes) {
            return Ok(false);
        }
        std::fs::remove_file(path)?;
        Ok(true)
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
        CreateDirectoryW, CreateFileW, FileDispositionInfo, GetDriveTypeW,
        GetFileInformationByHandle, GetVolumeInformationByHandleW, GetVolumePathNameW,
        SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, CREATE_NEW, DELETE,
        FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, OPEN_ALWAYS,
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

    /// Deletes the regular file at `path` only when its bytes pass `holds` (FR-01, core review
    /// 1, finding 1). One handle reads the bytes and deletes the file. It is opened without
    /// following a link, and it shares reading only, so no other writer can change, rename or
    /// delete the file between the check and the delete. `false` when nothing was deleted: no
    /// file, a link or a folder, or other bytes.
    pub(crate) fn remove_file_if(
        path: &Path,
        holds: impl FnOnce(&[u8]) -> bool,
    ) -> io::Result<bool> {
        use std::os::windows::fs::OpenOptionsExt;
        let mut file = match fs::OpenOptions::new()
            .access_mode(FILE_GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(false);
        }
        let bytes = match super::read_bounded(&mut file, crate::vault::MAX_VAULT_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::FileTooLarge => return Ok(false),
            Err(error) => return Err(error),
        };
        if !holds(&bytes) {
            return Ok(false);
        }
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: the handle is open with DELETE access, and the buffer is one
        // FILE_DISPOSITION_INFO that lives across the call.
        let marked = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as HANDLE,
                FileDispositionInfo,
                (&raw const disposition).cast::<c_void>(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        };
        if marked == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }

    /// The path for a Win32 call, NUL-terminated. A path of 248 units or more gets the
    /// verbatim form (`\\?\C:\…`, `\\?\UNC\server\…`), which has no `MAX_PATH` limit, as the
    /// standard library does for its own calls (FV-21). A shorter path stays as written.
    pub(super) fn wide(path: &Path) -> Result<Vec<u16>> {
        const LEGACY_MAX_PATH: usize = 248;
        let units = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().collect() };
        let mut wide = units(path);
        if wide.contains(&0) {
            return Err(refuse("the path contains a NUL character"));
        }
        if wide.len() >= LEGACY_MAX_PATH {
            let absolute = std::path::absolute(path)
                .map_err(|error| refuse(format!("{}: {error}", path.display())))?;
            wide = verbatim(units(&absolute));
        }
        wide.push(0);
        Ok(wide)
    }

    /// The verbatim form of an absolute path (`std::path::absolute` has made its separators
    /// `\` and resolved `.` and `..`, which a verbatim path does not do).
    fn verbatim(absolute: Vec<u16>) -> Vec<u16> {
        let starts = |prefix: &str| {
            absolute
                .iter()
                .copied()
                .take(prefix.len())
                .eq(prefix.encode_utf16())
        };
        let drive = absolute.get(1..3) == Some(&[u16::from(b':'), u16::from(b'\\')][..]);
        let (prefix, skip) = if starts(r"\\?\") || starts(r"\??\") {
            ("", 0)
        } else if starts(r"\\.\") {
            (r"\\?\", 4)
        } else if starts(r"\\") {
            (r"\\?\UNC\", 2)
        } else if drive {
            (r"\\?\", 0)
        } else {
            ("", 0)
        };
        prefix
            .encode_utf16()
            .chain(absolute[skip..].iter().copied())
            .collect()
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

    fn volume_flags(file: &File) -> Option<u32> {
        let mut flags = 0;
        // SAFETY: the handle is open, and `flags` lives across the call.
        let read = unsafe {
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
        };
        (read != 0).then_some(flags)
    }

    /// Whether the volume of the folder of `path` keeps Windows permissions. `None` when AKM
    /// cannot tell.
    pub(crate) fn volume_keeps_permissions(path: &Path) -> Option<bool> {
        use std::os::windows::fs::OpenOptionsExt;
        let folder = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let folder = fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(folder)
            .ok()?;
        volume_flags(&folder).map(|flags| flags & FILE_PERSISTENT_ACLS != 0)
    }

    fn check_storage(file: &File, path: &Path) -> Result<()> {
        let flags = volume_flags(file).ok_or_else(|| os_error("cannot read the volume"))?;
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

    /// `verify`, and the access list is protected: it takes no entry from its folder.
    #[cfg(test)]
    pub(crate) fn check_private(path: &Path) -> Result<()> {
        use windows_sys::Win32::Security::{GetSecurityDescriptorControl, SE_DACL_PROTECTED};
        let file = open_file(path, OpenMode::Read)?
            .ok_or_else(|| refuse(format!("{} does not exist", path.display())))?;
        verify(&file, path)?;
        let mut descriptor = null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(refuse(format!(
                "cannot read the permissions of {}",
                path.display()
            )));
        }
        let descriptor = Local(descriptor);
        let (mut control, mut revision) = (0u16, 0u32);
        if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0 {
            return Err(os_error("cannot read a security descriptor"));
        }
        if control & SE_DACL_PROTECTED == 0 {
            return Err(refuse(format!(
                "{} takes permissions from its folder",
                path.display()
            )));
        }
        Ok(())
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

    /// The number of names (hard links) of an open file (CORE-R2-05).
    pub(crate) fn link_count(file: &File) -> io::Result<u32> {
        Ok(information(file)?.nNumberOfLinks)
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
    use super::{read_limited, take_read_count};

    /// CORE-R2-06: the stream bound holds by itself, for a file that grows after its length
    /// was checked. At the limit the bytes are read; one byte past it, the read is refused
    /// after at most one byte more.
    #[test]
    fn a_bounded_read_refuses_a_stream_one_byte_past_its_limit() {
        assert_eq!(read_limited(&[7u8; 8][..], 8).unwrap(), [7u8; 8]);
        take_read_count();
        let error = read_limited(&[7u8; 4096][..], 8).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::FileTooLarge);
        assert_eq!(take_read_count(), 9, "one byte past the limit, no more");
    }

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

    /// FV-21: a path of 248 units or more gets the verbatim form; a shorter one stays as
    /// written.
    #[cfg(windows)]
    #[test]
    fn a_long_path_gets_the_verbatim_form_for_win32() {
        let text = |path: &str| {
            let wide = super::imp::wide(std::path::Path::new(path)).unwrap();
            String::from_utf16(&wide[..wide.len() - 1]).unwrap()
        };
        let long = "a".repeat(250);
        assert_eq!(text(r"C:\short\vault.kdbx"), r"C:\short\vault.kdbx");
        assert_eq!(text(&format!(r"C:\{long}")), format!(r"\\?\C:\{long}"));
        assert_eq!(text(&format!("C:/{long}/b")), format!(r"\\?\C:\{long}\b"));
        assert_eq!(text(&format!(r"C:\x\..\{long}")), format!(r"\\?\C:\{long}"));
        assert_eq!(
            text(&format!(r"\\server\share\{long}")),
            format!(r"\\?\UNC\server\share\{long}")
        );
        assert_eq!(text(&format!(r"\\?\C:\{long}")), format!(r"\\?\C:\{long}"));
        assert!(
            text(&long).starts_with(r"\\?\"),
            "a relative path is made absolute"
        );
        assert!(super::imp::wide(std::path::Path::new("a\0b")).is_err());
    }

    /// FV-21, the class: every Win32 call of this module that takes a path works past 260
    /// characters. A private folder, its storage check, a private file and a file ID.
    #[cfg(windows)]
    #[test]
    fn the_private_calls_take_paths_longer_than_260_characters() {
        use super::{check_private, create_private_dir, file_id, open_dir, open_file};
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("p".repeat(120)).join("q".repeat(120));
        std::fs::create_dir_all(&parent).unwrap();
        let folder = parent.join("journal");
        create_private_dir(&folder).unwrap();
        assert!(open_dir(&folder).unwrap().is_some());
        let first = folder.join("a".repeat(40));
        assert!(first.as_os_str().len() > 260);
        drop(
            open_file(&first, super::OpenMode::CreateNew)
                .unwrap()
                .unwrap(),
        );
        check_private(&first).unwrap();
        assert!(file_id(&first).is_some());
    }

    /// FR-01, core review 1, finding 1: the delete of a pruned backup. A file with the
    /// recorded bytes is deleted; a file with other bytes, a folder, a folder link and a
    /// missing file are not, and a link is never followed to its target.
    #[cfg(windows)]
    #[test]
    fn remove_file_if_deletes_only_a_regular_file_with_the_expected_bytes() {
        use super::remove_file_if;
        let dir = tempfile::tempdir().unwrap();
        let ours = dir.path().join("ours.kdbx");
        std::fs::write(&ours, b"recorded bytes").unwrap();
        assert!(remove_file_if(&ours, |bytes| bytes == b"recorded bytes").unwrap());
        assert!(!ours.exists());

        let other = dir.path().join("other.kdbx");
        std::fs::write(&other, b"other bytes").unwrap();
        assert!(!remove_file_if(&other, |bytes| bytes == b"recorded bytes").unwrap());
        assert_eq!(std::fs::read(&other).unwrap(), b"other bytes");

        let folder = dir.path().join("folder.kdbx");
        std::fs::create_dir(&folder).unwrap();
        assert!(!remove_file_if(&folder, |_| true).unwrap_or(false));
        assert!(folder.is_dir());

        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("inside.txt"), b"keep").unwrap();
        let junction = dir.path().join("junction.kdbx");
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        assert!(!remove_file_if(&junction, |_| true).unwrap_or(false));
        assert_eq!(std::fs::read(target.join("inside.txt")).unwrap(), b"keep");
        let _ = std::fs::remove_dir(&junction);

        assert!(!remove_file_if(&dir.path().join("missing.kdbx"), |_| true).unwrap());

        // The open refuses a folder and a folder junction by itself. A file link opens, so
        // only the reparse-point check keeps the link.
        let linked = dir.path().join("linked.txt");
        std::fs::write(&linked, b"keep").unwrap();
        let link = dir.path().join("link.kdbx");
        match std::os::windows::fs::symlink_file(&linked, &link) {
            Ok(()) => {
                assert!(!remove_file_if(&link, |_| true).unwrap());
                assert!(std::fs::symlink_metadata(&link).is_ok(), "the link stays");
                assert_eq!(std::fs::read(&linked).unwrap(), b"keep");
            }
            // Written past the test harness's capture, so the line shows when the test passes.
            Err(error) if error.raw_os_error() == Some(1314) => {
                use std::io::Write as _;
                let _ = writeln!(
                    std::io::stderr(),
                    "NOT RUN: remove_file_if file-link case: Windows error 1314, no privilege to make a file symbolic link"
                );
            }
            Err(error) => panic!("could not make the file link: {error}"),
        }
    }
}
