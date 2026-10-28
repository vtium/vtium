//! Rebooting, powering off and suspending the machine: the requests the
//! power keys make of PID 1, logind and the kernel. Each needs root, which
//! vtium holds for its life (ADR-1400).

use std::cell::Cell;
use std::io;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction};
use rustix::process::{Pid, PidfdFlags, Signal, kill_process, pidfd_open};

/// Sends PID 1 `SIGINT`, which init takes as Ctrl+Alt+Del: systemd starts
/// `ctrl-alt-del.target`, a reboot, and sysvinit runs its `ctrlaltdel`
/// entry.
pub fn interrupt_init() -> io::Result<()> {
    kill_process(Pid::INIT, Signal::INT)?;
    Ok(())
}

/// Sends PID 1 `SIGRTMIN+4`, which systemd takes as a request to power
/// off: it starts `poweroff.target`, stopping the services and unmounting
/// the filesystems first. `SIGRTMIN` is libc's, as systemd numbers it: 34
/// under glibc, which keeps the kernel's 32 and 33 for its threads.
pub fn power_off_init() -> io::Result<()> {
    let signal = power_off_signal();
    #[expect(
        unsafe_code,
        reason = "rustix sends no real-time signal without an unsafe constructor whose contract forbids sending it"
    )]
    // SAFETY: kill(2) takes any pid and signal number and touches no
    // memory of this process; an invalid one is an error it returns.
    let sent = unsafe { libc::kill(1, signal) };
    if sent == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// systemd's power-off signal, `SIGRTMIN+4`.
fn power_off_signal() -> libc::c_int {
    libc::SIGRTMIN().saturating_add(4)
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
    /// VT's release when the kernel switches to its suspend console.
    /// [`watch`] reaps it.
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
    let child = spawn(shell("echo mem > /sys/power/state").stdin(Stdio::null()))?;
    Ok(Suspend::Sysfs(child))
}

/// Starts `command` holding no descriptor of vtium's but its stdio:
/// every descriptor vtium inherited is marked close-on-exec first.
fn spawn(command: &mut Command) -> io::Result<Child> {
    crate::fds::cloexec_above_stdio()?;
    command.spawn()
}

/// `/bin/sh` running `script` with none of vtium's environment, the
/// session's included, and a fixed `PATH`.
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", script])
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    command
}

/// Reaps the sysfs suspend's `child` from the loop, through its pidfd, as
/// the pty's child is reaped: at the wake-up its exit is logged, and an
/// exit other than 0, a write the kernel refused, is an error. `pending`
/// is true until then.
pub fn watch<D: 'static>(
    handle: &LoopHandle<'static, D>,
    child: Child,
    pending: Rc<Cell<bool>>,
) -> io::Result<()> {
    let pidfd = pidfd_open(Pid::from_child(&child), PidfdFlags::empty())?;
    let mut child = Some(child);
    pending.set(true);
    handle
        .insert_source(
            Generic::new(pidfd, Interest::READ, Mode::Level),
            move |_, _, _: &mut D| {
                let Some(status) = child.as_mut().map(Child::try_wait).transpose()?.flatten()
                else {
                    return Ok(PostAction::Continue);
                };
                child = None;
                pending.set(false);
                if status.success() {
                    log::info!(target: "power", "resumed: the write to /sys/power/state returned");
                } else {
                    log::error!(target: "power", "the write to /sys/power/state failed: {status}");
                }
                Ok(PostAction::Remove)
            },
        )
        .map_err(|e| io::Error::from(e.error))?;
    Ok(())
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[cfg(target_env = "gnu")]
    #[test]
    fn power_off_is_signal_38_under_glibc() {
        assert_eq!(power_off_signal(), 38);
    }

    #[test]
    fn the_sysfs_shell_inherits_no_environment() {
        // cargo runs the test with HOME and CARGO_PKG_NAME set; the shell
        // sets neither itself.
        let out = shell(r#"printf '%s|%s|%s' "$HOME" "$CARGO_PKG_NAME" "$PATH""#)
            .output()
            .expect("a shell");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "||/usr/bin:/bin");
    }

    #[test]
    fn the_sysfs_shell_holds_only_its_stdio() {
        let _one = crate::fds::tests::one_at_a_time();
        // a descriptor without close-on-exec, as one vtium's parent passes
        // down may be, numbered high so it is never the one ls opens
        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        let held = rustix::io::fcntl_dupfd_cloexec(&null, 100).expect("dup");
        rustix::io::fcntl_setfd(&held, rustix::io::FdFlags::empty()).expect("setfd");
        let mut command = shell("ls /proc/self/fd");
        command.stdin(Stdio::null()).stdout(Stdio::piped());
        let out = spawn(&mut command)
            .expect("a shell")
            .wait_with_output()
            .expect("its output");
        let listed = String::from_utf8_lossy(&out.stdout);
        let fds: Vec<&str> = listed.lines().collect();
        let held = std::os::fd::AsRawFd::as_raw_fd(&held).to_string();
        // only the planted descriptor is looked for: other tests' threads
        // open their own meanwhile, xkbcommon's keymap files without
        // close-on-exec among them
        assert!(
            ["0", "1", "2"].iter().all(|f| fds.contains(f)) && !fds.contains(&held.as_str()),
            "{fds:?}"
        );
    }

    /// The child is reaped from the loop at its exit, whatever its status,
    /// and the suspend is pending until then.
    #[test]
    fn the_sysfs_child_is_reaped_from_the_loop() {
        for code in ["0", "3"] {
            let mut lp: calloop::EventLoop<'static, ()> =
                calloop::EventLoop::try_new().expect("a loop");
            // The shell exits once its stdin closes, as the write returns
            // at the wake-up.
            let mut child = Command::new("/bin/sh")
                .args(["-c", &format!("read x; exit {code}")])
                .stdin(Stdio::piped())
                .spawn()
                .expect("a shell");
            let pid = child.id();
            let stdin = child.stdin.take();
            let pending = Rc::new(Cell::new(false));
            watch(&lp.handle(), child, Rc::clone(&pending)).expect("watched");
            assert!(pending.get(), "pending while the child runs");
            lp.dispatch(Some(std::time::Duration::ZERO), &mut ())
                .expect("dispatch");
            assert!(pending.get(), "still pending");
            drop(stdin);
            for _ in 0..200 {
                lp.dispatch(Some(std::time::Duration::from_millis(10)), &mut ())
                    .expect("dispatch");
                if !pending.get() {
                    break;
                }
            }
            assert!(!pending.get(), "exit {code} reaped");
            // Reaped: the pid is no child of ours any more.
            assert!(
                rustix::process::waitpid(
                    rustix::process::Pid::from_raw(i32::try_from(pid).expect("a pid")),
                    rustix::process::WaitOptions::NOHANG
                )
                .is_err()
            );
        }
    }
}
