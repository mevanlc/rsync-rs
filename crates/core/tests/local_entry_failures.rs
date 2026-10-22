//! A local copy that meets failing entries goes on with the rest of the tree.
//!
//! upstream prints each failure as it happens (`rsyserr(FERROR_XFER, ...)`,
//! log.c:337-338), finishes the transfer, prints its listing and totals, and
//! only then names the exit code: `some files/attrs were not transferred (see
//! previous errors)` (log.c:98, `RERR_PARTIAL`). These tests pin that the run
//! finishes, that its summary survives the error it owes, and that a progress
//! preview meeting the same failures does not stop the copy before it starts.

mod test_timeout;

use std::fs;
use std::path::{Path, PathBuf};

use core::client::{
    ClientConfig, ClientError, ClientProgressObserver, ClientProgressUpdate,
    PARTIAL_TRANSFER_EXIT_CODE, run_client, run_client_with_observer,
};

use test_timeout::{LOCAL_TIMEOUT, run_with_timeout};

/// Keeps an entry unreadable to the copy while it lives.
struct Unreadable {
    #[cfg(windows)]
    _handle: fs::File,
    #[cfg(unix)]
    path: PathBuf,
}

/// Makes `path` - a file or a directory - unreadable, or returns `None` when
/// this process can read it regardless.
#[cfg(windows)]
fn unreadable(path: &Path) -> Option<Unreadable> {
    use std::os::windows::fs::OpenOptionsExt;
    // A handle that shares nothing refuses every other open for reading;
    // FILE_FLAG_BACKUP_SEMANTICS lets it hold a directory as well as a file.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .expect("hold the entry open");
    Some(Unreadable { _handle: handle })
}

#[cfg(unix)]
fn unreadable(path: &Path) -> Option<Unreadable> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    let guard = Unreadable {
        path: path.to_path_buf(),
    };
    // Root reads through any mode, so there is nothing to fail.
    let readable = if path.is_dir() {
        fs::read_dir(path).is_ok()
    } else {
        fs::File::open(path).is_ok()
    };
    (!readable).then_some(guard)
}

#[cfg(unix)]
impl Drop for Unreadable {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
    }
}

/// Builds a tree whose `locked.txt` cannot be read and whose `nolist/` cannot
/// be listed, returning the source and destination roots and the guards that
/// keep both unreadable. `None` when the process can read them anyway.
fn tree(root: &Path) -> Option<(PathBuf, PathBuf, [Unreadable; 2])> {
    let source = root.join("src");
    let destination = root.join("dst");
    fs::create_dir_all(source.join("sub")).expect("source sub");
    fs::create_dir_all(source.join("nolist")).expect("source nolist");
    fs::write(source.join("a.txt"), b"alpha").expect("write a");
    fs::write(source.join("locked.txt"), b"locked").expect("write locked");
    fs::write(source.join("sub").join("c.txt"), b"charlie").expect("write c");
    fs::write(source.join("nolist").join("e.txt"), b"echo").expect("write e");
    let file = unreadable(&source.join("locked.txt"))?;
    let directory = unreadable(&source.join("nolist"))?;
    Some((source, destination, [file, directory]))
}

fn config(source: &Path, destination: &Path) -> ClientConfig {
    let mut operand = source.as_os_str().to_owned();
    operand.push(std::path::MAIN_SEPARATOR_STR);
    ClientConfig::builder()
        .transfer_args([operand, destination.as_os_str().to_owned()])
        .recursive(true)
        .times(true)
        .build()
}

/// Asserts the run owes `RERR_PARTIAL` yet went on to its end: everything
/// readable was copied and the summary counts it.
fn assert_finished_partial(mut error: ClientError, destination: &Path) {
    assert_eq!(error.exit_code(), PARTIAL_TRANSFER_EXIT_CODE, "{error}");
    assert!(
        error
            .to_string()
            .contains("some files/attrs were not transferred (see previous errors)"),
        "the failures were printed as they happened, so only the exit code is left: {error}"
    );
    let summary = error
        .take_completed_summary()
        .expect("the run went on to its end");
    assert_eq!(
        summary.io_error_exit_code(),
        Some(PARTIAL_TRANSFER_EXIT_CODE)
    );
    assert_eq!(summary.files_copied(), 2, "a.txt and sub/c.txt");

    assert_eq!(fs::read(destination.join("a.txt")).expect("a"), b"alpha");
    assert_eq!(
        fs::read(destination.join("sub").join("c.txt")).expect("c"),
        b"charlie"
    );
    assert!(!destination.join("locked.txt").exists());
    // upstream sends a directory's own entry before it opens it, so the
    // unlistable directory is still created, empty.
    assert!(destination.join("nolist").is_dir());
    assert!(!destination.join("nolist").join("e.txt").exists());
}

#[test]
fn failed_entries_leave_the_rest_copied_and_the_summary_intact() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let Some((source, destination, _guards)) = tree(temp.path()) else {
            return;
        };
        let error = run_client(config(&source, &destination)).expect_err("failed entries");
        assert_finished_partial(error, &destination);
    });
}

struct Quiet;

impl ClientProgressObserver for Quiet {
    fn on_progress(&mut self, _update: &ClientProgressUpdate) {}
}

/// `--progress` previews a small tree before copying it. The preview meets the
/// unlistable directory too; it must leave the failure to the copy rather than
/// end the run before anything is copied.
#[test]
fn a_preview_meeting_the_failures_does_not_stop_the_copy() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let Some((source, destination, _guards)) = tree(temp.path()) else {
            return;
        };
        let mut observer = Quiet;
        let error = run_client_with_observer(config(&source, &destination), Some(&mut observer))
            .expect_err("failed entries");
        assert_finished_partial(error, &destination);
    });
}
