//! Progress reporting for local copies whose file list is not built up front.
//!
//! upstream lists a tree that fits the INC_RECURSE sender's lookahead
//! (rsync.h:151 `MIN_FILECNT_LOOKAHEAD`) before the first transfer, so its
//! progress counts against the full total (`to-chk`). A larger tree is checked
//! against a total that grows as sub-lists arrive (`ir-chk`), and the copy
//! starts reporting at once instead of after a walk of the whole tree. These
//! tests pin both shapes and the `--no-inc-recursive` fallback, which upstream
//! always lists whole.

mod test_timeout;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use core::client::{
    ClientConfig, ClientEvent, ClientProgressObserver, ClientProgressUpdate,
    run_client_with_observer,
};

use test_timeout::{LOCAL_TIMEOUT, run_with_timeout};

/// Directories in a tree that outgrows the preview.
const DIRECTORIES: usize = 12;
/// Files in each of those directories.
const FILES_PER_DIRECTORY: usize = 100;
/// Every entry of that tree: the transfer root, its directories and files.
const ENTRIES: usize = 1 + DIRECTORIES + DIRECTORIES * FILES_PER_DIRECTORY;

#[derive(Debug)]
enum Seen {
    Start,
    Entry(PathBuf),
    Progress(Tick),
}

#[derive(Debug)]
struct Tick {
    path: PathBuf,
    is_final: bool,
    transfer_complete: bool,
    flist_eof: bool,
    total: usize,
    remaining: usize,
    overall_total_bytes: Option<u64>,
}

#[derive(Default)]
struct OrderObserver {
    seen: Vec<Seen>,
}

impl OrderObserver {
    fn ticks(&self) -> impl Iterator<Item = &Tick> {
        self.seen.iter().filter_map(|seen| match seen {
            Seen::Progress(tick) => Some(tick),
            _ => None,
        })
    }

    /// The final tick of each transferred file, in transfer order.
    fn transfer_ticks(&self) -> Vec<&Tick> {
        self.ticks()
            .filter(|tick| tick.is_final && !tick.transfer_complete)
            .collect()
    }
}

impl ClientProgressObserver for OrderObserver {
    fn on_progress(&mut self, update: &ClientProgressUpdate) {
        self.seen.push(Seen::Progress(Tick {
            path: update.event().relative_path().to_path_buf(),
            is_final: update.is_final(),
            transfer_complete: update.is_transfer_complete(),
            flist_eof: update.flist_eof(),
            total: update.total(),
            remaining: update.remaining(),
            overall_total_bytes: update.overall_total_bytes(),
        }));
    }

    fn on_start(&mut self, _created_destination_root: Option<&Path>) {
        self.seen.push(Seen::Start);
    }

    fn on_entry(&mut self, event: &ClientEvent) {
        self.seen
            .push(Seen::Entry(event.relative_path().to_path_buf()));
    }
}

fn touch(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, contents).expect("write fixture file");
}

/// Builds `directories` directories of `files` small files each under `root`.
fn build_tree(root: &Path, directories: usize, files: usize) {
    for directory in 0..directories {
        for file in 0..files {
            let path = root.join(format!("d{directory:02}/f{file:03}.txt"));
            touch(&path, format!("payload {directory} {file}").as_bytes());
        }
    }
}

/// The source operand with a trailing separator, so its contents copy.
fn contents_of(root: &Path) -> OsString {
    let mut arg = root.as_os_str().to_owned();
    arg.push(std::path::MAIN_SEPARATOR.to_string());
    arg
}

fn copy_config(source: &Path, destination: &Path, incremental: bool) -> ClientConfig {
    ClientConfig::builder()
        .transfer_args([contents_of(source), destination.as_os_str().to_owned()])
        .recursive(true)
        .inc_recursive_send(incremental)
        .progress(true)
        .build()
}

#[test]
fn tree_past_preview_limit_streams_with_growing_total() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("src");
        let destination = temp.path().join("dst");
        build_tree(&source, DIRECTORIES, FILES_PER_DIRECTORY);

        let mut observer = OrderObserver::default();
        let summary = run_client_with_observer(
            copy_config(&source, &destination, true),
            Some(&mut observer),
        )
        .expect("run client");
        assert_eq!(
            summary.files_copied(),
            (DIRECTORIES * FILES_PER_DIRECTORY) as u64
        );

        assert!(matches!(observer.seen.first(), Some(Seen::Start)));
        assert_eq!(
            observer
                .seen
                .iter()
                .filter(|seen| matches!(seen, Seen::Start))
                .count(),
            1
        );

        let transfers = observer.transfer_ticks();
        assert_eq!(transfers.len(), DIRECTORIES * FILES_PER_DIRECTORY);
        assert!(
            transfers.iter().all(|tick| !tick.flist_eof),
            "every transfer is checked against a list still growing (ir-chk)"
        );
        assert!(
            transfers[0].total < ENTRIES,
            "the first transfer sees only the directories read so far, not {}",
            transfers[0].total
        );

        let mut previous_total = 0;
        for tick in observer.ticks() {
            assert!(tick.total >= previous_total, "the total never shrinks");
            assert!(tick.remaining <= tick.total);
            previous_total = tick.total;
        }

        let last = observer.ticks().last().expect("a final summary tick");
        assert!(last.transfer_complete);
        assert!(last.flist_eof, "the final tick reports the completed list");
        assert_eq!(last.remaining, 0);
        assert_eq!(last.total, ENTRIES);
        assert_eq!(last.overall_total_bytes, Some(summary.total_source_bytes()));
    });
}

#[test]
fn streamed_transfer_is_named_before_its_progress() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("src");
        let destination = temp.path().join("dst");
        build_tree(&source, DIRECTORIES, FILES_PER_DIRECTORY);
        // Sorts last in the last directory, well past the preview window.
        let big = PathBuf::from(format!("d{:02}/zz-big.bin", DIRECTORIES - 1));
        touch(&source.join(&big), &vec![0x5a; 4 * 1024 * 1024]);

        // --inplace keeps the copy on the chunked loop, which reports progress
        // while the data moves rather than only once it has landed.
        let config = ClientConfig::builder()
            .transfer_args([contents_of(&source), destination.as_os_str().to_owned()])
            .recursive(true)
            .inplace(true)
            .progress(true)
            .build();
        let mut observer = OrderObserver::default();
        run_client_with_observer(config, Some(&mut observer)).expect("run client");

        let entry = observer
            .seen
            .iter()
            .position(|seen| matches!(seen, Seen::Entry(path) if *path == big))
            .expect("the big file is named");
        let first_tick = observer
            .seen
            .iter()
            .position(|seen| matches!(seen, Seen::Progress(tick) if tick.path == big))
            .expect("the big file reports progress");
        assert!(entry < first_tick, "named before its first progress tick");
        assert_eq!(
            observer
                .seen
                .iter()
                .filter(|seen| matches!(seen, Seen::Entry(path) if *path == big))
                .count(),
            1,
            "named once, not again when it completes"
        );
        let big_ticks: Vec<_> = observer.ticks().filter(|tick| tick.path == big).collect();
        assert!(big_ticks.iter().any(|tick| !tick.is_final));
        assert!(big_ticks.iter().all(|tick| !tick.flist_eof));
    });
}

#[test]
fn tree_within_preview_limit_reports_full_total() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("src");
        let destination = temp.path().join("dst");
        build_tree(&source, 3, 5);

        let mut observer = OrderObserver::default();
        run_client_with_observer(
            copy_config(&source, &destination, true),
            Some(&mut observer),
        )
        .expect("run client");

        let transfers = observer.transfer_ticks();
        assert_eq!(transfers.len(), 15);
        for tick in transfers {
            assert!(tick.flist_eof, "a previewed list is complete (to-chk)");
            assert_eq!(tick.total, 1 + 3 + 15);
        }
    });
}

#[test]
fn no_inc_recursive_previews_whole_tree() {
    run_with_timeout(LOCAL_TIMEOUT, || {
        let temp = test_support::create_tempdir();
        let source = temp.path().join("src");
        let destination = temp.path().join("dst");
        build_tree(&source, DIRECTORIES, FILES_PER_DIRECTORY);

        let mut observer = OrderObserver::default();
        run_client_with_observer(
            copy_config(&source, &destination, false),
            Some(&mut observer),
        )
        .expect("run client");

        let transfers = observer.transfer_ticks();
        assert_eq!(transfers.len(), DIRECTORIES * FILES_PER_DIRECTORY);
        for tick in transfers {
            assert!(
                tick.flist_eof,
                "without INC_RECURSE the list is built whole"
            );
            assert_eq!(tick.total, ENTRIES);
        }
    });
}
