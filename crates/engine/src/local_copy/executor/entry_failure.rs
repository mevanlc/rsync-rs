//! Upstream-format reporting for an entry whose transfer failed.
//!
//! The run prints the failure where it happens and goes on with the next
//! entry. The first failure is kept so the run still finishes `RERR_PARTIAL`
//! (23) or `RERR_VANISHED` (24) once every entry has been tried, and it is
//! marked as printed so the directory frames it unwinds through, and the
//! closing exit-code line, do not print it again.
//!
//! # Upstream Reference
//!
//! - `log.c:337-338` - `rsyserr(FERROR_XFER, ...)` prints the failure and sets
//!   `got_xfer_error`
//! - `flist.c:1688`, `sender.c:715` - `file has vanished: %s`
//! - `rsync.c:784`, `rsync.c:814-816` - `failed to set times on %s`,
//!   `failed to set permissions on %s`
//! - `cleanup.c:213-218` - `IOERR_GENERAL` (23) outranks `IOERR_VANISHED` (24)

use std::path::Path;

use super::create_failure::full_fname;
use crate::local_copy::{CopyContext, LocalCopyError, upstream_io_error};

/// Prints `error` for the entry `name` unless a deeper frame already did, and
/// returns it marked as printed.
///
/// A vanished file is named by its entry; any other failure names the path
/// the failing operation touched. A preview pass marks the error without
/// printing it, leaving the line to the copy that repeats the pass.
pub(crate) fn report_entry_failure(
    context: &CopyContext,
    error: LocalCopyError,
    name: &Path,
) -> LocalCopyError {
    if error.is_reported() || !context.reports_entry_failures() {
        return error.into_reported();
    }
    if error.is_vanished_error() {
        // full_fname() wraps the path in double quotes (util1.c:1528).
        logging::stderr_line!("file has vanished: \"{}\"", name.display());
    } else {
        logging::stderr_line!("{}", failure_line(&error, context.destination_root()));
    }
    error.into_reported()
}

/// Keeps the failure a run that went on past failed entries finishes with:
/// the first general failure, or the first vanished file when nothing worse
/// happened.
pub(crate) fn retain_entry_failure(kept: &mut Option<LocalCopyError>, error: LocalCopyError) {
    match kept {
        Some(first) if !first.is_vanished_error() || error.is_vanished_error() => {}
        _ => *kept = Some(error),
    }
}

/// Renders a failure as `rsyserr()` does:
/// `rsync: [<role>] <what>: <strerror> (<errno>)`.
fn failure_line(error: &LocalCopyError, destination_root: &Path) -> String {
    let Some((action, path, source)) = error.kind().as_io() else {
        return format!("rsync: [sender] {error}");
    };
    // The receiver writes the destination; any other path is a source the
    // sender reads.
    let role = if path.starts_with(destination_root) {
        "receiver"
    } else {
        "sender"
    };
    let name = full_fname(path);
    let what = match action {
        "preserve timestamps" | "preserve access time" => format!("failed to set times on {name}"),
        "preserve permissions" => format!("failed to set permissions on {name}"),
        _ => format!("failed to {action} {name}"),
    };
    format!("rsync: [{role}] {what}: {}", upstream_io_error(source))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::Path;

    use super::{failure_line, retain_entry_failure};
    use crate::local_copy::LocalCopyError;

    fn general(path: &str) -> LocalCopyError {
        LocalCopyError::io(
            "copy file",
            path,
            io::Error::from(io::ErrorKind::PermissionDenied),
        )
    }

    fn vanished(path: &str) -> LocalCopyError {
        LocalCopyError::io("copy file", path, io::Error::from(io::ErrorKind::NotFound))
    }

    fn kept_path(kept: &Option<LocalCopyError>) -> &Path {
        kept.as_ref()
            .and_then(|error| error.kind().as_io())
            .map(|(_, path, _)| path)
            .expect("an io error is kept")
    }

    #[test]
    fn first_general_failure_is_kept() {
        let mut kept = None;
        retain_entry_failure(&mut kept, general("/src/a"));
        retain_entry_failure(&mut kept, general("/src/b"));
        retain_entry_failure(&mut kept, vanished("/src/c"));
        assert_eq!(kept_path(&kept), Path::new("/src/a"));
    }

    #[test]
    fn general_failure_outranks_an_earlier_vanished_file() {
        let mut kept = None;
        retain_entry_failure(&mut kept, vanished("/src/a"));
        retain_entry_failure(&mut kept, vanished("/src/b"));
        assert_eq!(kept_path(&kept), Path::new("/src/a"));
        retain_entry_failure(&mut kept, general("/src/c"));
        assert_eq!(kept_path(&kept), Path::new("/src/c"));
    }

    #[test]
    fn failure_line_uses_rsyserr_shape_and_role() {
        let denied =
            |action, path: &str| LocalCopyError::io(action, path, io::Error::other("denied"));
        let root = Path::new("/dst");
        assert_eq!(
            failure_line(&denied("copy file", "/src/a"), root),
            "rsync: [sender] failed to copy file \"/src/a\": denied"
        );
        assert_eq!(
            failure_line(&denied("preserve timestamps", "/dst/a"), root),
            "rsync: [receiver] failed to set times on \"/dst/a\": denied"
        );
        assert_eq!(
            failure_line(&denied("preserve permissions", "/dst/a"), root),
            "rsync: [receiver] failed to set permissions on \"/dst/a\": denied"
        );
    }
}
