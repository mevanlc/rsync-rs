//! The progress line a message must not be written onto.
//!
//! upstream leaves every in-flight progress tick unended by a newline and sets
//! `output_needs_newline` (progress.c:121-123). `rwrite()` then writes `'\n'`
//! to the message's own stream before the message, and clears the flag
//! (log.c:455-458), so a diagnostic printed mid-transfer starts on its own line
//! and the next tick starts below it. The progress renderer and every
//! diagnostic printed straight to stderr share that flag through this module.
//!
//! Like the rest of this crate's state the flag is per thread: a local copy
//! drives its progress observer from the thread that prints its diagnostics.

use std::cell::Cell;
use std::fmt;
use std::io::{self, Write};

thread_local! {
    static OUTPUT_NEEDS_NEWLINE: Cell<bool> = const { Cell::new(false) };
}

/// Records whether a progress line is left open, unended by a newline.
pub fn set_output_needs_newline(needs: bool) {
    OUTPUT_NEEDS_NEWLINE.with(|flag| flag.set(needs));
}

/// Reports whether a progress line is left open, unended by a newline.
#[must_use]
pub fn output_needs_newline() -> bool {
    OUTPUT_NEEDS_NEWLINE.with(Cell::get)
}

/// Writes one diagnostic line to stderr, first ending a progress line left
/// open on the terminal. Use it through [`stderr_line!`](crate::stderr_line).
///
/// upstream: log.c:455-458 `rwrite()`
pub fn write_stderr_line(args: fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    if OUTPUT_NEEDS_NEWLINE.with(|flag| flag.replace(false)) {
        let _ = stderr.write_all(b"\n");
    }
    let _ = writeln!(stderr, "{args}");
}

#[cfg(test)]
mod tests {
    use super::{output_needs_newline, set_output_needs_newline, write_stderr_line};

    #[test]
    fn a_stderr_line_ends_the_open_progress_line() {
        set_output_needs_newline(true);
        write_stderr_line(format_args!("message"));
        assert!(!output_needs_newline());
    }

    #[test]
    fn the_flag_is_per_thread() {
        set_output_needs_newline(true);
        let elsewhere = std::thread::spawn(output_needs_newline)
            .join()
            .expect("thread");
        assert!(!elsewhere);
        assert!(output_needs_newline());
        set_output_needs_newline(false);
    }
}
