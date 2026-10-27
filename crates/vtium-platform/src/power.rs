//! Rebooting and powering off the machine: the requests the power keys
//! make of PID 1 and the kernel. Each needs root, which vtium holds for
//! its life (ADR-1400).

use std::io;

use rustix::process::{Pid, Signal, kill_process};

/// glibc's `SIGRTMIN`, from which systemd numbers its real-time signals.
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
    // SAFETY: 38 is a valid, non-zero signal number. The signal is sent
    // to another process only, and is never consumed, blocked or handled
    // here, which is what libc reserves its real-time signals for.
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
