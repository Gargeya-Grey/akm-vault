use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use crate::error::CommandError;

fn invalid(message: &str) -> CommandError {
    CommandError::new("invalidPath", message)
}
fn io(error: std::io::Error) -> CommandError {
    CommandError::new("io", error.to_string())
}

fn check_windows_path(raw: &str) -> Result<(), CommandError> {
    let bytes = raw.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || raw[2..].contains(':')
        || raw.contains('/')
        || raw.contains('\0')
    {
        return Err(invalid("Choose an absolute local drive path. Network, UNC, device and alternate-stream paths are not allowed."));
    }
    for part in raw[3..].split('\\') {
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with([' ', '.'])
            || part
                .chars()
                .any(|ch| ch < ' ' || matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(invalid(
                "Choose a normal local file path without aliases or parent traversal.",
            ));
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches(' ')
            .to_ascii_lowercase();
        if matches!(
            stem.as_str(),
            "con"
                | "conin$"
                | "conout$"
                | "prn"
                | "aux"
                | "nul"
                | "startup"
                | "windows"
                | "system32"
                | "syswow64"
        ) || stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|number| {
                matches!(
                    number,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        {
            return Err(invalid(
                "System, Startup and device paths are not valid export destinations.",
            ));
        }
    }
    if !raw.to_ascii_lowercase().ends_with(".env") {
        return Err(invalid(
            "Plaintext exports must be named .env or end in .env.",
        ));
    }
    Ok(())
}

fn identity(path: &Path) -> String {
    let resolved = fs::canonicalize(path)
        .or_else(|error| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => fs::canonicalize(parent).map(|parent| parent.join(name)),
            _ => Err(error),
        })
        .unwrap_or_else(|_| path.to_path_buf());
    resolved
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

pub fn validate_destination(
    path: &Path,
    protected: &[PathBuf],
    protected_dirs: &[PathBuf],
) -> Result<PathBuf, CommandError> {
    check_windows_path(
        path.to_str()
            .ok_or_else(|| invalid("The export path must be valid Unicode."))?,
    )?;
    #[cfg(windows)]
    windows::check_local_drive(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Choose a local export folder."))?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid("Choose an export filename."))?;
    let resolved = fs::canonicalize(parent).map_err(io)?.join(name);
    let resolved = resolved
        .to_str()
        .ok_or_else(|| invalid("The export path must be valid Unicode."))?;
    let resolved = resolved.strip_prefix(r"\\?\").unwrap_or(resolved);
    check_windows_path(resolved)?;
    let path = Path::new(resolved);
    let target = identity(path);
    for source in protected {
        let source = identity(source);
        if target == source || target.starts_with(&(source + ".")) {
            return Err(invalid(
                "Exports cannot replace the vault, its backups, audit log, locks or settings.",
            ));
        }
    }
    for directory in protected_dirs {
        let directory = identity(directory);
        if target == directory || target.starts_with(&(directory + "\\")) {
            return Err(invalid(
                "Exports cannot be saved in an application configuration or protected directory.",
            ));
        }
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io(error)),
    };
    if let Some(metadata) = metadata {
        if !metadata.is_file() || is_reparse(&metadata) {
            return Err(invalid(
                "The destination must be a regular file, not a link or directory.",
            ));
        }
        let mut signature = [0u8; 4];
        match File::open(path).map_err(io)?.read_exact(&mut signature) {
            Ok(()) if signature == [0x03, 0xd9, 0xa2, 0x9a] => {
                return Err(invalid(
                    "Exports cannot replace a KeePass vault, even when it has an .env filename.",
                ));
            }
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {}
            Err(error) => return Err(io(error)),
        }
    }
    Ok(PathBuf::from(resolved))
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    metadata.is_symlink()
}

/// The temporary files whose deletion was requested and not confirmed. Such a file may still
/// hold the plaintext keys. The caller of `prepare_private` owns the log and reads it after the
/// export ended: a drop cannot return an error, so the log is how a failed cleanup is reported.
#[derive(Clone, Default)]
pub struct Remnants(Arc<Mutex<Vec<PathBuf>>>);

impl Remnants {
    pub(crate) fn record(&self, path: PathBuf) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path);
    }

    /// The text that names each possible remnant, or `None` when every cleanup was confirmed.
    /// It holds a path and no value.
    pub fn warning(&self) -> Option<String> {
        let paths = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if paths.is_empty() {
            return None;
        }
        let list = paths
            .iter()
            .map(|path| format!("\"{}\"", path.display()))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "AKM could not confirm that it deleted the temporary export file {list}. It may still hold the plaintext keys. Delete it yourself."
        ))
    }
}

/// Asks the system to delete the file behind a handle. `true` means that the request was
/// accepted. The file goes when the last handle closes.
type Discard = fn(&File) -> bool;

#[cfg(windows)]
fn system_discard(file: &File) -> bool {
    windows::discard(file)
}

#[cfg(not(windows))]
fn system_discard(_: &File) -> bool {
    false
}

struct TempFile {
    file: File,
    path: PathBuf,
    published: bool,
    discard: Discard,
    remnants: Remnants,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.published && !(self.discard)(&self.file) {
            self.remnants.record(self.path.clone());
        }
    }
}

pub struct PreparedExport {
    destination: PathBuf,
    temp: TempFile,
    #[cfg(windows)]
    _parents: Vec<File>,
}

impl PreparedExport {
    pub fn publish(self) -> Result<PathBuf, CommandError> {
        self.publish_with(&mut SystemExportIo)
    }

    fn publish_with(mut self, export_io: &mut impl ExportIo) -> Result<PathBuf, CommandError> {
        export_io.publish(&self.temp.file, &self.destination)?;
        self.temp.published = true;
        Ok(self.destination.clone())
    }
}

/// Writes the plaintext to a private temporary file beside `path`. Every temporary file whose
/// deletion cannot be confirmed is recorded in `remnants`, whatever the way the export ends.
pub fn prepare_private(
    path: &Path,
    contents: &str,
    protected: &[PathBuf],
    protected_dirs: &[PathBuf],
    remnants: &Remnants,
) -> Result<PreparedExport, CommandError> {
    prepare_private_with(
        path,
        contents,
        protected,
        protected_dirs,
        remnants,
        &mut SystemExportIo,
    )
}

fn prepare_private_with(
    path: &Path,
    contents: &str,
    protected: &[PathBuf],
    protected_dirs: &[PathBuf],
    remnants: &Remnants,
    export_io: &mut impl ExportIo,
) -> Result<PreparedExport, CommandError> {
    let destination = validate_destination(path, protected, protected_dirs)?;
    #[cfg(windows)]
    let parents = windows::pin_local_parents(&destination)?;
    let temp = prepare_temp(&destination, contents, remnants, export_io)?;
    validate_destination(&destination, protected, protected_dirs)?;
    Ok(PreparedExport {
        destination,
        temp,
        #[cfg(windows)]
        _parents: parents,
    })
}

trait ExportIo {
    fn create_private(&mut self, path: &Path) -> Result<File, CommandError> {
        create_private(path)
    }

    fn write(&mut self, file: &mut File, contents: &str) -> Result<(), CommandError> {
        file.write_all(contents.as_bytes()).map_err(io)
    }

    fn flush(&mut self, file: &mut File) -> Result<(), CommandError> {
        file.flush().map_err(io)
    }

    fn sync(&mut self, file: &File) -> Result<(), CommandError> {
        file.sync_all().map_err(io)
    }

    fn publish(&mut self, file: &File, destination: &Path) -> Result<(), CommandError> {
        replace(file, destination)
    }

    /// How a temporary file that must go is deleted. A test replaces it to fail the request.
    fn discard(&self) -> Discard {
        system_discard
    }
}

struct SystemExportIo;
impl ExportIo for SystemExportIo {}

fn prepare_temp(
    path: &Path,
    contents: &str,
    remnants: &Remnants,
    export_io: &mut impl ExportIo,
) -> Result<TempFile, CommandError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Choose an export folder."))?;
    let temp_path = parent.join(format!(".akm-export-{}.tmp", uuid::Uuid::new_v4()));
    let mut temp = TempFile {
        file: export_io.create_private(&temp_path)?,
        path: temp_path,
        published: false,
        discard: export_io.discard(),
        remnants: remnants.clone(),
    };
    #[cfg(windows)]
    windows::verify_private_acl(&temp.file)?;
    export_io.write(&mut temp.file, contents)?;
    export_io.flush(&mut temp.file)?;
    export_io.sync(&temp.file)?;
    Ok(temp)
}

#[cfg(windows)]
fn create_private(path: &Path) -> Result<File, CommandError> {
    windows::create_private(path)
}

#[cfg(not(windows))]
fn create_private(_path: &Path) -> Result<File, CommandError> {
    Err(invalid("Private file export requires Windows."))
}

#[cfg(windows)]
fn replace(file: &File, target: &Path) -> Result<(), CommandError> {
    windows::replace(file, target)
}

#[cfg(not(windows))]
fn replace(_file: &File, _target: &Path) -> Result<(), CommandError> {
    Err(invalid("Private file export requires Windows."))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        ffi::c_void,
        mem::{offset_of, size_of},
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle},
        },
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
        Security::{
            Authorization::{
                ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
                ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
                SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
            TOKEN_QUERY, TOKEN_USER,
        },
        Storage::FileSystem::{
            CreateFileW, FileDispositionInfo, FileRenameInfo, GetDriveTypeW,
            GetFileInformationByHandle, GetVolumeInformationW, SetFileInformationByHandle,
            BY_HANDLE_FILE_INFORMATION, CREATE_NEW, DELETE, FILE_ATTRIBUTE_NORMAL,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_RENAME_INFO,
            FILE_SHARE_DELETE, FILE_SHARE_READ, OPEN_EXISTING, READ_CONTROL,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    pub(super) fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    struct Local(*mut c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn descriptor() -> Result<Local, CommandError> {
        unsafe {
            let mut token = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io(std::io::Error::last_os_error()));
            }
            let token = Handle(token);
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
                return Err(io(std::io::Error::last_os_error()));
            }
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let mut string_sid = null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut string_sid) == 0 {
                return Err(io(std::io::Error::last_os_error()));
            }
            let sid = Local(string_sid.cast());
            let mut length = 0;
            while *string_sid.add(length) != 0 {
                length += 1;
            }
            let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(string_sid, length));
            let sddl: Vec<u16> = format!("O:{sid_text}D:P(A;;FA;;;{sid_text})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            ) == 0
            {
                return Err(io(std::io::Error::last_os_error()));
            }
            drop(sid);
            Ok(Local(descriptor))
        }
    }

    pub(super) fn create_private(path: &Path) -> Result<File, CommandError> {
        let descriptor = descriptor()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        // CREATE_NEW and a protected, single-user DACL apply before the first plaintext byte exists.
        let handle = unsafe {
            CreateFileW(
                wide(path).as_ptr(),
                GENERIC_WRITE | DELETE | READ_CONTROL,
                FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io(std::io::Error::last_os_error()));
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    fn dacl_text(descriptor: &Local) -> Result<Vec<u16>, CommandError> {
        let mut text = null_mut();
        let mut length = 0;
        unsafe {
            if ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.0,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                &mut length,
            ) == 0
            {
                return Err(io(std::io::Error::last_os_error()));
            }
            let text = Local(text.cast());
            Ok(
                std::slice::from_raw_parts(text.0.cast::<u16>(), length.saturating_sub(1) as usize)
                    .to_vec(),
            )
        }
    }

    pub(super) fn verify_private_acl(file: &File) -> Result<(), CommandError> {
        let mut actual = null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut actual,
            )
        };
        if status != 0 {
            return Err(io(std::io::Error::from_raw_os_error(status as i32)));
        }
        let actual = Local(actual);
        if dacl_text(&actual)? != dacl_text(&descriptor()?)? {
            return Err(invalid(
                "The export requires protected owner-only Windows permissions.",
            ));
        }
        Ok(())
    }

    /// Whether Windows accepted the delete request. The handle identifies our file even if its
    /// temporary name has been replaced.
    pub(super) fn discard(file: &File) -> bool {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                std::ptr::from_ref(&info).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            ) != 0
        }
    }

    pub(super) fn replace(file: &File, target: &Path) -> Result<(), CommandError> {
        let name: Vec<u16> = target.as_os_str().encode_wide().collect();
        let bytes = name.len() * size_of::<u16>();
        let length = size_of::<FILE_RENAME_INFO>() + bytes;
        let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = true;
            (*info).FileNameLength = bytes
                .try_into()
                .map_err(|_| invalid("The export path is too long."))?;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                buffer
                    .as_mut_ptr()
                    .cast::<u8>()
                    .add(offset_of!(FILE_RENAME_INFO, FileName))
                    .cast::<u16>(),
                name.len(),
            );
            // Rename the owned handle and retain its private DACL when replacing an approved target.
            if SetFileInformationByHandle(
                file.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                length
                    .try_into()
                    .map_err(|_| invalid("The export path is too long."))?,
            ) == 0
            {
                return Err(io(std::io::Error::last_os_error()));
            }
        }
        Ok(())
    }

    pub(super) fn check_local_drive(path: &Path) -> Result<(), CommandError> {
        let raw = path
            .to_str()
            .ok_or_else(|| invalid("Choose a local path."))?;
        let root = wide(Path::new(&raw[..3]));
        let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
        if !matches!(drive_type, 2 | 3 | 6) {
            return Err(invalid(
                "Only local writable drives are allowed. Network drives are not allowed.",
            ));
        }
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            )
        } == 0
            || flags & 8 == 0
        {
            return Err(invalid(
                "This drive cannot enforce owner-only Windows permissions.",
            ));
        }
        Ok(())
    }

    pub(super) fn pin_local_parents(path: &Path) -> Result<Vec<File>, CommandError> {
        check_local_drive(path)?;
        let raw = path
            .to_str()
            .ok_or_else(|| invalid("Choose a local path."))?;
        let mut parents = Vec::new();
        let mut current = PathBuf::from(&raw[..3]);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("Choose a local folder."))?;
        let suffix = parent
            .to_str()
            .ok_or_else(|| invalid("Choose a local folder."))?;
        for component in
            std::iter::once("").chain(suffix[3..].split('\\').filter(|part| !part.is_empty()))
        {
            if !component.is_empty() {
                current.push(component);
            }
            let handle = unsafe {
                CreateFileW(
                    wide(&current).as_ptr(),
                    FILE_READ_ATTRIBUTES,
                    FILE_SHARE_READ,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                    null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io(std::io::Error::last_os_error()));
            }
            let file = unsafe { File::from_raw_handle(handle) };
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0 {
                return Err(io(std::io::Error::last_os_error()));
            }
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(invalid(
                    "Exports cannot traverse directory links, junctions or other reparse points.",
                ));
            }
            parents.push(file);
        }
        Ok(parents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> tempfile::TempDir {
        let base = std::env::temp_dir().join("opencode");
        fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new()
            .prefix("akm-desktop-export-")
            .tempdir_in(base)
            .unwrap()
    }

    fn write_private(
        path: &Path,
        contents: &str,
        protected: &[PathBuf],
        protected_dirs: &[PathBuf],
    ) -> Result<PathBuf, CommandError> {
        prepare_private(
            path,
            contents,
            protected,
            protected_dirs,
            &Remnants::default(),
        )?
        .publish()
    }

    #[cfg(windows)]
    fn displace_temp(directory: &Path) -> (PathBuf, PathBuf) {
        let original = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".akm-export-")
            })
            .expect("prepared export temporary file");
        let elsewhere = directory.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let displaced = elsewhere.join("renamed-owned.tmp");
        fs::rename(&original, &displaced).unwrap();
        fs::write(&original, b"unrelated replacement file\n").unwrap();
        (original, displaced)
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_removes_the_owned_file_after_its_temp_name_is_replaced() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let prepared = prepare_private(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &Remnants::default(),
        )
        .unwrap();
        let (replacement, displaced) = displace_temp(directory.path());

        drop(prepared);

        assert_eq!(
            fs::read(&replacement).ok().as_deref(),
            Some(b"unrelated replacement file\n".as_slice()),
            "cancellation must not delete a replacement at the old temporary name"
        );
        assert!(
            !displaced.exists(),
            "renamed owned plaintext must be removed"
        );
        assert!(!target.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        assert_eq!(
            fs::read_dir(displaced.parent().unwrap()).unwrap().count(),
            0
        );
    }

    #[cfg(windows)]
    #[test]
    fn publication_moves_the_owned_file_after_its_temp_name_is_replaced() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let prepared = prepare_private(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &Remnants::default(),
        )
        .unwrap();
        let (replacement, displaced) = displace_temp(directory.path());

        assert_eq!(prepared.publish().unwrap(), target);

        assert_eq!(fs::read(&target).unwrap(), b"TOKEN=fake-owned\n");
        assert_private_acl(&target);
        assert_eq!(
            fs::read(&replacement).unwrap(),
            b"unrelated replacement file\n"
        );
        assert!(
            !displaced.exists(),
            "publication must move the owned plaintext"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
        assert_eq!(
            fs::read_dir(displaced.parent().unwrap()).unwrap().count(),
            0
        );
    }

    #[test]
    fn export_paths_reject_network_device_startup_and_non_env_files() {
        for path in [
            r"\\server\share\out.env",
            r"\\?\C:\out.env",
            r"\\.\C:\out.env",
            r"C:out.env",
            r"C:\safe\out.env:stream",
            r"C:\safe\..\out.env",
            r"C:\safe\NUL.env",
            r"C:\safe\NUL .env",
            r"C:\safe\COM¹.env",
            r"C:\safe\CONOUT$.env",
            r"C:\Users\test\StartUp\out.env",
            r"C:\safe\vault.kdbx",
            r"C:\safe\out.env.",
        ] {
            assert!(check_windows_path(path).is_err(), "Accepted {path}");
        }
        assert!(check_windows_path(r"C:\Users\test\out.env").is_ok());
        assert!(check_windows_path(r"C:\Users\test\.env").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn failed_export_removes_its_unique_plaintext_temporary_file() {
        let directory = temporary_directory();
        let temp = prepare_temp(
            &directory.path().join("out.env"),
            "TOKEN=fake\n",
            &Remnants::default(),
            &mut SystemExportIo,
        )
        .unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        drop(temp);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    /// The Windows delete request, refused. It leaves the file where it is, as a real refusal does.
    #[cfg(windows)]
    fn delete_refused(_: &File) -> bool {
        false
    }

    /// An export system whose delete request is always refused, and that fails where it is told.
    #[cfg(windows)]
    #[derive(Default)]
    struct DeleteRefused {
        fail_sync: bool,
        fail_publish: bool,
    }

    #[cfg(windows)]
    impl ExportIo for DeleteRefused {
        fn sync(&mut self, file: &File) -> Result<(), CommandError> {
            file.sync_all().map_err(io)?;
            if self.fail_sync {
                return Err(io(std::io::Error::other("injected sync failure")));
            }
            Ok(())
        }

        fn publish(&mut self, _file: &File, _destination: &Path) -> Result<(), CommandError> {
            if self.fail_publish {
                return Err(io(std::io::Error::other("injected publication failure")));
            }
            Ok(())
        }

        fn discard(&self) -> Discard {
            delete_refused
        }
    }

    #[cfg(windows)]
    fn temporary_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".akm-export-")
            })
            .collect()
    }

    /// The remnant is real and the warning names it, and the warning holds no value.
    #[cfg(windows)]
    fn assert_remnant_is_named(directory: &Path, remnants: &Remnants) {
        let left = temporary_files(directory);
        assert_eq!(left.len(), 1, "the refused delete leaves the file");
        assert_eq!(
            fs::read(&left[0]).unwrap(),
            b"TOKEN=fake-owned\n",
            "the remnant holds the plaintext"
        );
        let warning = remnants
            .warning()
            .expect("an unconfirmed delete is reported");
        assert!(
            warning.contains(&left[0].display().to_string()),
            "the warning names the remnant: {warning}"
        );
        assert!(!warning.contains("fake-owned"), "{warning}");
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_refused_delete_after_a_failed_sync_names_the_remnant() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();
        let mut export_io = DeleteRefused {
            fail_sync: true,
            ..Default::default()
        };

        let error = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut export_io,
        )
        .err()
        .expect("injected sync failure");

        assert_eq!(error.message, "injected sync failure");
        assert_remnant_is_named(directory.path(), &remnants);
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_refused_delete_after_a_failed_publication_names_the_remnant() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();
        let prepared = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut DeleteRefused {
                fail_publish: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(remnants.warning().is_none(), "nothing is dropped yet");

        let error = prepared
            .publish_with(&mut DeleteRefused {
                fail_publish: true,
                ..Default::default()
            })
            .unwrap_err();

        assert_eq!(error.message, "injected publication failure");
        assert_remnant_is_named(directory.path(), &remnants);
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_refused_delete_after_a_cancelled_export_names_the_remnant() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();
        let prepared = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut DeleteRefused::default(),
        )
        .unwrap();

        drop(prepared);

        assert_remnant_is_named(directory.path(), &remnants);
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_failed_validation_after_the_write_with_a_refused_delete_names_the_remnant() {
        struct LateVault<'a>(&'a Path, DeleteRefused);
        impl ExportIo for LateVault<'_> {
            fn sync(&mut self, file: &File) -> Result<(), CommandError> {
                file.sync_all().map_err(io)?;
                fs::write(self.0, [0x03, 0xd9, 0xa2, 0x9a, 0x67, 0xfb, 0x4b, 0xb5]).map_err(io)
            }

            fn discard(&self) -> Discard {
                self.1.discard()
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();

        let error = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut LateVault(&target, DeleteRefused::default()),
        )
        .err()
        .expect("the destination became a vault");

        assert_eq!(error.kind, "invalidPath");
        let left = temporary_files(directory.path());
        assert_eq!(left.len(), 1);
        assert!(remnants
            .warning()
            .unwrap()
            .contains(&left[0].display().to_string()));
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_refused_delete_after_a_failed_write_names_the_remnant() {
        struct FailedWrite(DeleteRefused);
        impl ExportIo for FailedWrite {
            fn write(&mut self, file: &mut File, contents: &str) -> Result<(), CommandError> {
                // Part of the plaintext reaches the file before the write fails.
                file.write_all(&contents.as_bytes()[..contents.len() / 2])
                    .map_err(io)?;
                Err(io(std::io::Error::other("injected write failure")))
            }

            fn discard(&self) -> Discard {
                self.0.discard()
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();

        let error = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut FailedWrite(DeleteRefused::default()),
        )
        .err()
        .expect("injected write failure");

        assert_eq!(error.message, "injected write failure");
        let left = temporary_files(directory.path());
        assert_eq!(left.len(), 1, "the refused delete leaves the file");
        assert_eq!(
            fs::read(&left[0]).unwrap(),
            b"TOKEN=fa",
            "part of the plaintext"
        );
        let warning = remnants
            .warning()
            .expect("an unconfirmed delete is reported");
        assert!(
            warning.contains(&left[0].display().to_string()),
            "the warning names the remnant: {warning}"
        );
        assert!(
            !warning.contains("fake-owned") && !warning.contains("TOKEN"),
            "{warning}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn fr04_a_confirmed_delete_reports_nothing_whichever_way_the_export_ends() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let remnants = Remnants::default();

        let failed = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &remnants,
            &mut FailedSync,
        );
        assert!(failed.is_err());
        drop(prepare_private(&target, "TOKEN=fake-owned\n", &[], &[], &remnants).unwrap());

        assert!(temporary_files(directory.path()).is_empty());
        assert!(remnants.warning().is_none());

        struct FailedSync;
        impl ExportIo for FailedSync {
            fn sync(&mut self, _file: &File) -> Result<(), CommandError> {
                Err(io(std::io::Error::other("injected sync failure")))
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn fr04_the_windows_delete_request_reports_whether_it_was_accepted() {
        let directory = temporary_directory();
        let file = windows::create_private(&directory.path().join("probe.tmp")).unwrap();
        assert!(windows::discard(&file), "Windows accepts a delete request");
        drop(file);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[cfg(not(windows))]
    #[test]
    fn private_file_export_requires_windows() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");

        let error = create_private(&target).unwrap_err();

        assert_eq!(error.kind, "invalidPath");
        assert!(error.message.contains("requires Windows"));
        assert!(!target.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_create_does_not_claim_an_existing_temporary_name() {
        struct ExistingTemp(Option<PathBuf>);
        impl ExportIo for ExistingTemp {
            fn create_private(&mut self, path: &Path) -> Result<File, CommandError> {
                fs::write(path, b"unrelated replacement file\n").map_err(io)?;
                self.0 = Some(path.to_path_buf());
                create_private(path)
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let mut export_io = ExistingTemp(None);

        let error = prepare_private_with(
            &target,
            "TOKEN=fake\n",
            &[],
            &[],
            &Remnants::default(),
            &mut export_io,
        )
        .err()
        .expect("CREATE_NEW must refuse an existing temporary name");

        assert_eq!(error.kind, "io");
        assert_eq!(
            fs::read(export_io.0.unwrap()).unwrap(),
            b"unrelated replacement file\n"
        );
        assert!(!target.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn an_ignored_private_dacl_is_rejected_before_plaintext_is_written() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{
            Foundation::GENERIC_WRITE,
            Storage::FileSystem::{DELETE, FILE_SHARE_DELETE, READ_CONTROL},
        };

        struct IgnoredAcl {
            wrote: bool,
        }

        impl ExportIo for IgnoredAcl {
            fn create_private(&mut self, path: &Path) -> Result<File, CommandError> {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .access_mode(GENERIC_WRITE | DELETE | READ_CONTROL)
                    .share_mode(FILE_SHARE_DELETE)
                    .open(path)
                    .map_err(io)
            }

            fn write(&mut self, file: &mut File, contents: &str) -> Result<(), CommandError> {
                self.wrote = true;
                file.write_all(contents.as_bytes()).map_err(io)
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let mut export_io = IgnoredAcl { wrote: false };
        let result = prepare_private_with(
            &target,
            "TOKEN=fake\n",
            &[],
            &[],
            &Remnants::default(),
            &mut export_io,
        );
        assert!(
            result.is_err() && !export_io.wrote,
            "an unverified DACL must fail before any plaintext is written"
        );
        assert!(result
            .err()
            .unwrap()
            .message
            .contains("owner-only Windows permissions"));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn preparation_failures_remove_renamed_plaintext_and_preserve_replacement_files() {
        #[derive(Clone, Copy, Debug)]
        enum Failure {
            PartialWrite,
            Flush,
            Sync,
        }

        struct FailedPreparation {
            failure: Failure,
            directory: PathBuf,
            displaced: Option<(PathBuf, PathBuf)>,
        }

        impl FailedPreparation {
            fn fail(&mut self, file: &File, expected_length: u64) -> Result<(), CommandError> {
                assert_eq!(file.metadata().unwrap().len(), expected_length);
                self.displaced = Some(displace_temp(&self.directory));
                Err(io(std::io::Error::other(format!(
                    "injected {:?} failure",
                    self.failure
                ))))
            }
        }

        impl ExportIo for FailedPreparation {
            fn write(&mut self, file: &mut File, contents: &str) -> Result<(), CommandError> {
                if matches!(self.failure, Failure::PartialWrite) {
                    file.write_all(&contents.as_bytes()[..7]).map_err(io)?;
                    self.fail(file, 7)
                } else {
                    file.write_all(contents.as_bytes()).map_err(io)
                }
            }

            fn flush(&mut self, file: &mut File) -> Result<(), CommandError> {
                file.flush().map_err(io)?;
                if matches!(self.failure, Failure::Flush) {
                    self.fail(file, 17)
                } else {
                    Ok(())
                }
            }

            fn sync(&mut self, file: &File) -> Result<(), CommandError> {
                file.sync_all().map_err(io)?;
                self.fail(file, 17)
            }
        }

        for failure in [Failure::PartialWrite, Failure::Flush, Failure::Sync] {
            let directory = temporary_directory();
            let target = directory.path().join("out.env");
            fs::write(&target, b"previous approved export\n").unwrap();
            let mut export_io = FailedPreparation {
                failure,
                directory: directory.path().to_path_buf(),
                displaced: None,
            };
            let error = prepare_private_with(
                &target,
                "TOKEN=fake-owned\n",
                &[],
                &[],
                &Remnants::default(),
                &mut export_io,
            )
            .err()
            .expect("injected preparation failure");

            assert_eq!(error.kind, "io");
            assert_eq!(error.message, format!("injected {failure:?} failure"));
            let (replacement, displaced) = export_io.displaced.unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"previous approved export\n");
            assert_eq!(
                fs::read(replacement).unwrap(),
                b"unrelated replacement file\n"
            );
            assert!(!displaced.exists(), "{failure:?} leaked owned plaintext");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
            assert_eq!(
                fs::read_dir(displaced.parent().unwrap()).unwrap().count(),
                0
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn publication_failure_removes_renamed_plaintext_and_preserves_replacement_files() {
        struct FailedPublication {
            displaced: Option<(PathBuf, PathBuf)>,
        }

        impl ExportIo for FailedPublication {
            fn publish(&mut self, file: &File, destination: &Path) -> Result<(), CommandError> {
                assert_eq!(file.metadata().unwrap().len(), 17);
                self.displaced = Some(displace_temp(destination.parent().unwrap()));
                Err(io(std::io::Error::other("injected publication failure")))
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        fs::write(&target, b"previous approved export\n").unwrap();
        let prepared = prepare_private(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &Remnants::default(),
        )
        .unwrap();
        let mut export_io = FailedPublication { displaced: None };

        let error = prepared.publish_with(&mut export_io).unwrap_err();

        assert_eq!(error.kind, "io");
        assert_eq!(error.message, "injected publication failure");
        let (replacement, displaced) = export_io.displaced.unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"previous approved export\n");
        assert_eq!(
            fs::read(replacement).unwrap(),
            b"unrelated replacement file\n"
        );
        assert!(
            !displaced.exists(),
            "failed publication leaked owned plaintext"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
        assert_eq!(
            fs::read_dir(displaced.parent().unwrap()).unwrap().count(),
            0
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_native_rename_is_not_reported_as_a_saved_export() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        fs::write(&target, b"previous approved export\n").unwrap();
        let prepared = prepare_private(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &Remnants::default(),
        )
        .unwrap();
        let (replacement, displaced) = displace_temp(directory.path());
        let blocker = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&target)
            .unwrap();

        let error = prepared.publish().unwrap_err();

        assert_eq!(error.kind, "io");
        drop(blocker);
        assert_eq!(fs::read(&target).unwrap(), b"previous approved export\n");
        assert_eq!(
            fs::read(replacement).unwrap(),
            b"unrelated replacement file\n"
        );
        assert!(!displaced.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
        assert_eq!(
            fs::read_dir(displaced.parent().unwrap()).unwrap().count(),
            0
        );
    }

    #[cfg(windows)]
    #[test]
    fn preparation_revalidates_the_destination_after_sync() {
        const FAKE_VAULT: [u8; 8] = [0x03, 0xd9, 0xa2, 0x9a, 0x67, 0xfb, 0x4b, 0xb5];

        struct LateVault(PathBuf);
        impl ExportIo for LateVault {
            fn sync(&mut self, file: &File) -> Result<(), CommandError> {
                file.sync_all().map_err(io)?;
                fs::write(&self.0, FAKE_VAULT).map_err(io)
            }
        }

        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        let error = prepare_private_with(
            &target,
            "TOKEN=fake-owned\n",
            &[],
            &[],
            &Remnants::default(),
            &mut LateVault(target.clone()),
        )
        .err()
        .expect("destination must be revalidated before preparation finishes");

        assert_eq!(error.kind, "invalidPath");
        assert!(error.message.contains("KeePass vault"));
        assert_eq!(fs::read(&target).unwrap(), FAKE_VAULT);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    fn assert_private_acl(path: &Path) {
        use std::{os::windows::io::AsRawHandle, ptr::null_mut};
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
                EqualSid, GetAce, GetSecurityDescriptorControl, ACCESS_ALLOWED_ACE, ACL,
                DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, SE_DACL_PROTECTED,
            },
            Storage::FileSystem::FILE_ALL_ACCESS,
        };
        let file = File::open(path).unwrap();
        unsafe {
            let mut owner = null_mut();
            let mut acl: *mut ACL = null_mut();
            let mut descriptor = null_mut();
            assert_eq!(
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
                    &mut owner,
                    null_mut(),
                    &mut acl,
                    null_mut(),
                    &mut descriptor
                ),
                0
            );
            let mut control = 0;
            let mut revision = 0;
            assert_ne!(
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert!(!acl.is_null());
            assert_eq!((*acl).AceCount, 1);
            let mut ace = null_mut();
            assert_ne!(GetAce(acl, 0, &mut ace), 0);
            let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
            assert_eq!(ace.Header.AceType, 0, "ACCESS_ALLOWED_ACE_TYPE");
            assert_eq!(ace.Header.AceFlags, 0);
            assert_eq!(ace.Mask, FILE_ALL_ACCESS);
            assert_ne!(
                EqualSid(owner, std::ptr::addr_of!(ace.SidStart).cast_mut().cast()),
                0
            );
            LocalFree(descriptor);
        }
    }

    #[cfg(windows)]
    #[test]
    fn overwriting_an_existing_file_keeps_the_export_private() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        fs::write(&target, b"previous approved export\n").unwrap();
        assert!(windows::verify_private_acl(&File::open(&target).unwrap()).is_err());

        assert_eq!(
            write_private(&target, "TOKEN=fake-owned\n", &[], &[]).unwrap(),
            target
        );

        assert_eq!(fs::read(&target).unwrap(), b"TOKEN=fake-owned\n");
        assert_private_acl(&target);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn atomic_exports_preserve_protected_files_and_use_owner_only_acl() {
        let directory = temporary_directory();
        let target = directory.path().join("out.env");
        write_private(&target, "TOKEN=fake\n", &[], &[]).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "TOKEN=fake\n");
        assert_private_acl(&target);
        assert!(write_private(
            &target,
            "TOKEN=changed\n",
            std::slice::from_ref(&target),
            &[]
        )
        .is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "TOKEN=fake\n");
        write_private(&target, "TOKEN=replaced\n", &[], &[]).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "TOKEN=replaced\n");
        assert_private_acl(&target);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_differently_named_vault_is_not_overwritten_as_plaintext() {
        let directory = temporary_directory();
        let vault = directory.path().join("other.env");
        let fake_vault = [0x03, 0xd9, 0xa2, 0x9a, 0x67, 0xfb, 0x4b, 0xb5];
        fs::write(&vault, fake_vault).unwrap();
        assert!(write_private(&vault, "TOKEN=fake\n", &[], &[]).is_err());
        assert_eq!(fs::read(&vault).unwrap(), fake_vault);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn vault_siblings_and_configuration_directories_are_protected() {
        let directory = temporary_directory();
        let vault = directory.path().join("vault.kdbx");
        let backup = directory.path().join("vault.kdbx.bak.env");
        assert!(write_private(&backup, "TOKEN=fake\n", &[vault], &[]).is_err());
        let config = directory.path().join("config");
        fs::create_dir(&config).unwrap();
        let target = config.join("out.env");
        assert!(
            write_private(&target, "TOKEN=fake\n", &[], std::slice::from_ref(&config)).is_err()
        );
        assert!(!backup.exists());
        assert_eq!(fs::read_dir(config).unwrap().count(), 0);
    }
}
