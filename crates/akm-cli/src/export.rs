use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use akm_core::{Error, Result};
use zeroize::Zeroizing;

pub fn destination(path: &Path) -> Result<PathBuf> {
    destination_with_query(path, filesystem_flags_for_path)
}

pub(super) fn destination_with_query(
    path: &Path,
    query: impl FnOnce(&Path) -> Result<u32>,
) -> Result<PathBuf> {
    validate_path(path)?;
    let path = resolved_destination(path)?;
    ensure_private_filesystem(query(path.parent().ok_or_else(unsafe_destination)?)?)?;
    Ok(path)
}

fn ensure_private_filesystem(flags: u32) -> Result<()> {
    const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
    if flags & FILE_PERSISTENT_ACLS == 0 {
        return Err(Error::Invalid(
            "export storage cannot enforce owner-only Windows permissions (persistent ACLs required)"
                .into(),
        ));
    }
    Ok(())
}

fn resolved_destination(path: &Path) -> Result<PathBuf> {
    let name = path.file_name().ok_or_else(unsafe_destination)?;
    validate_path(Path::new(name))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    validate_parent(&parent)?;
    Ok(parent.join(name))
}

pub fn write_new(path: &Path, envs: &[(String, String)]) -> Result<()> {
    write_new_with(path, envs, &mut SystemExportIo)
}

pub(super) trait ExportIo {
    fn create_private(&mut self, path: &Path) -> Result<File> {
        private_file(path)
    }

    fn filesystem_flags(&mut self, file: &File) -> Result<u32> {
        filesystem_flags_for_file(file)
    }

    fn write(&mut self, file: &mut File, envs: &[(String, String)]) -> Result<()> {
        write(file, envs)
    }

    fn sync(&mut self, file: &File) -> Result<()> {
        file.sync_all()?;
        Ok(())
    }

    fn publish(&mut self, file: &File, destination: &Path) -> Result<()> {
        publish_new(file, destination)
    }

    /// Remove the owned plaintext temporary file. `false` means the removal is not confirmed.
    fn discard(&mut self, file: &File) -> bool {
        discard(file)
    }
}

#[cfg(windows)]
fn filesystem_flags_for_path(path: &Path) -> Result<u32> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = vec![0u16; 32768];
    let mut flags = 0;
    unsafe {
        if GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if !matches!(GetDriveTypeW(root.as_ptr()), 2 | 3 | 6) {
            return Err(unsafe_destination());
        }
        if GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(flags)
}

#[cfg(windows)]
fn filesystem_flags_for_file(file: &File) -> Result<u32> {
    use std::os::windows::io::AsRawHandle;
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;

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
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(flags)
}

#[cfg(not(windows))]
fn filesystem_flags_for_path(_path: &Path) -> Result<u32> {
    Err(Error::Invalid(
        "private file export requires Windows".into(),
    ))
}

#[cfg(not(windows))]
fn filesystem_flags_for_file(_file: &File) -> Result<u32> {
    Err(Error::Invalid(
        "private file export requires Windows".into(),
    ))
}

struct SystemExportIo;
impl ExportIo for SystemExportIo {}

pub(super) fn write_new_with(
    path: &Path,
    envs: &[(String, String)],
    io: &mut impl ExportIo,
) -> Result<()> {
    let path = resolved_destination(path)?;
    let mut temp = PrivateTemp::create(&path, io)?;
    let outcome = fill_and_publish(&mut temp, &path, envs, io);
    temp.settled = true;
    match outcome {
        Ok(()) => Ok(()),
        Err(error) if io.discard(&temp.file) => Err(error),
        Err(error) => Err(Error::Io(std::io::Error::other(format!(
            "{error}; could not confirm cleanup: the plaintext temporary file {} may still hold the exported values. Delete it yourself",
            temp.path.display()
        )))),
    }
}

fn fill_and_publish(
    temp: &mut PrivateTemp,
    path: &Path,
    envs: &[(String, String)],
    io: &mut impl ExportIo,
) -> Result<()> {
    ensure_private_filesystem(io.filesystem_flags(&temp.file)?)?;
    verify_private_acl(&temp.file)?;
    io.write(&mut temp.file, envs)?;
    io.sync(&temp.file)?;
    io.publish(&temp.file, path)
}

#[cfg(windows)]
fn verify_private_acl(file: &File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    let mut descriptor = null_mut();
    unsafe {
        let status = GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        );
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status as i32).into());
        }
        let mut text = null_mut();
        let mut length = 0;
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            &mut length,
        );
        let error = std::io::Error::last_os_error();
        LocalFree(descriptor);
        if converted == 0 {
            return Err(error.into());
        }
        let actual = std::slice::from_raw_parts(text, length.saturating_sub(1) as usize);
        let private = actual.iter().copied().eq("D:P(A;;FA;;;OW)".encode_utf16());
        LocalFree(text.cast());
        if !private {
            return Err(Error::Invalid(
                "export requires protected owner-only Windows permissions".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn verify_private_acl(_file: &File) -> Result<()> {
    Err(Error::Invalid(
        "private file export requires Windows".into(),
    ))
}

struct PrivateTemp {
    file: File,
    path: PathBuf,
    settled: bool,
}

impl PrivateTemp {
    fn create(destination: &Path, io: &mut impl ExportIo) -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        let parent = destination.parent().ok_or_else(unsafe_destination)?;
        for _ in 0..128 {
            let name = format!(
                ".akm-export-{}-{}.tmp",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            );
            if destination
                .file_name()
                .is_some_and(|target| target.to_string_lossy().eq_ignore_ascii_case(&name))
            {
                continue;
            }
            let path = parent.join(name);
            match io.create_private(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file,
                        path,
                        settled: false,
                    });
                }
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create a private export temporary file",
        )
        .into())
    }
}

impl Drop for PrivateTemp {
    fn drop(&mut self) {
        if !self.settled {
            discard(&self.file);
        }
    }
}

#[cfg(windows)]
fn discard(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };

    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    // The handle still identifies our file even if its name has been replaced.
    unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            std::ptr::from_ref(&info).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        ) != 0
    }
}

#[cfg(windows)]
pub(super) fn publish_new(file: &File, destination: &Path) -> Result<()> {
    use std::mem::{offset_of, size_of};
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
    };

    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let bytes = name.len() * size_of::<u16>();
    let length = size_of::<FILE_RENAME_INFO>() + bytes;
    let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).FileNameLength = bytes.try_into().map_err(|_| unsafe_destination())?;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            buffer
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset_of!(FILE_RENAME_INFO, FileName))
                .cast::<u16>(),
            name.len(),
        );
        // Rename the owned handle, never a source name another process can replace.
        if SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            length.try_into().map_err(|_| unsafe_destination())?,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn discard(_file: &File) -> bool {
    false
}

#[cfg(not(windows))]
pub(super) fn publish_new(_file: &File, _destination: &Path) -> Result<()> {
    Err(Error::Invalid(
        "private file export requires Windows".into(),
    ))
}

pub fn write(output: &mut impl Write, envs: &[(String, String)]) -> Result<()> {
    for (name, value) in envs {
        let line = Zeroizing::new(akm_core::dotenv::line(name, value));
        output.write_all(line.as_bytes())?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}

fn unsafe_destination() -> Error {
    Error::Invalid(
        "export requires a local file path, not a UNC, device, or alternate-stream path".into(),
    )
}

#[cfg(windows)]
fn validate_path(path: &Path) -> Result<()> {
    use std::path::{Component, Prefix};

    for component in path.components() {
        match component {
            Component::Prefix(prefix) if !matches!(prefix.kind(), Prefix::Disk(_)) => {
                return Err(unsafe_destination());
            }
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                let stem = name
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                if name.contains(':')
                    || name.ends_with(['.', ' '])
                    || matches!(
                        stem.as_str(),
                        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                    )
                    || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                        && stem.len() == 4
                        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
                {
                    return Err(unsafe_destination());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_parent(parent: &Path) -> Result<()> {
    use std::path::{Component, Prefix};

    match parent.components().next() {
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) =>
        {
            Ok(())
        }
        _ => Err(unsafe_destination()),
    }
}

#[cfg(not(windows))]
fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") || text.starts_with("/dev/") {
        return Err(unsafe_destination());
    }
    Ok(())
}

#[cfg(not(windows))]
fn validate_parent(parent: &Path) -> Result<()> {
    validate_path(parent)
}

#[cfg(windows)]
fn private_file(path: &Path) -> Result<File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{LocalFree, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
        FILE_SHARE_DELETE, READ_CONTROL,
    };

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if path[..path.len() - 1].contains(&0) {
        return Err(unsafe_destination());
    }
    // OWNER RIGHTS grants only the new file's owner access, with no inherited ACEs.
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)".encode_utf16().chain(Some(0)).collect();
    let mut descriptor = null_mut();
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let handle = CreateFileW(
            path.as_ptr(),
            FILE_GENERIC_WRITE | DELETE | READ_CONTROL,
            FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        let error = std::io::Error::last_os_error();
        LocalFree(descriptor);
        if handle == INVALID_HANDLE_VALUE {
            return Err(error.into());
        }
        // CreateFileW returns a fresh owned handle; File closes it on every exit path.
        Ok(File::from_raw_handle(handle))
    }
}

#[cfg(not(windows))]
fn private_file(_path: &Path) -> Result<File> {
    Err(Error::Invalid(
        "private file export requires Windows".into(),
    ))
}
