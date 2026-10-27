//! The power keys: `grab-reboot`, `grab-power-off` and `grab-suspend`, the
//! seat's keys, second in the key order after the VT's. Each is unbound
//! until the configuration binds a [`crate::grab::Chord`] to it, and acts
//! once a press through [`Power`]: the machine's ([`System`]), or a
//! test's stand-in.

use std::io;
use std::process::Child;

use vtium_platform::power::Suspend;

/// A power key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerKey {
    /// `grab-reboot`.
    Reboot,
    /// `grab-power-off`.
    PowerOff,
    /// `grab-suspend`.
    Suspend,
}

/// What the power keys act through: PID 1, logind and the kernel.
pub trait Power {
    /// `/proc/sys/kernel/ctrl-alt-del`'s text.
    fn ctrl_alt_del(&mut self) -> io::Result<String>;
    /// `SIGINT` to PID 1, which init takes as Ctrl+Alt+Del: systemd starts
    /// `ctrl-alt-del.target`, a reboot.
    fn interrupt_init(&mut self) -> io::Result<()>;
    /// `SIGRTMIN+4` to PID 1: systemd starts `poweroff.target`, which
    /// stops the services and unmounts the filesystems first.
    fn power_off_init(&mut self) -> io::Result<()>;
    /// `sync()`, then `reboot(RB_AUTOBOOT)`: the kernel restarts at once.
    fn restart_kernel(&mut self) -> io::Result<()>;
    /// Suspend to RAM: logind's `Suspend` when built in and running, else
    /// `mem` written to `/sys/power/state` by the child given back, which
    /// the caller reaps.
    fn suspend(&mut self) -> io::Result<Option<Child>>;
}

/// The machine vtium runs on.
pub struct System;

impl Power for System {
    fn ctrl_alt_del(&mut self) -> io::Result<String> {
        std::fs::read_to_string("/proc/sys/kernel/ctrl-alt-del")
    }
    fn interrupt_init(&mut self) -> io::Result<()> {
        vtium_platform::power::interrupt_init()
    }
    fn power_off_init(&mut self) -> io::Result<()> {
        vtium_platform::power::power_off_init()
    }
    fn restart_kernel(&mut self) -> io::Result<()> {
        vtium_platform::power::restart_kernel()
    }
    fn suspend(&mut self) -> io::Result<Option<Child>> {
        Ok(match vtium_platform::power::suspend()? {
            Suspend::Logind => {
                log::warn!("grab-suspend: logind took the request");
                None
            }
            Suspend::Sysfs(child) => {
                log::warn!("grab-suspend: writing mem to /sys/power/state");
                Some(child)
            }
        })
    }
}

/// Acts on a power key's press through `power`, and logs what it did; the
/// child writing `/sys/power/state`, when a suspend starts one. While
/// `sleeping`, the last such child has not exited, and a suspend does
/// nothing more.
///
/// The reboot takes the C reference's two paths (`src/seat.c:479-513`),
/// chosen by `/proc/sys/kernel/ctrl-alt-del`, which the kernel reads for
/// its own Ctrl+Alt+Del: at 0, or when it cannot be read, PID 1 is asked
/// to reboot; at any other number the kernel restarts at once. Power-off
/// asks PID 1, and suspend asks logind, or the kernel where logind is not
/// running or not built in; neither calls `reboot(2)`.
pub fn press(key: PowerKey, power: &mut dyn Power, sleeping: bool) -> Option<Child> {
    let done = match key {
        PowerKey::Reboot => match ctrl_alt_del(power) {
            0 => {
                log::warn!("grab-reboot: asking init to reboot");
                power.interrupt_init().map(|()| None)
            }
            _ => {
                log::warn!("grab-reboot: restarting the kernel at once");
                power.restart_kernel().map(|()| None)
            }
        },
        PowerKey::PowerOff => {
            log::warn!("grab-power-off: asking init to power off");
            power.power_off_init().map(|()| None)
        }
        PowerKey::Suspend if sleeping => {
            log::warn!("grab-suspend: the last suspend has not woken yet");
            Ok(None)
        }
        PowerKey::Suspend => {
            log::warn!("grab-suspend: suspending");
            power.suspend()
        }
    };
    done.unwrap_or_else(|e| {
        log::error!("{}: {e}", name(key));
        None
    })
}

/// The configuration key that binds `key`.
pub fn name(key: PowerKey) -> &'static str {
    match key {
        PowerKey::Reboot => "grab-reboot",
        PowerKey::PowerOff => "grab-power-off",
        PowerKey::Suspend => "grab-suspend",
    }
}

/// `/proc/sys/kernel/ctrl-alt-del`'s number; 0, with a warning, when it
/// cannot be read, as the C reference took it.
fn ctrl_alt_del(power: &mut dyn Power) -> i64 {
    match power.ctrl_alt_del() {
        Ok(text) => text.trim().parse().unwrap_or_else(|_| {
            log::warn!("grab-reboot: cannot read /proc/sys/kernel/ctrl-alt-del; asking init");
            0
        }),
        Err(e) => {
            log::warn!("grab-reboot: cannot open /proc/sys/kernel/ctrl-alt-del ({e}); asking init");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for PID 1, logind and `reboot(2)`: it records each call.
    #[derive(Default)]
    struct Host {
        ctrl_alt_del: Option<String>,
        calls: Vec<&'static str>,
    }

    impl Power for Host {
        fn ctrl_alt_del(&mut self) -> io::Result<String> {
            self.calls.push("read ctrl-alt-del");
            self.ctrl_alt_del
                .clone()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
        fn interrupt_init(&mut self) -> io::Result<()> {
            self.calls.push("SIGINT to PID 1");
            Ok(())
        }
        fn power_off_init(&mut self) -> io::Result<()> {
            self.calls.push("SIGRTMIN+4 to PID 1");
            Ok(())
        }
        fn restart_kernel(&mut self) -> io::Result<()> {
            self.calls.push("sync, reboot(RB_AUTOBOOT)");
            Ok(())
        }
        fn suspend(&mut self) -> io::Result<Option<Child>> {
            self.calls.push("suspend");
            Ok(None)
        }
    }

    fn reboot_with(ctrl_alt_del: Option<&str>) -> Vec<&'static str> {
        let mut host = Host {
            ctrl_alt_del: ctrl_alt_del.map(str::to_owned),
            ..Host::default()
        };
        press(PowerKey::Reboot, &mut host, false);
        host.calls
    }

    #[test]
    fn the_reboot_asks_init_while_ctrl_alt_del_is_0() {
        assert_eq!(
            reboot_with(Some("0\n")),
            ["read ctrl-alt-del", "SIGINT to PID 1"]
        );
    }

    #[test]
    fn the_reboot_restarts_the_kernel_while_ctrl_alt_del_is_set() {
        for text in ["1\n", "-1", " 7 "] {
            assert_eq!(
                reboot_with(Some(text)),
                ["read ctrl-alt-del", "sync, reboot(RB_AUTOBOOT)"],
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_reboot_asks_init_when_ctrl_alt_del_cannot_be_read() {
        assert_eq!(reboot_with(None), ["read ctrl-alt-del", "SIGINT to PID 1"]);
        assert_eq!(
            reboot_with(Some("on")),
            ["read ctrl-alt-del", "SIGINT to PID 1"]
        );
    }

    #[test]
    fn power_off_asks_init_and_suspend_asks_the_host() {
        let mut host = Host::default();
        press(PowerKey::PowerOff, &mut host, false);
        press(PowerKey::Suspend, &mut host, false);
        assert_eq!(host.calls, ["SIGRTMIN+4 to PID 1", "suspend"]);
    }

    #[test]
    fn a_suspend_waits_for_the_last_one_to_wake() {
        let mut host = Host::default();
        press(PowerKey::Suspend, &mut host, true);
        assert_eq!(host.calls, [] as [&str; 0]);
        // The other keys act whatever a suspend's state.
        press(PowerKey::PowerOff, &mut host, true);
        assert_eq!(host.calls, ["SIGRTMIN+4 to PID 1"]);
    }
}
