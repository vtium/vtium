//! The keys vtium takes for itself, before the encoder (ADR-1005 D7,
//! D10): the [`Chord`]s each key binding of vtium.conf binds, and the key
//! order they are matched in. After the VT's keys, which no key binds,
//! come the power keys ([`crate::power`]), then the heads' chords
//! ([`crate::head::chord`]), then the paste, then the terminal's grabs. A
//! binding matches the held modifiers exactly, and the key's keysyms.
//! Each key's default is its built-in default in the key table, so with no
//! vtium.conf vtium takes the keys it took before they were configurable:
//! Shift+Insert pastes, as xterm's `insert-selection` translation does,
//! and the prompt jumps, Ctrl+Shift+Up and Down, are ADR-1002's, beside
//! the scroll keys as Ghostty places its own.

use crate::config::{Config, Key, keys};
use crate::head::chord::HeadChord;
use crate::power::PowerKey;

/// What a terminal's grab does.
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
    /// The view to the nearest prompt above its top row (ADR-1002, "What
    /// the marks are for").
    PromptUp,
    /// The view to the nearest prompt below its top row, or to the screen.
    PromptDown,
    /// The console shown resets, as RIS resets it (ADR-1005 D10).
    Reset,
}

impl Grab {
    /// A key the main screen takes and the alternate screen leaves to the
    /// program (ADR-1005 D7): a scroll key or a prompt jump.
    pub fn main_screen_only(self) -> bool {
        matches!(
            self,
            Grab::ScrollUp
                | Grab::ScrollDown
                | Grab::PageUp
                | Grab::PageDown
                | Grab::PromptUp
                | Grab::PromptDown
        )
    }
}

/// What a binding does, in the part of the key order it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A power key, the seat's.
    Power(PowerKey),
    /// A console chord or the focus chord.
    Head(HeadChord),
    /// The paste.
    Paste,
    /// A terminal's grab.
    Grab(Grab),
}

/// Every key of vtium.conf that binds an action, in the key order: a
/// combination two of them bind goes to the one that comes first here.
pub const ORDER: [(&Key, Action); 21] = [
    (&keys::KEY_REBOOT, Action::Power(PowerKey::Reboot)),
    (&keys::KEY_POWER_OFF, Action::Power(PowerKey::PowerOff)),
    (&keys::KEY_SUSPEND, Action::Power(PowerKey::Suspend)),
    (
        &keys::KEY_CONSOLE_NEXT,
        Action::Head(HeadChord::NextConsole),
    ),
    (
        &keys::KEY_CONSOLE_PREVIOUS,
        Action::Head(HeadChord::PreviousConsole),
    ),
    (
        &keys::KEY_CONSOLE_OPEN,
        Action::Head(HeadChord::OpenConsole),
    ),
    (
        &keys::KEY_CONSOLE_CLOSE,
        Action::Head(HeadChord::CloseConsole),
    ),
    (&keys::KEY_FOCUS_NEXT, Action::Head(HeadChord::NextHead)),
    (
        &keys::KEY_FOCUS_PREVIOUS,
        Action::Head(HeadChord::PreviousHead),
    ),
    (&keys::KEY_PASTE, Action::Paste),
    (&keys::KEY_SCROLL_UP, Action::Grab(Grab::ScrollUp)),
    (&keys::KEY_SCROLL_DOWN, Action::Grab(Grab::ScrollDown)),
    (&keys::KEY_PAGE_UP, Action::Grab(Grab::PageUp)),
    (&keys::KEY_PAGE_DOWN, Action::Grab(Grab::PageDown)),
    (&keys::KEY_PROMPT_UP, Action::Grab(Grab::PromptUp)),
    (&keys::KEY_PROMPT_DOWN, Action::Grab(Grab::PromptDown)),
    (&keys::KEY_ZOOM_IN, Action::Grab(Grab::ZoomIn)),
    (&keys::KEY_ZOOM_OUT, Action::Grab(Grab::ZoomOut)),
    (&keys::KEY_ROTATE_CW, Action::Grab(Grab::RotateCw)),
    (&keys::KEY_ROTATE_CCW, Action::Grab(Grab::RotateCcw)),
    (&keys::KEY_RESET, Action::Grab(Grab::Reset)),
];

/// The combinations each action is bound to, as the configuration binds
/// them, in the key order. The configuration's reading has already given
/// each combination to one key alone and dropped every combination that
/// switches the VT ([`crate::config::load`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    bound: Vec<(Action, Vec<Chord>)>,
}

impl Bindings {
    /// The bindings `config` holds.
    pub fn of(config: &Config) -> Bindings {
        Bindings {
            bound: ORDER
                .iter()
                .map(|&(key, action)| (action, config.chords(key).to_vec()))
                .collect(),
        }
    }

    /// The action a key held with `mods` (the bits `SHIFT` to `LOGO`) and
    /// making `keysyms` is bound to, the first in the key order.
    pub fn action(&self, mods: u8, keysyms: &[u32]) -> Option<Action> {
        self.bound
            .iter()
            .find(|(_, chords)| matches(chords, mods, keysyms))
            .map(|&(action, _)| action)
    }

    /// The keys a person presses for `action`, such as `Ctrl+Super+↑`:
    /// its first combination ([`pressed`]), for the notices that tell a
    /// person which keys to press; `None` when nothing is bound to it.
    pub fn keys(&self, action: Action) -> Option<String> {
        self.bound
            .iter()
            .find(|&&(a, _)| a == action)
            .and_then(|(_, chords)| chords.first())
            .map(pressed)
    }
}

impl std::default::Default for Bindings {
    /// The built-in bindings, those of a configuration that sets none.
    fn default() -> Bindings {
        Bindings::of(&Config::default())
    }
}

pub(crate) const SHIFT: u8 = 1;
const LOCK: u8 = 2;
pub(crate) const CTRL: u8 = 4;
pub(crate) const ALT: u8 = 8;
pub(crate) const LOGO: u8 = 16;

/// The modifiers as vtium.conf writes them, in the order [`written`]
/// writes them.
const WRITTEN: [(u8, &str); 5] = [
    (CTRL, "Ctrl"),
    (ALT, "Alt"),
    (SHIFT, "Shift"),
    (LOGO, "Logo"),
    (LOCK, "Lock"),
];

/// The modifiers as a person presses them, in the order [`pressed`]
/// names them: Ctrl, Alt, Shift, then Super.
const PRESSED: [(u8, &str); 5] = [
    (CTRL, "Ctrl"),
    (ALT, "Alt"),
    (SHIFT, "Shift"),
    (LOGO, "Super"),
    (LOCK, "Caps Lock"),
];

/// The xkb name of `keysym`.
fn keysym_name(keysym: u32) -> String {
    xkbcommon::xkb::keysym_get_name(keysym.into())
}

/// `chord` as vtium.conf writes it, such as `<Ctrl><Shift>Delete`, for
/// the reports that name a combination.
pub fn written(chord: &Chord) -> String {
    let mods: String = WRITTEN
        .iter()
        .filter(|&&(bit, _)| chord.mods & bit != 0)
        .map(|&(_, name)| format!("<{name}>"))
        .collect();
    let keys: Vec<String> = chord.keysyms.iter().map(|&k| keysym_name(k)).collect();
    format!("{mods}{}", keys.join("+"))
}

/// `chord` as a person presses it, such as `Ctrl+Super+↑`: the modifiers
/// it is held with, in the order Ctrl, Alt, Shift, Super, then its keys
/// by their xkb names, an arrow key as its arrow, all joined by `+`.
pub fn pressed(chord: &Chord) -> String {
    let arrow = |keysym: u32| {
        let name = keysym_name(keysym);
        match name.as_str() {
            "Up" => "\u{2191}".to_owned(),
            "Down" => "\u{2193}".to_owned(),
            "Left" => "\u{2190}".to_owned(),
            "Right" => "\u{2192}".to_owned(),
            _ => name,
        }
    };
    PRESSED
        .iter()
        .filter(|&&(bit, _)| chord.mods & bit != 0)
        .map(|&(_, name)| name.to_owned())
        .chain(chord.keysyms.iter().map(|&k| arrow(k)))
        .collect::<Vec<_>>()
        .join("+")
}

/// Whether `chord` is a key that switches the VT, which no key of
/// vtium.conf binds (ADR-1005 D10): Ctrl and Alt with F1 to F12, with any
/// other modifier too, and XF86Switch_VT_1 to 12, as the seat matches
/// them first; the rescue key, the shown VT's own Ctrl+Alt+F key, is one
/// of them.
pub fn switches_vt(chord: &Chord) -> bool {
    use xkbcommon::xkb::keysyms;
    let Some(&first) = chord.keysyms.first() else {
        return false;
    };
    let f_key = (keysyms::KEY_F1..=keysyms::KEY_F12).contains(&first);
    let vt_key = (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&first);
    vt_key || f_key && chord.mods & (CTRL | ALT) == CTRL | ALT
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

/// The chords `text` binds, in the syntax `kmscon` configuration files
/// use for key bindings: a comma-separated list, each chord
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

/// The keysym xkb names `name`, case-sensitively.
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

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    const UP: u32 = 0xff52;
    const DOWN: u32 = 0xff54;
    const LEFT: u32 = 0xff51;
    const RIGHT: u32 = 0xff53;
    const PRIOR: u32 = 0xff55;
    const NEXT: u32 = 0xff56;
    const RETURN: u32 = 0xff0d;
    const BACKSPACE: u32 = 0xff08;
    const PLUS: u32 = 0x2b;
    const EQUAL: u32 = 0x3d;
    const MINUS: u32 = 0x2d;
    const INSERT: u32 = 0xff63;

    /// The action the built-in bindings give a key.
    fn action(mods: u8, keysyms: &[u32]) -> Option<Action> {
        Bindings::default().action(mods, keysyms)
    }

    // ADR-1005 D10: with no vtium.conf, each key does what it did before
    // the keys were configurable.
    #[test]
    fn the_built_in_bindings_are_the_keys_vtium_took_before() {
        let g = |grab| Some(Action::Grab(grab));
        let c = |chord| Some(Action::Head(chord));
        assert_eq!(action(SHIFT, &[UP]), g(Grab::ScrollUp));
        assert_eq!(action(SHIFT, &[DOWN]), g(Grab::ScrollDown));
        assert_eq!(action(SHIFT, &[PRIOR]), g(Grab::PageUp));
        assert_eq!(action(SHIFT, &[NEXT]), g(Grab::PageDown));
        assert_eq!(action(CTRL | SHIFT, &[UP]), g(Grab::PromptUp));
        assert_eq!(action(CTRL | SHIFT, &[DOWN]), g(Grab::PromptDown));
        assert_eq!(action(CTRL, &[PLUS]), g(Grab::ZoomIn));
        assert_eq!(action(CTRL, &[EQUAL]), g(Grab::ZoomIn));
        assert_eq!(action(CTRL, &[MINUS]), g(Grab::ZoomOut));
        assert_eq!(action(LOGO, &[PLUS]), g(Grab::RotateCw));
        assert_eq!(action(LOGO, &[EQUAL]), g(Grab::RotateCw));
        assert_eq!(action(LOGO, &[MINUS]), g(Grab::RotateCcw));
        assert_eq!(action(CTRL | SHIFT, &[DELETE]), g(Grab::Reset));
        assert_eq!(action(SHIFT, &[INSERT]), Some(Action::Paste));
        let ctrl_super = CTRL | LOGO;
        assert_eq!(action(ctrl_super, &[RIGHT]), c(HeadChord::NextConsole));
        assert_eq!(action(ctrl_super, &[LEFT]), c(HeadChord::PreviousConsole));
        assert_eq!(action(ctrl_super, &[RETURN]), c(HeadChord::OpenConsole));
        assert_eq!(action(ctrl_super, &[BACKSPACE]), c(HeadChord::CloseConsole));
        assert_eq!(action(ctrl_super, &[UP]), c(HeadChord::NextHead));
        assert_eq!(action(ctrl_super, &[DOWN]), c(HeadChord::PreviousHead));
        // The power keys are unbound until vtium.conf binds them.
        assert!(
            Bindings::default()
                .bound
                .iter()
                .filter(|(a, _)| matches!(a, Action::Power(_)))
                .all(|(_, chords)| chords.is_empty())
        );
    }

    #[test]
    fn the_modifiers_match_exactly() {
        assert_eq!(action(0, &[UP]), None);
        assert_eq!(action(SHIFT | ALT, &[UP]), None);
        assert_eq!(action(CTRL, &[UP]), None);
        assert_eq!(action(CTRL | SHIFT | ALT, &[UP]), None);
        // Shift held for '+' is not Ctrl alone: Ctrl+= is the zoom.
        assert_eq!(action(CTRL | SHIFT, &[PLUS]), None);
        assert_eq!(action(SHIFT, &[UP, DOWN]), None);
        assert_eq!(
            action(CTRL | LOGO | SHIFT, &[UP]),
            None,
            "a chord with Shift"
        );
    }

    // ADR-1005 D7: the scroll keys and the prompt jumps are the main
    // screen's; zoom, rotation and reset are vtium's on both screens.
    #[test]
    fn only_the_scroll_keys_and_the_prompt_jumps_are_the_main_screens_alone() {
        let main_only: Vec<Grab> = [
            Grab::ScrollUp,
            Grab::ScrollDown,
            Grab::PageUp,
            Grab::PageDown,
            Grab::PromptUp,
            Grab::PromptDown,
            Grab::ZoomIn,
            Grab::ZoomOut,
            Grab::RotateCw,
            Grab::RotateCcw,
            Grab::Reset,
        ]
        .into_iter()
        .filter(|g| g.main_screen_only())
        .collect();
        assert_eq!(
            main_only,
            [
                Grab::ScrollUp,
                Grab::ScrollDown,
                Grab::PageUp,
                Grab::PageDown,
                Grab::PromptUp,
                Grab::PromptDown,
            ]
        );
    }

    // ADR-1005 D10: each binding is a key the configuration reads, and the
    // defaults bind no combination twice and none that switches the VT.
    #[test]
    fn the_key_order_holds_every_binding_key_once_and_the_defaults_never_collide() {
        let mut names: Vec<&str> = ORDER.iter().map(|(k, _)| k.name).collect();
        let binding_keys: Vec<&str> = keys::KEYS
            .iter()
            .filter(|k| k.kind == crate::config::Kind::KeyBinding)
            .map(|k| k.name)
            .collect();
        names.sort_unstable();
        let mut sorted = binding_keys.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let mut seen: Vec<Chord> = Vec::new();
        for (key, _) in &ORDER {
            for chord in Config::default().chords(key) {
                assert!(!seen.contains(chord), "{} twice", written(chord));
                assert!(!switches_vt(chord), "{}", written(chord));
                seen.push(chord.clone());
            }
        }
    }

    // ADR-1005 D10: Ctrl+Alt+F1 to F12, with any other modifier, and the
    // XF86Switch_VT keys switch the VT; nothing else does.
    #[test]
    fn the_vt_keys_are_ctrl_alt_and_a_function_key_or_a_switch_vt_key() {
        let f1 = 0xffbe;
        let f12 = 0xffc9;
        assert!(switches_vt(&chord(CTRL | ALT, &[f1])));
        assert!(switches_vt(&chord(CTRL | ALT | SHIFT, &[f12])));
        assert!(switches_vt(&chord(0, &[0x1008_fe01])));
        assert!(switches_vt(&chord(LOGO, &[0x1008_fe0c])));
        assert!(!switches_vt(&chord(CTRL, &[f1])));
        assert!(!switches_vt(&chord(ALT, &[f1])));
        assert!(!switches_vt(&chord(CTRL | ALT, &[0xffca])), "F13");
        assert!(!switches_vt(&chord(CTRL | ALT, &[DELETE])));
    }

    // ADR-1005 D10: a notice names the keys vtium.conf binds, and none when
    // the key is empty.
    #[test]
    fn a_bindings_keys_name_its_first_combination_as_a_person_presses_it() {
        let d = Bindings::default();
        let keys = |chord| d.keys(Action::Head(chord));
        assert_eq!(keys(HeadChord::NextHead).as_deref(), Some("Ctrl+Super+↑"));
        assert_eq!(
            keys(HeadChord::PreviousHead).as_deref(),
            Some("Ctrl+Super+↓")
        );
        assert_eq!(
            keys(HeadChord::NextConsole).as_deref(),
            Some("Ctrl+Super+→")
        );
        assert_eq!(
            keys(HeadChord::PreviousConsole).as_deref(),
            Some("Ctrl+Super+←")
        );
        assert_eq!(
            keys(HeadChord::OpenConsole).as_deref(),
            Some("Ctrl+Super+Return")
        );
        assert_eq!(
            keys(HeadChord::CloseConsole).as_deref(),
            Some("Ctrl+Super+BackSpace")
        );
        assert_eq!(
            d.keys(Action::Grab(Grab::Reset)).as_deref(),
            Some("Ctrl+Shift+Delete")
        );
        assert_eq!(d.keys(Action::Power(PowerKey::Reboot)), None);
        let mut config = Config::default();
        config.set_chords(
            &keys::KEY_FOCUS_NEXT,
            parse("<Alt>n,<Logo>Tab").expect("chords"),
        );
        config.set_chords(&keys::KEY_FOCUS_PREVIOUS, Vec::new());
        let bound = Bindings::of(&config);
        assert_eq!(
            bound.keys(Action::Head(HeadChord::NextHead)).as_deref(),
            Some("Alt+n")
        );
        assert_eq!(bound.keys(Action::Head(HeadChord::PreviousHead)), None);
    }

    // The keys a notice names are the keys the matcher takes, and the
    // combination a report writes reads back as itself.
    #[test]
    fn a_combination_written_or_pressed_reads_back_as_itself() {
        let d = Bindings::default();
        for (action, chords) in &d.bound {
            for chord in chords {
                assert_eq!(parse(&written(chord)), Ok(vec![chord.clone()]));
                let text = pressed(chord)
                    .replace("Super", "Logo")
                    .replace('\u{2191}', "Up")
                    .replace('\u{2193}', "Down")
                    .replace('\u{2190}', "Left")
                    .replace('\u{2192}', "Right");
                let mut parts: Vec<&str> = text.split('+').collect();
                let key = parts.pop().unwrap_or_default();
                let mods: String = parts.iter().map(|m| format!("<{m}>")).collect();
                let back = parse(&format!("{mods}{key}")).expect("reads");
                assert_eq!(back, std::slice::from_ref(chord), "{text}");
                assert_eq!(d.action(chord.mods, &chord.keysyms), Some(*action));
            }
        }
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
    fn a_chord_reads_its_modifiers_and_keysym_names() {
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
}
