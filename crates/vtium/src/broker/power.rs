//! The power keys, which vtium's root process runs for its drawing
//! process.
//!
//! Rebooting, powering off and suspending the machine need root, so the
//! drawing process asks the root process to run a power key that a person
//! pressed. The root process runs only a key that its own reading of
//! vtium.conf binds: a drawing process that crafted output has taken over
//! can run no power key that vtium.conf leaves unbound. It runs a bound
//! key as vtium in one process does ([`vtium::power::press`]).
//!
//! References:
//! - ADR-1400 D4, "What the broker does": the broker runs the power keys
//!   that vtium.conf binds.
//! - ADR-1400 D4, "vtium.conf under the split": the broker reads the power
//!   keys from vtium.conf itself.

use std::cell::Cell;
use std::rc::Rc;

use vtium::config::{Config, keys};
use vtium::power::{self as keys_power, Power};
use vtium_platform::calloop::LoopHandle;

use super::protocol::PowerKey;
use super::watch::Seen;

/// The power keys vtium.conf binds, and what they act through.
pub struct PowerKeys {
    bound: [bool; 3],
    power: Box<dyn Power>,
    /// Whether a suspend has not woken yet, so a second press waits.
    sleeping: Rc<Cell<bool>>,
    handle: Option<LoopHandle<'static, Seen>>,
}

/// The power key of vtium's own table that `key` names.
fn ours(key: PowerKey) -> keys_power::PowerKey {
    match key {
        PowerKey::Reboot => keys_power::PowerKey::Reboot,
        PowerKey::PowerOff => keys_power::PowerKey::PowerOff,
        PowerKey::Suspend => keys_power::PowerKey::Suspend,
    }
}

/// The place of `key` in [`PowerKeys::bound`].
fn slot(key: PowerKey) -> usize {
    match key {
        PowerKey::Reboot => 0,
        PowerKey::PowerOff => 1,
        PowerKey::Suspend => 2,
    }
}

impl PowerKeys {
    /// The power keys `config` binds, acting through `power`. `handle` is
    /// the loop that watches a suspend's child, or `None` in a test.
    pub fn new(
        config: &Config,
        power: Box<dyn Power>,
        handle: Option<LoopHandle<'static, Seen>>,
    ) -> PowerKeys {
        let bound = |key| !config.chords(key).is_empty();
        PowerKeys {
            bound: [
                bound(&keys::KEY_REBOOT),
                bound(&keys::KEY_POWER_OFF),
                bound(&keys::KEY_SUSPEND),
            ],
            power,
            sleeping: Rc::default(),
            handle,
        }
    }

    /// Whether vtium.conf binds `key`.
    pub fn binds(&self, key: PowerKey) -> bool {
        self.bound.get(slot(key)).copied().unwrap_or(false)
    }

    /// Runs `key` when vtium.conf binds it, and logs a refusal otherwise.
    pub fn run(&mut self, key: PowerKey) {
        let name = keys_power::name(ours(key));
        if !self.binds(key) {
            log::warn!(
                target: "broker",
                "vtium's root process does not run {name}, because vtium.conf binds no keys to it"
            );
            return;
        }
        let Some(child) = keys_power::press(ours(key), self.power.as_mut(), self.sleeping.get())
        else {
            return;
        };
        match &self.handle {
            Some(handle) => {
                if let Err(e) =
                    vtium_platform::power::watch(handle, child, Rc::clone(&self.sleeping))
                {
                    log::error!(target: "broker", "{name}: vtium cannot watch the suspend: {e}");
                }
            }
            // a test's stand-in starts no real child
            None => drop(child),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::io;
    use std::path::Path;
    use std::process::Child;

    /// Records what the keys asked of the machine.
    #[derive(Clone, Default)]
    struct Recorded(Rc<RefCell<Vec<&'static str>>>);

    impl Power for Recorded {
        fn ctrl_alt_del(&mut self) -> io::Result<String> {
            Ok("0".to_owned())
        }
        fn interrupt_init(&mut self) -> io::Result<()> {
            self.0.borrow_mut().push("reboot");
            Ok(())
        }
        fn power_off_init(&mut self) -> io::Result<()> {
            self.0.borrow_mut().push("power-off");
            Ok(())
        }
        fn restart_kernel(&mut self) -> io::Result<()> {
            self.0.borrow_mut().push("restart");
            Ok(())
        }
        fn suspend(&mut self) -> io::Result<Option<Child>> {
            self.0.borrow_mut().push("suspend");
            Ok(None)
        }
    }

    fn config(args: &[&str]) -> Config {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        vtium::config::load(&args, &mut |_: &Path| Ok(None)).config
    }

    #[test]
    fn only_a_key_vtium_conf_binds_runs() {
        let recorded = Recorded::default();
        let config = config(&["--key-reboot=<Ctrl><Alt>Delete"]);
        let mut keys = PowerKeys::new(&config, Box::new(recorded.clone()), None);
        assert!(keys.binds(PowerKey::Reboot));
        assert!(!keys.binds(PowerKey::PowerOff));
        keys.run(PowerKey::PowerOff);
        keys.run(PowerKey::Suspend);
        assert!(recorded.0.borrow().is_empty(), "an unbound key ran");
        keys.run(PowerKey::Reboot);
        assert_eq!(*recorded.0.borrow(), ["reboot"]);
    }

    #[test]
    fn with_nothing_bound_no_key_runs() {
        let recorded = Recorded::default();
        let mut keys = PowerKeys::new(&config(&[]), Box::new(recorded.clone()), None);
        for key in [PowerKey::Reboot, PowerKey::PowerOff, PowerKey::Suspend] {
            keys.run(key);
        }
        assert!(recorded.0.borrow().is_empty());
    }
}
