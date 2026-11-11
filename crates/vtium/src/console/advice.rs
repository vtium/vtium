//! What `vtium console enable` says with no mode, and whether seat mode can
//! run (ADR-1500 D7, ADR-1400 D3).
//!
//! Plain `enable` turns nothing on. It reports what vtium finds on the
//! machine and recommends a mode. [`recommend`] holds the rule, and
//! [`seat_block`] says why seat mode cannot run, which `enable --seat`
//! refuses with too.

use super::Mode;

/// What vtium finds on the machine, beyond the systemd check that every
/// verb makes first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// Whether logind runs and manages seat0: whether
    /// `/run/systemd/seats/seat0` exists.
    pub seat0: bool,
    /// Whether this vtium is built with `libseat` and `logind`.
    pub seat_build: bool,
    /// Whether a unit directory has `vtium-seat@.service`, and does not
    /// mask it.
    pub seat_unit: bool,
    /// The display manager's unit, when one is enabled.
    pub display_manager: Option<String>,
}

/// Why seat mode cannot run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatBlock {
    /// This vtium is built without `libseat` and `logind`.
    Build,
    /// No unit directory has `vtium-seat@.service`, or it is masked.
    Unit,
    /// logind manages no seat0.
    Logind,
}

/// The first reason seat mode cannot run on the machine `facts` describe,
/// or `None` when it can.
pub fn seat_block(facts: &Facts) -> Option<SeatBlock> {
    if !facts.seat_build {
        Some(SeatBlock::Build)
    } else if !facts.seat_unit {
        Some(SeatBlock::Unit)
    } else if !facts.seat0 {
        Some(SeatBlock::Logind)
    } else {
        None
    }
}

/// The mode vtium recommends on a host that runs systemd. Direct mode
/// gives each login an active logind session on seat0, as a getty does,
/// and seat mode gives each login a session with no seat, which polkit
/// treats as inactive and which udev's uaccess rules give no devices
/// (ADR-1400 D3). So vtium recommends direct mode on every such host, and
/// the advice offers seat mode as a choice, with its cost.
pub fn recommend(_facts: &Facts) -> Mode {
    Mode::Direct
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(seat0: bool, seat_build: bool, seat_unit: bool) -> Facts {
        Facts {
            seat0,
            seat_build,
            seat_unit,
            display_manager: None,
        }
    }

    #[test]
    fn seat_mode_needs_the_build_its_unit_and_loginds_seat0_in_that_order() {
        assert_eq!(seat_block(&facts(true, true, true)), None);
        assert_eq!(
            seat_block(&facts(true, false, true)),
            Some(SeatBlock::Build)
        );
        assert_eq!(
            seat_block(&facts(false, false, false)),
            Some(SeatBlock::Build)
        );
        assert_eq!(seat_block(&facts(true, true, false)), Some(SeatBlock::Unit));
        assert_eq!(
            seat_block(&facts(false, true, false)),
            Some(SeatBlock::Unit)
        );
        assert_eq!(
            seat_block(&facts(false, true, true)),
            Some(SeatBlock::Logind)
        );
    }

    #[test]
    fn direct_mode_is_recommended_whether_or_not_seat_mode_can_run() {
        for seat0 in [false, true] {
            for build in [false, true] {
                for unit in [false, true] {
                    let mut f = facts(seat0, build, unit);
                    assert_eq!(recommend(&f), Mode::Direct, "{f:?}");
                    f.display_manager = Some("sddm.service".into());
                    assert_eq!(recommend(&f), Mode::Direct, "{f:?}");
                }
            }
        }
    }
}
