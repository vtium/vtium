//! Rebooting, powering off and suspending the machine: the requests the
//! power keys make of PID 1, logind and the kernel. Each needs root, which
//! vtium holds for its life (ADR-1400).

use std::io;
use std::process::{Child, Command, Stdio};

use rustix::process::{Pid, Signal, kill_process};

/// glibc's `SIGRTMIN`, from which systemd numbers its real-time signals:
/// the kernel's first real-time signal is 32, and glibc keeps 32 and 33
/// for its threads.
const SIGRTMIN: i32 = 34;

/// Sends PID 1 `SIGINT`, which init takes as Ctrl+Alt+Del: systemd starts
/// `ctrl-alt-del.target`, a reboot, and sysvinit runs its `ctrlaltdel`
/// entry.
pub fn interrupt_init() -> io::Result<()> {
    kill_process(Pid::INIT, Signal::INT)?;
    Ok(())
}

/// Sends PID 1 `SIGRTMIN+4`, which systemd takes as a request to power
/// off: it starts `poweroff.target`, stopping the services and unmounting
/// the filesystems first.
pub fn power_off_init() -> io::Result<()> {
    #[expect(
        unsafe_code,
        reason = "rustix has no safe constructor for a real-time signal"
    )]
    // SAFETY: 38 is a valid, non-zero signal on Linux, whose signals run
    // from 1 to 64 with the real-time ones from 32. It is sent to PID 1
    // only, and is never consumed, blocked or handled in this process,
    // which is what rustix's contract for a libc real-time signal forbids.
    let signal = unsafe { Signal::from_raw_unchecked(SIGRTMIN.wrapping_add(4)) };
    kill_process(Pid::INIT, signal)?;
    Ok(())
}

/// Writes the filesystems' dirty data out with `sync()`, then restarts the
/// kernel at once with `reboot(RB_AUTOBOOT)`. On success it does not
/// return.
pub fn restart_kernel() -> io::Result<()> {
    rustix::fs::sync();
    rustix::system::reboot(rustix::system::RebootCommand::Restart)?;
    Ok(())
}

/// How [`suspend`] asked for the sleep.
#[derive(Debug)]
pub enum Suspend {
    /// logind took the request, and suspends once its inhibitors let go.
    Logind,
    /// logind is not running, or not built in: a shell, the child given
    /// here, writes `mem` to `/sys/power/state` and exits at the wake-up.
    /// The write is another process's, so the loop goes on serving the
    /// VT's release when the kernel switches to its suspend console; the
    /// caller reaps the child.
    Sysfs(Child),
}

/// Suspends the machine to RAM: through logind's `Suspend` when vtium is
/// built with `logind` and logind is on the system bus, else by writing
/// `mem` to `/sys/power/state`. A refusal from logind, such as an
/// inhibitor's, is returned and never worked around.
pub fn suspend() -> io::Result<Suspend> {
    #[cfg(feature = "logind")]
    match crate::logind::suspend() {
        Ok(()) => return Ok(Suspend::Logind),
        Err(e @ (crate::logind::Error::NoBus(_) | crate::logind::Error::NotRunning(_))) => {
            log::info!(target: "power", "logind cannot suspend: {e}");
        }
        Err(e) => return Err(io::Error::other(e.to_string())),
    }
    let child = Command::new("/bin/sh")
        .args(["-c", "echo mem > /sys/power/state"])
        .stdin(Stdio::null())
        .spawn()?;
    Ok(Suspend::Sysfs(child))
}
