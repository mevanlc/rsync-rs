//! Local copies of symbolic links WSL created on an NTFS volume.
//!
//! WSL stores a link as an `IO_REPARSE_TAG_LX_SYMLINK` reparse point holding
//! its POSIX target, which `std::fs::read_link` refuses. upstream rsync on
//! Windows runs under Cygwin, which reads these as ordinary symlinks, so a
//! `-l` copy lists and recreates them like any other link. The copy keeps
//! WSL's form, since the target is a POSIX path only WSL resolves.
#![cfg(windows)]

mod test_timeout;

use std::fs;
use std::path::Path;

use core::client::{ClientConfig, run_client};

use test_timeout::{LOCAL_TIMEOUT, run_with_timeout};

fn config(source: &Path, destination: &Path, links: bool) -> ClientConfig {
    let mut operand = source.as_os_str().to_owned();
    operand.push(std::path::MAIN_SEPARATOR_STR);
    ClientConfig::builder()
        .transfer_args([operand, destination.as_os_str().to_owned()])
        .recursive(true)
        .links(links)
        .build()
}

#[test]
fn wsl_symlinks_are_copied_as_wsl_symlinks() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("bin");
        let destination = temp.path().join("dst");
        fs::create_dir_all(&source).expect("source");
        fs::write(source.join("activate"), b"# venv").expect("write activate");
        let absolute = Path::new("/home/user/.local/share/uv/python/bin/python3.11");
        fast_io::create_wsl_symlink(absolute, &source.join("python")).expect("python link");
        fast_io::create_wsl_symlink(Path::new("python"), &source.join("python3"))
            .expect("python3 link");

        let summary = run_client(config(&source, &destination, true)).expect("copy");
        assert_eq!(summary.symlinks_copied(), 2);
        for (name, target) in [("python", absolute), ("python3", Path::new("python"))] {
            let link = destination.join(name);
            assert!(fast_io::is_wsl_symlink(&link), "{name} keeps WSL's form");
            assert_eq!(fast_io::read_link(&link).expect("read copy"), target);
        }

        let summary = run_client(config(&source, &destination, true)).expect("re-run");
        assert_eq!(
            summary.symlinks_copied(),
            0,
            "an unchanged link is up to date"
        );
    });
}

/// Without `-l` a WSL symlink is skipped as a non-regular file, as upstream
/// skips any symlink, rather than failing to be read.
#[test]
fn wsl_symlinks_are_skipped_without_links() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("bin");
        let destination = temp.path().join("dst");
        fs::create_dir_all(&source).expect("source");
        fs::write(source.join("activate"), b"# venv").expect("write activate");
        fast_io::create_wsl_symlink(Path::new("python3.11"), &source.join("python"))
            .expect("python link");

        run_client(config(&source, &destination, false)).expect("copy");
        assert!(destination.join("activate").exists());
        assert!(fs::symlink_metadata(destination.join("python")).is_err());
    });
}
