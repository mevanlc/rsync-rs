//! Local copies of read-only files.
//!
//! upstream sets a file's times with `utimensat` and replaces it with
//! `rename(2)`, neither of which looks at the file's own mode, so a read-only
//! file - every git pack file is one - copies and updates like any other. On
//! Windows the read-only attribute refuses a handle opened for write and a
//! rename onto the file, so both steps have to step around it.

mod test_timeout;

use std::fs;
use std::path::Path;

use core::client::{ClientConfig, run_client};
use filetime::FileTime;

use test_timeout::{LOCAL_TIMEOUT, run_with_timeout};

fn make_read_only(path: &Path) {
    let mut permissions = fs::metadata(path).expect("stat").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).expect("make read-only");
}

/// Rewrites a read-only file, leaving it read-only with the given mtime.
#[allow(clippy::permissions_set_readonly_false)]
fn rewrite_read_only(path: &Path, contents: &[u8], mtime: FileTime) {
    let mut permissions = fs::metadata(path).expect("stat").permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).expect("make writable");
    fs::write(path, contents).expect("rewrite");
    filetime::set_file_mtime(path, mtime).expect("set mtime");
    make_read_only(path);
}

fn archive_config(source: &Path, destination: &Path) -> ClientConfig {
    ClientConfig::builder()
        .transfer_args([source.as_os_str(), destination.as_os_str()])
        .recursive(true)
        .times(true)
        .permissions(true)
        .build()
}

#[test]
fn read_only_file_is_copied_and_updated() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("pack");
        let destination = temp.path().join("dst");
        fs::create_dir_all(&source).expect("source dir");
        let idx = source.join("pack-0001.idx");
        fs::write(&idx, b"first").expect("write idx");
        let first_mtime = FileTime::from_unix_time(1_600_000_000, 0);
        filetime::set_file_mtime(&idx, first_mtime).expect("set mtime");
        make_read_only(&idx);

        run_client(archive_config(&source, &destination)).expect("first copy");
        let copied = destination.join("pack").join("pack-0001.idx");
        let meta = fs::metadata(&copied).expect("copied idx");
        assert_eq!(fs::read(&copied).expect("read"), b"first");
        assert_eq!(FileTime::from_last_modification_time(&meta), first_mtime);
        assert!(meta.permissions().readonly());

        let second_mtime = FileTime::from_unix_time(1_700_000_000, 0);
        rewrite_read_only(&idx, b"second version", second_mtime);
        run_client(archive_config(&source, &destination)).expect("update");
        let meta = fs::metadata(&copied).expect("updated idx");
        assert_eq!(fs::read(&copied).expect("read"), b"second version");
        assert_eq!(FileTime::from_last_modification_time(&meta), second_mtime);
        assert!(meta.permissions().readonly());
    });
}
