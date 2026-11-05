//! The helper that writes the tests' stand-in programs
//! (`common/script.rs`): a program it writes runs at once, however busy
//! the test's other threads are starting processes.

#[path = "common/script.rs"]
mod script;

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// Four threads start `true` without pause while this thread writes 300
/// programs and runs each one as soon as it is written. Had this process
/// held a program open for writing while one of those threads forked, the
/// forked child would have kept the program busy and the kernel would have
/// refused to run it with ETXTBSY. Every program runs.
#[test]
fn a_program_it_writes_runs_while_other_threads_start_processes() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("script");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("a fresh directory");
    }
    std::fs::create_dir_all(&dir).expect("the directory");
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    let started = Command::new("true").status().expect("true starts");
                    assert!(started.success(), "{started}");
                }
            });
        }
        // The threads stop when this thread is done, whether it passes or
        // panics, so a failure never leaves the scope waiting on them.
        let _stop = Stop(&done);
        let mut failed = Vec::new();
        for n in 0..300 {
            let program = dir.join(format!("program-{n}"));
            script::executable(&program, "#!/bin/sh\nexit 0\n");
            match Command::new(&program).status() {
                Ok(status) if status.success() => {}
                Ok(status) => failed.push(format!("{}: {status}", program.display())),
                Err(e) => failed.push(format!("{}: {e}", program.display())),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
    });
}

/// Tells the threads that start `true` to stop when it is dropped.
struct Stop<'a>(&'a AtomicBool);

impl Drop for Stop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
