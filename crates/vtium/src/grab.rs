//! The keys vtium takes for itself, before the encoder.
//!
//! The seat's are the power keys, second in the key order after the VT's:
//! `grab-reboot`, `grab-power-off` and `grab-suspend`, each unbound until
//! the configuration binds a [`Chord`] to it. The terminal's grabs come
//! third. Each is the C reference's default (`src/config.c:1275-1303` at
//! kmscon b1f7a7c5), matched as it matched them
//! (`src/misc/conf.c:1103-1131`): the held modifiers exactly, and the
//! key's keysyms. The C reference had no key to paste; Shift+Insert
//! pastes, as xterm's `insert-selection` translation does.

use std::io;

/// What a grab does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ZoomIn,
    ZoomOut,
    RotateCw,
    RotateCcw,
    Paste,
}

const SHIFT: u8 = 1;
const LOCK: u8 = 2;
const CTRL: u8 = 4;
const ALT: u8 = 8;
const LOGO: u8 = 16;

const UP: u32 = 0xff52;
const DOWN: u32 = 0xff54;
const PRIOR: u32 = 0xff55;
const NEXT: u32 = 0xff56;
const PLUS: u32 = 0x2b;
const EQUAL: u32 = 0x3d;
const MINUS: u32 = 0x2d;
const INSERT: u32 = 0xff63;

/// Each grab: its modifiers and the one keysym it takes.
const GRABS: [(Grab, u8, u32); 11] = [
    (Grab::ScrollUp, SHIFT, UP),
    (Grab::ScrollDown, SHIFT, DOWN),
    (Grab::PageUp, SHIFT, PRIOR),
    (Grab::PageDown, SHIFT, NEXT),
    (Grab::ZoomIn, CTRL, PLUS),
    (Grab::ZoomIn, CTRL, EQUAL),
    (Grab::ZoomOut, CTRL, MINUS),
    (Grab::RotateCw, LOGO, PLUS),
    (Grab::RotateCw, LOGO, EQUAL),
    (Grab::RotateCcw, LOGO, MINUS),
    (Grab::Paste, SHIFT, INSERT),
];

/// The grab a key held with `mods` (the C reference's bits) and making
/// `keysyms` takes, if any.
pub fn grab(mods: u8, keysyms: &[u32]) -> Option<Grab> {
    GRABS
        .iter()
        .find(|&&(_, m, sym)| m == mods && keysyms == [sym])
        .map(|&(g, _, _)| g)
}

/// A key a configuration key binds: the modifiers held, exactly, and the
/// keysyms the key makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: u8,
    pub keysyms: Vec<u32>,
}

/// The modifiers a chord names, in any case.
const MODIFIERS: [(&str, u8); 6] = [
    ("shift", SHIFT),
    ("lock", LOCK),
    ("control", CTRL),
    ("ctrl", CTRL),
    ("alt", ALT),
    ("logo", LOGO),
];

/// The chords `text` binds, in the C reference's syntax
/// (`src/misc/conf.c:859-1012`): a comma-separated list, each chord
/// `<Mod>...` and then keysym names joined by `+`, such as
/// `<Ctrl><Alt>Delete`. Spaces around a name are dropped. A chord with no
/// keysym, and so an empty list, binds nothing.
pub fn parse(text: &str) -> Result<Vec<Chord>, &'static str> {
    let mut chords = Vec::new();
    for item in text.split(',').filter(|s| !s.is_empty()) {
        let mut chord = Chord {
            mods: 0,
            keysyms: Vec::new(),
        };
        for (i, part) in item.split('+').filter(|s| !s.is_empty()).enumerate() {
            let mut rest = part.trim_start_matches(' ');
            // Only the first part takes modifiers.
            while let Some(after) = rest.strip_prefix('<').filter(|_| i == 0) {
                let (name, tail) = after.split_once('>').ok_or("a '<' with no '>'")?;
                let (_, bit) = MODIFIERS
                    .iter()
                    .find(|(m, _)| m.eq_ignore_ascii_case(name))
                    .ok_or("a modifier other than Shift, Lock, Control, Ctrl, Alt or Logo")?;
                chord.mods |= bit;
                rest = tail.trim_start_matches(' ');
            }
            let name = rest.trim_end_matches(' ');
            if !name.is_empty() {
                chord.keysyms.push(keysym(name)?);
            }
        }
        if !chord.keysyms.is_empty() {
            chords.push(chord);
        }
    }
    Ok(chords)
}

/// The keysym xkb names `name`, case-sensitively, as the C reference
/// looked it up.
fn keysym(name: &str) -> Result<u32, &'static str> {
    use xkbcommon::xkb;
    if name.contains('\0') {
        return Err("a key name xkb does not know");
    }
    let sym = xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS).raw();
    if sym != 0 {
        return Ok(sym);
    }
    if xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE).raw() != 0 {
        Err("a key name in the wrong case; key names are case-sensitive")
    } else {
        Err("a key name xkb does not know")
    }
}

/// A key held with `mods` and making `keysyms` is one of `chords`.
pub fn matches(chords: &[Chord], mods: u8, keysyms: &[u32]) -> bool {
    chords
        .iter()
        .any(|c| c.mods == mods && c.keysyms == keysyms)
}

/// A power key: the seat's, unbound by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerKey {
    /// `grab-reboot`.
    Reboot,
    /// `grab-power-off`.
    PowerOff,
    /// `grab-suspend`.
    Suspend,
}

/// What the power keys act through: PID 1, logind and the kernel. The
/// binary's is [`System`]; a test's is a stand-in.
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
    /// logind's `Suspend`.
    fn suspend(&mut self) -> io::Result<()>;
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
    fn suspend(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "logind's Suspend is not built in",
        ))
    }
}

/// Acts on a power key's press through `power`, and logs what it did.
///
/// The reboot takes the C reference's two paths (`src/seat.c:479-513`),
/// chosen by `/proc/sys/kernel/ctrl-alt-del`, which the kernel reads for
/// its own Ctrl+Alt+Del: at 0, or when it cannot be read, PID 1 is asked
/// to reboot; at any other number the kernel restarts at once. Power-off
/// asks PID 1 and suspend asks logind; neither calls `reboot(2)`.
pub fn press(key: PowerKey, power: &mut dyn Power) {
    let done = match key {
        PowerKey::Reboot => match ctrl_alt_del(power) {
            0 => {
                log::warn!("grab-reboot: asking init to reboot");
                power.interrupt_init()
            }
            _ => {
                log::warn!("grab-reboot: restarting the kernel at once");
                power.restart_kernel()
            }
        },
        PowerKey::PowerOff => {
            log::warn!("grab-power-off: asking init to power off");
            power.power_off_init()
        }
        PowerKey::Suspend => {
            log::warn!("grab-suspend: asking logind to suspend");
            power.suspend()
        }
    };
    if let Err(e) = done {
        log::error!("{}: {e}", name(key));
    }
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
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn the_c_references_defaults() {
        assert_eq!(grab(SHIFT, &[UP]), Some(Grab::ScrollUp));
        assert_eq!(grab(SHIFT, &[NEXT]), Some(Grab::PageDown));
        assert_eq!(grab(CTRL, &[EQUAL]), Some(Grab::ZoomIn));
        assert_eq!(grab(CTRL, &[MINUS]), Some(Grab::ZoomOut));
        assert_eq!(grab(LOGO, &[PLUS]), Some(Grab::RotateCw));
    }

    // covers: clipboard.paste
    #[test]
    fn shift_insert_pastes_as_in_xterm() {
        assert_eq!(grab(SHIFT, &[INSERT]), Some(Grab::Paste));
        assert_eq!(grab(0, &[INSERT]), None);
    }

    #[test]
    fn the_modifiers_match_exactly() {
        assert_eq!(grab(0, &[UP]), None);
        assert_eq!(grab(SHIFT | CTRL, &[UP]), None);
        // Shift held for '+' is not Ctrl alone: Ctrl+= is the zoom.
        assert_eq!(grab(CTRL | SHIFT, &[PLUS]), None);
        assert_eq!(grab(SHIFT, &[UP, DOWN]), None);
    }

    const DELETE: u32 = 0xffff;
    const END: u32 = 0xff57;

    fn chord(mods: u8, keysyms: &[u32]) -> Chord {
        Chord {
            mods,
            keysyms: keysyms.to_vec(),
        }
    }

    #[test]
    fn a_chord_reads_as_the_c_reference_read_it() {
        assert_eq!(
            parse("<Ctrl><Alt>Delete"),
            Ok(vec![chord(CTRL | ALT, &[DELETE])])
        );
        // Modifier names in any case, spaces around the names, `+`
        // joining parts, a list.
        assert_eq!(
            parse(" <control> <ALT> Delete ,<Logo>+End,<Shift>Up+Down"),
            Ok(vec![
                chord(CTRL | ALT, &[DELETE]),
                chord(LOGO, &[END]),
                chord(SHIFT, &[UP, DOWN]),
            ])
        );
        assert_eq!(parse("<Lock>plus"), Ok(vec![chord(LOCK, &[PLUS])]));
        // Nothing to bind: an empty value, a chord of modifiers alone.
        assert_eq!(parse(""), Ok(vec![]));
        assert_eq!(parse(",,<Ctrl>"), Ok(vec![]));
    }

    #[test]
    fn a_chord_that_does_not_read_is_refused() {
        assert_eq!(
            parse("<Ctrl>NoSuchKey"),
            Err("a key name xkb does not know")
        );
        assert_eq!(
            parse("<Ctrl>delete"),
            Err("a key name in the wrong case; key names are case-sensitive")
        );
        assert_eq!(parse("<Ctrl Delete"), Err("a '<' with no '>'"));
        assert_eq!(
            parse("<Hyper>Delete"),
            Err("a modifier other than Shift, Lock, Control, Ctrl, Alt or Logo")
        );
        // Modifiers only before the first key.
        assert_eq!(
            parse("Delete+<Ctrl>End"),
            Err("a key name xkb does not know")
        );
        assert_eq!(parse("<Ctrl>\0"), Err("a key name xkb does not know"));
    }

    #[test]
    fn a_chord_matches_its_modifiers_and_keysyms_exactly() {
        let chords = parse("<Ctrl><Alt>Delete,<Logo>End").expect("chords");
        assert!(matches(&chords, CTRL | ALT, &[DELETE]));
        assert!(matches(&chords, LOGO, &[END]));
        assert!(!matches(&chords, CTRL | ALT | SHIFT, &[DELETE]));
        assert!(!matches(&chords, CTRL, &[DELETE]));
        assert!(!matches(&chords, CTRL | ALT, &[END]));
        assert!(!matches(&chords, CTRL | ALT, &[DELETE, END]));
        assert!(!matches(&[], CTRL | ALT, &[DELETE]));
    }

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
        fn suspend(&mut self) -> io::Result<()> {
            self.calls.push("logind Suspend");
            Ok(())
        }
    }

    fn reboot_with(ctrl_alt_del: Option<&str>) -> Vec<&'static str> {
        let mut host = Host {
            ctrl_alt_del: ctrl_alt_del.map(str::to_owned),
            ..Host::default()
        };
        press(PowerKey::Reboot, &mut host);
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
    fn power_off_asks_init_and_suspend_asks_logind() {
        let mut host = Host::default();
        press(PowerKey::PowerOff, &mut host);
        press(PowerKey::Suspend, &mut host);
        assert_eq!(host.calls, ["SIGRTMIN+4 to PID 1", "logind Suspend"]);
    }
}
