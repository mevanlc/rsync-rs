//! Windows directory-symlink creation with an unprivileged junction fallback.
//!
//! `CreateSymbolicLinkW` needs either administrator rights or Windows Developer
//! Mode; an ordinary user hits `ERROR_PRIVILEGE_NOT_HELD` (1314). A directory
//! symlink has a privilege-free equivalent - a junction (mount-point reparse
//! point, what `mklink /J` creates) - so this module first tries the real
//! symlink (adding `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` so it
//! succeeds under Developer Mode) and, on a privilege refusal, falls back to a
//! junction built with `DeviceIoControl(FSCTL_SET_REPARSE_POINT)`.
//!
//! File symlinks have no junction equivalent: [`create_file_symlink`] returns
//! the raw `ERROR_PRIVILEGE_NOT_HELD` so the caller can skip the entry with a
//! warning and a soft (exit 23) error rather than aborting.
//!
//! A symbolic link created inside WSL on an NTFS volume is a third kind: an
//! `IO_REPARSE_TAG_LX_SYMLINK` reparse point holding the POSIX target as
//! UTF-8. `std::fs` reports it as a symlink but cannot read it ("Unsupported
//! reparse point type"), so [`read_link`] reads it here, and
//! [`create_wsl_symlink`] writes one back, which needs no privilege. Cygwin,
//! upstream rsync's Windows runtime, reads these as symlinks too.
//!
//! This is the designated encapsulation point for the raw Win32 reparse-point
//! calls; consumer crates (`engine`, `transfer`) reach it through the safe
//! functions here and never touch `unsafe` themselves.

use std::io;
use std::path::{Path, PathBuf};

/// Reparse tag of a symbolic link WSL creates on an NTFS volume.
#[cfg(windows)]
const IO_REPARSE_TAG_LX_SYMLINK: u32 = 0xA000_001D;

/// Format version WSL writes ahead of an LX symlink's target. The target
/// follows as UTF-8 with no terminator.
#[cfg(windows)]
const LX_SYMLINK_VERSION: u32 = 2;

/// Largest reparse buffer the kernel accepts or returns.
#[cfg(windows)]
const MAX_REPARSE_DATA_BUFFER_SIZE: usize = 16 * 1024;

/// Reads the target of the symbolic link at `path`, including a WSL symlink
/// that [`std::fs::read_link`] refuses.
///
/// # Errors
///
/// Returns [`std::fs::read_link`]'s error when `path` is not a symbolic link
/// of any kind it or WSL writes.
pub fn read_link(path: &Path) -> io::Result<PathBuf> {
    let error = match std::fs::read_link(path) {
        Ok(target) => return Ok(target),
        Err(error) => error,
    };
    #[cfg(windows)]
    if let Ok(Some(target)) = read_reparse_buffer(path).map(|buffer| wsl_symlink_target(&buffer)) {
        return Ok(target);
    }
    Err(error)
}

/// Reports whether `path` is a symbolic link WSL created.
#[cfg(windows)]
pub fn is_wsl_symlink(path: &Path) -> bool {
    read_reparse_buffer(path)
        .is_ok_and(|buffer| reparse_tag(&buffer) == Some(IO_REPARSE_TAG_LX_SYMLINK))
}

/// Creates a symbolic link at `link` pointing to `target` in the form WSL
/// writes, so WSL resolves the copy exactly as it did the original. Unlike
/// [`create_file_symlink`] this needs no privilege.
///
/// # Errors
///
/// Fails with [`io::ErrorKind::InvalidInput`] when `target` is not UTF-8 or
/// too long for a reparse point, and otherwise propagates the failure to
/// create `link` or to set its reparse point; `link` is removed again in the
/// latter case.
#[cfg(windows)]
pub fn create_wsl_symlink(target: &Path, link: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let target = target.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "WSL symlink target is not valid UTF-8",
        )
    })?;
    let buffer = wsl_symlink_buffer(target)?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(link)?;
    if let Err(error) = set_reparse_point(&file, &buffer) {
        drop(file);
        let _ = std::fs::remove_file(link);
        return Err(error);
    }
    Ok(())
}

/// Builds the reparse buffer WSL writes for a link to `target`.
#[cfg(windows)]
fn wsl_symlink_buffer(target: &str) -> io::Result<Vec<u8>> {
    // ReparseTag(4) + ReparseDataLength(2) + Reserved(2), then the data.
    let data_length = 4 + target.len();
    let too_long = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "WSL symlink target exceeds the maximum reparse-point size",
        )
    };
    if 8 + data_length > MAX_REPARSE_DATA_BUFFER_SIZE {
        return Err(too_long());
    }
    let length = u16::try_from(data_length).map_err(|_| too_long())?;
    let mut buffer = Vec::with_capacity(8 + data_length);
    buffer.extend_from_slice(&IO_REPARSE_TAG_LX_SYMLINK.to_le_bytes());
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&LX_SYMLINK_VERSION.to_le_bytes());
    buffer.extend_from_slice(target.as_bytes());
    Ok(buffer)
}

/// Returns the reparse tag at the head of a reparse buffer.
#[cfg(windows)]
fn reparse_tag(buffer: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(buffer.get(..4)?.try_into().ok()?))
}

/// Returns the target of a WSL symlink's reparse buffer, or `None` when the
/// buffer holds another kind of reparse point or a version this does not read.
#[cfg(windows)]
fn wsl_symlink_target(buffer: &[u8]) -> Option<PathBuf> {
    if reparse_tag(buffer)? != IO_REPARSE_TAG_LX_SYMLINK {
        return None;
    }
    let length = usize::from(u16::from_le_bytes(buffer.get(4..6)?.try_into().ok()?));
    let data = buffer.get(8..8 + length)?;
    if u32::from_le_bytes(data.get(..4)?.try_into().ok()?) != LX_SYMLINK_VERSION {
        return None;
    }
    std::str::from_utf8(&data[4..]).ok().map(PathBuf::from)
}

/// Reads the reparse buffer of `path` without following it.
#[cfg(windows)]
#[allow(unsafe_code)]
fn read_reparse_buffer(path: &Path) -> io::Result<Vec<u8>> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;

    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut buffer = vec![0u8; MAX_REPARSE_DATA_BUFFER_SIZE];
    let mut returned: u32 = 0;
    // SAFETY: `file` owns a live handle opened with reparse-point semantics.
    // FSCTL_GET_REPARSE_POINT takes no input and writes at most `buffer.len()`
    // bytes into the heap buffer, reporting the count through `returned`;
    // `lpOverlapped` is null for the synchronous handle.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as HANDLE,
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(returned as usize);
    Ok(buffer)
}

/// Writes the reparse buffer `buffer` onto the open `file`.
#[cfg(windows)]
#[allow(unsafe_code)]
fn set_reparse_point(file: &std::fs::File, buffer: &[u8]) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    let mut bytes_returned: u32 = 0;
    // SAFETY: `file` owns a live handle opened with write access and
    // reparse-point semantics. `buffer` is a correctly laid-out
    // REPARSE_DATA_BUFFER of `buffer.len()` bytes. FSCTL_SET_REPARSE_POINT
    // writes no output, so the out buffer is null / zero-length;
    // `lpBytesReturned` points at a valid u32 and `lpOverlapped` is null for
    // the synchronous handle.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as HANDLE,
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Which reparse-point kind actually landed on disk for a Windows directory
/// link request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsDirLink {
    /// A true directory symbolic link (`IO_REPARSE_TAG_SYMLINK`), created when
    /// the caller held the create-symlink privilege (admin or Developer Mode).
    Symlink,
    /// A junction / mount point (`IO_REPARSE_TAG_MOUNT_POINT`), created as the
    /// unprivileged fallback after `CreateSymbolicLinkW` was refused.
    Junction,
}

/// Creates a directory link at `link` pointing to `target`, preferring a real
/// directory symbolic link and falling back to a junction when the caller
/// lacks the create-symlink privilege.
///
/// Returns which kind was created. The junction fallback resolves `target` to
/// an absolute path (junctions require an absolute NT-namespace target), so a
/// junction created from a relative `target` points at the resolved location.
///
/// # Errors
///
/// Propagates any error other than the privilege refusal that triggers the
/// junction fallback (for example a missing parent directory or a junction
/// `DeviceIoControl` failure).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn create_directory_symlink_or_junction(
    target: &Path,
    link: &Path,
) -> io::Result<WindowsDirLink> {
    match try_create_symlink(target, link, true) {
        Ok(()) => Ok(WindowsDirLink::Symlink),
        Err(err) if is_unprivileged_symlink_error(&err) => {
            create_junction(target, link)?;
            Ok(WindowsDirLink::Junction)
        }
        Err(err) => Err(err),
    }
}

/// Creates a file symbolic link at `link` pointing to `target`.
///
/// Adds `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE` so the call succeeds
/// under Developer Mode. Unlike a directory link there is no privilege-free
/// fallback, so a privilege refusal surfaces as `ERROR_PRIVILEGE_NOT_HELD`;
/// callers detect it with [`is_unprivileged_symlink_error`] and skip the entry.
///
/// # Errors
///
/// Propagates the underlying `CreateSymbolicLinkW` failure, including the
/// `ERROR_PRIVILEGE_NOT_HELD` privilege refusal.
#[cfg(windows)]
pub fn create_file_symlink(target: &Path, link: &Path) -> io::Result<()> {
    try_create_symlink(target, link, false)
}

/// Reports whether `err` is the Windows "a required privilege is not held by
/// the client" failure (`ERROR_PRIVILEGE_NOT_HELD`, 1314) that unprivileged
/// symlink creation raises.
#[cfg(windows)]
pub fn is_unprivileged_symlink_error(err: &io::Error) -> bool {
    err.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_PRIVILEGE_NOT_HELD as i32)
}

/// Attempts `CreateSymbolicLinkW`, preferring the unprivileged-create flag and
/// retrying without it on the older-Windows `ERROR_INVALID_PARAMETER` refusal
/// so the caller still observes the `ERROR_PRIVILEGE_NOT_HELD` that gates the
/// junction fallback.
#[cfg(windows)]
#[allow(unsafe_code)]
fn try_create_symlink(target: &Path, link: &Path, directory: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateSymbolicLinkW, SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE,
        SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let link_w: Vec<u16> = link
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_w: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let base = if directory {
        SYMBOLIC_LINK_FLAG_DIRECTORY
    } else {
        0
    };

    // SAFETY: both wide buffers are NUL-terminated and live for the whole
    // call; `CreateSymbolicLinkW` only reads them and returns success as a bool.
    let ok = unsafe {
        CreateSymbolicLinkW(
            link_w.as_ptr(),
            target_w.as_ptr(),
            base | SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE,
        )
    };
    if ok {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() != Some(ERROR_INVALID_PARAMETER as i32) {
        return Err(err);
    }

    // Older Windows (pre-1703) rejects the unprivileged flag outright; retry
    // without it so an admin / Developer-Mode caller still succeeds and an
    // unprivileged one gets the ERROR_PRIVILEGE_NOT_HELD the caller expects.
    // SAFETY: identical invariants to the call above.
    let ok = unsafe { CreateSymbolicLinkW(link_w.as_ptr(), target_w.as_ptr(), base) };
    if ok {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

/// Builds a junction (mount-point reparse point) at `link` pointing to the
/// absolute location of `target`, mirroring `mklink /J`.
#[cfg(windows)]
fn create_junction(target: &Path, link: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    // \\?\  verbatim prefix that canonicalize() prepends.
    const VERBATIM_PREFIX: [u16; 4] = [0x5C, 0x5C, 0x3F, 0x5C];
    // \??\  NT-namespace prefix a mount-point substitute name requires.
    const NT_PREFIX: [u16; 4] = [0x5C, 0x3F, 0x3F, 0x5C];

    // Junctions store an absolute NT-namespace target. Resolve `target` (which
    // may be relative) against the real filesystem when it exists, else fall
    // back to a lexical absolute path.
    let absolute = match std::fs::canonicalize(target) {
        Ok(p) => p,
        Err(_) => std::path::absolute(target)?,
    };

    let mut dos: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    if dos.starts_with(&VERBATIM_PREFIX) {
        dos.drain(0..VERBATIM_PREFIX.len());
    }

    let mut substitute: Vec<u16> = NT_PREFIX.to_vec();
    substitute.extend_from_slice(&dos);
    let print = dos;

    let substitute_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    // PathBuffer holds substitute + NUL + print + NUL (UTF-16 units).
    let path_buffer_bytes = substitute_bytes + 2 + print_bytes + 2;
    // MountPointReparseBuffer prefix is four USHORT offset/length fields.
    let reparse_data_length = 8 + path_buffer_bytes;
    // Full buffer: ReparseTag(4) + ReparseDataLength(2) + Reserved(2) + data.
    let total = 8 + reparse_data_length;
    if total > MAX_REPARSE_DATA_BUFFER_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "junction target path exceeds the maximum reparse-point size",
        ));
    }

    let mut buf: Vec<u8> = Vec::with_capacity(total);
    buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend_from_slice(&(reparse_data_length as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    buf.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
    buf.extend_from_slice(&(substitute_bytes as u16).to_le_bytes()); // SubstituteNameLength
    buf.extend_from_slice(&((substitute_bytes + 2) as u16).to_le_bytes()); // PrintNameOffset
    buf.extend_from_slice(&(print_bytes as u16).to_le_bytes()); // PrintNameLength
    for unit in &substitute {
        buf.extend_from_slice(&unit.to_le_bytes());
    }
    buf.extend_from_slice(&0u16.to_le_bytes()); // substitute NUL
    for unit in &print {
        buf.extend_from_slice(&unit.to_le_bytes());
    }
    buf.extend_from_slice(&0u16.to_le_bytes()); // print NUL

    // A junction is an empty directory carrying the reparse point.
    std::fs::create_dir(link)?;

    let dir = match std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(link)
    {
        Ok(dir) => dir,
        Err(err) => {
            let _ = std::fs::remove_dir(link);
            return Err(err);
        }
    };

    if let Err(err) = set_reparse_point(&dir, &buf) {
        drop(dir);
        let _ = std::fs::remove_dir(link);
        return Err(err);
    }
    Ok(())
}

/// Non-Windows stub: directory symlink / junction creation is Windows-only.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`]; callers gate this behind
/// `#[cfg(windows)]` and never reach the stub in practice.
#[cfg(not(windows))]
pub fn create_directory_symlink_or_junction(
    _target: &Path,
    _link: &Path,
) -> io::Result<WindowsDirLink> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory symlink/junction creation is Windows-only",
    ))
}

/// Non-Windows stub: the file-symlink helper is Windows-only.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
#[cfg(not(windows))]
pub fn create_file_symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file symlink helper is Windows-only",
    ))
}

/// Non-Windows stub: there is no unprivileged-symlink error class off Windows.
#[cfg(not(windows))]
pub fn is_unprivileged_symlink_error(_err: &io::Error) -> bool {
    false
}

/// Non-Windows stub: WSL symlinks exist only on Windows volumes; inside WSL
/// itself they are ordinary symlinks.
#[cfg(not(windows))]
pub fn is_wsl_symlink(_path: &Path) -> bool {
    false
}

/// Non-Windows stub: the WSL-symlink helper is Windows-only.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
#[cfg(not(windows))]
pub fn create_wsl_symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "WSL symlink helper is Windows-only",
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    /// The junction fallback yields a resolvable directory link regardless of
    /// privilege: an admin / Developer-Mode runner gets a symlink, a stock
    /// unprivileged runner gets a junction. Either way the link resolves to the
    /// target's contents.
    #[test]
    fn junction_fallback_creates_resolvable_directory_link() {
        let dir = test_support::create_tempdir();
        let target = dir.path().join("real_dir");
        fs::create_dir(&target).expect("create target");
        fs::write(target.join("inside.txt"), b"hello").expect("write inside");

        let link = dir.path().join("link_to_real");
        let kind = create_directory_symlink_or_junction(&target, &link)
            .expect("directory link creation must succeed via symlink or junction");

        // On Windows a reparse-tagged directory reports is_symlink() for both
        // junctions and symlinks.
        let meta = fs::symlink_metadata(&link).expect("symlink_metadata");
        assert!(
            meta.file_type().is_symlink(),
            "expected a reparse point, got {:?} (kind {kind:?})",
            meta.file_type()
        );

        let through = fs::read_to_string(link.join("inside.txt"))
            .expect("read target file through the directory link");
        assert_eq!(through, "hello");
    }

    /// A link written in WSL's form reads back through [`read_link`] with its
    /// POSIX target intact, although `std::fs::read_link` cannot read it.
    #[test]
    fn wsl_symlink_round_trips_through_read_link() {
        let dir = test_support::create_tempdir();
        let link = dir.path().join("python");
        let target = Path::new("/home/user/.local/share/uv/python/bin/python3.11");

        create_wsl_symlink(target, &link).expect("create WSL symlink");

        assert!(
            fs::symlink_metadata(&link)
                .expect("lstat")
                .file_type()
                .is_symlink()
        );
        assert!(is_wsl_symlink(&link));
        assert!(fs::read_link(&link).is_err(), "std cannot read WSL links");
        assert_eq!(read_link(&link).expect("read WSL link"), target);
    }

    /// The buffer matches what WSL writes: the tag, the data length, version 2
    /// and the UTF-8 target with no terminator.
    #[test]
    fn wsl_symlink_buffer_has_wsl_layout() {
        let buffer = wsl_symlink_buffer("python3.11").expect("buffer");
        let mut expected = vec![0x1D, 0x00, 0x00, 0xA0, 14, 0, 0, 0, 2, 0, 0, 0];
        expected.extend_from_slice(b"python3.11");
        assert_eq!(buffer, expected);
        assert_eq!(
            wsl_symlink_target(&buffer).as_deref(),
            Some(Path::new("python3.11"))
        );
    }

    /// Other reparse points are not WSL links, and a native symlink still
    /// reads through `std`.
    #[test]
    fn read_link_leaves_other_links_to_std() {
        let dir = test_support::create_tempdir();
        let target = dir.path().join("real_dir");
        fs::create_dir(&target).expect("create target");
        let link = dir.path().join("link");
        create_directory_symlink_or_junction(&target, &link).expect("directory link");

        assert!(!is_wsl_symlink(&link));
        assert!(!is_wsl_symlink(&target));
        let buffer = read_reparse_buffer(&link).expect("reparse buffer");
        assert_eq!(wsl_symlink_target(&buffer), None);
        assert!(read_link(&link).is_ok());
    }

    /// The file-symlink helper never panics: it either creates the link (when
    /// privileged) or reports the privilege refusal that callers translate into
    /// a skip-with-warning.
    #[test]
    fn file_symlink_reports_privilege_without_panicking() {
        let dir = test_support::create_tempdir();
        let target = dir.path().join("target.txt");
        fs::write(&target, b"payload").expect("write target");

        let link = dir.path().join("link.txt");
        match create_file_symlink(&target, &link) {
            Ok(()) => {
                let meta = fs::symlink_metadata(&link).expect("symlink_metadata");
                assert!(meta.file_type().is_symlink());
            }
            Err(err) => {
                assert!(
                    is_unprivileged_symlink_error(&err),
                    "unexpected file-symlink error: {err}"
                );
                assert!(!link.exists(), "no link should be left behind on refusal");
            }
        }
    }
}
