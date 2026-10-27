//! The binary's log: one line on stderr for each message, `target: text`
//! where the target is a short name (`vt: using tty /dev/tty2`, tier 3's
//! `terminal: render-timer: ...`), the text alone where it is a module
//! path. Under the units stderr reaches the journal.

use std::io::Write;

use log::{Level, Log, Metadata, Record};

struct Stderr;

static STDERR: Stderr = Stderr;

impl Log for Stderr {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let target = record.target();
        let mut err = std::io::stderr().lock();
        let _ = if target.contains("::") {
            writeln!(err, "{}", record.args())
        } else {
            writeln!(err, "{target}: {}", record.args())
        };
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Routes `log`'s messages at `level` and above to stderr. A second call
/// changes nothing.
pub fn init(level: Level) {
    if log::set_logger(&STDERR).is_ok() {
        log::set_max_level(level.to_level_filter());
    }
}
