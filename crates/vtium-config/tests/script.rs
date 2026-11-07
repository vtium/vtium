//! The helper that writes or copies the tests' stand-in programs
//! (`common/script.rs`): a program it writes or copies runs at once,
//! however busy the test's other threads are starting processes.

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
    every_program_runs("script", |program| {
        script::executable(program, "#!/bin/sh\nexit 0\n");
    });
}

/// The same for 300 copies of a program, each run as soon as it is
/// copied. The program is a script of 2 MB, the size of a small test
/// binary, so each copy holds its file open for as long as copying a real
/// program does.
#[test]
fn a_program_it_copies_runs_while_other_threads_start_processes() {
    let original = Path::new(env!("CARGO_TARGET_TMPDIR")).join("script-original");
    let mut text = String::from("#!/bin/sh\nexit 0\n");
    while text.len() < 2 << 20 {
        text.push_str("# a comment the shell never reads, to give the program its size\n");
    }
    script::executable(&original, &text);
    every_program_runs("script-copy", |program| {
        script::executable_copy(&original, program);
    });
}

/// Starts four threads that start `true` without pause, makes 300
/// programs in the directory `name` under the build's own temporary
/// directory with `make`, runs each as soon as it is made and then removes
/// it, and fails unless every one ran and ended with status 0.
fn every_program_runs(name: &str, make: impl Fn(&Path)) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
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
            make(&program);
            match Command::new(&program).status() {
                Ok(status) if status.success() => {}
                Ok(status) => failed.push(format!("{}: {status}", program.display())),
                Err(e) => failed.push(format!("{}: {e}", program.display())),
            }
            std::fs::remove_file(&program).expect("the program removed");
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
