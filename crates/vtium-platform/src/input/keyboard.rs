//! Keys through libxkbcommon: the keymap and compose table the seat
//! shares, and each keyboard's own state, compose state and repeat.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use xkbcommon::xkb::{self, Keycode, Keysym, compose, keysyms};

use super::{KeyEvent, Mods};
use crate::ioctl::{LED_CAPSL, LED_NUML, LED_SCROLLL};

/// evdev's key codes start 8 below xkb's.
const EVDEV_KEYCODE_OFFSET: u32 = 8;

/// The keymap built in for when nothing else compiles.
const FALLBACK: &str = include_str!("fallback.xkb");

/// Where the keymap comes from: a keymap file, or the rules' names
/// (`xkb-model`, `xkb-layout`, `xkb-variant`, `xkb-options`).
#[derive(Clone, Debug, Default)]
pub struct KeymapConfig {
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
    /// `xkb-keymap`: a whole keymap, tried first.
    pub keymap_file: Option<std::path::PathBuf>,
    /// `xkb-compose-file`: a compose table, tried before the locale's.
    pub compose_file: Option<std::path::PathBuf>,
    /// The locale whose compose table is taken, as [`locale`] finds it.
    pub locale: String,
}

/// The rules' names of the system's keyboard layout, as localed holds them
/// (ADR-1400's L2 layer). An empty name is one the system does not set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleNames {
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl KeymapConfig {
    /// The four rules' names this configuration sets.
    pub fn names(&self) -> RuleNames {
        RuleNames {
            model: self.model.clone(),
            layout: self.layout.clone(),
            variant: self.variant.clone(),
            options: self.options.clone(),
        }
    }

    /// This configuration, with the system's names in place of the
    /// `xkb-*` keys that vtium.conf leaves unset. A key that vtium.conf sets
    /// keeps its value. A variant names the variant of each layout in turn,
    /// so the system's variant is taken only together with the system's
    /// layout: when vtium.conf sets `xkb-layout` and not `xkb-variant`, the
    /// layout has no variant.
    #[must_use]
    pub fn follow(mut self, system: &RuleNames) -> KeymapConfig {
        let layout_set = !self.layout.is_empty();
        let take = |own: &mut String, theirs: &str| {
            if own.is_empty() {
                theirs.clone_into(own);
            }
        };
        take(&mut self.model, &system.model);
        take(&mut self.layout, &system.layout);
        if !layout_set {
            take(&mut self.variant, &system.variant);
        }
        take(&mut self.options, &system.options);
        self
    }
}

/// The locale for compose, found as locale(7) says without setting it:
/// `LC_ALL`, then `LC_CTYPE`, then `LANG`, then `C`.
pub fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .unwrap_or_else(|| String::from("C"))
}

/// What a keymap was compiled from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compiled {
    /// The keymap file that vtium.conf's `xkb-keymap` names.
    File(PathBuf),
    /// The rules' names; an empty name takes xkb's default.
    Names(RuleNames),
    /// vtium's built-in keymap.
    BuiltIn,
}

/// The seat's keymap and compose table, which every keyboard shares.
pub struct Keymap {
    context: xkb::Context,
    keymap: xkb::Keymap,
    compose: Option<compose::Table>,
    compiled: Compiled,
}

impl std::fmt::Debug for Keymap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keymap")
            .field("compose", &self.compose.is_some())
            .finish_non_exhaustive()
    }
}

impl Keymap {
    /// The configured keymap: the keymap file, else the names, else the
    /// system's default names, else the built-in fallback. `None` only
    /// when even the fallback fails.
    pub fn new(config: &KeymapConfig) -> Option<Keymap> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let (keymap, compiled) = config
            .keymap_file
            .as_deref()
            .and_then(|path| Some((from_file(&context, path)?, Compiled::File(path.to_owned()))))
            .or_else(|| Some((from_names(&context, config)?, Compiled::Names(config.names()))))
            .or_else(|| {
                log::warn!(
                    target: "input",
                    "cannot compile the keymap ({}, {}, {}, {}), taking the system's default",
                    config.model, config.layout, config.variant, config.options
                );
                let default = KeymapConfig::default();
                Some((from_names(&context, &default)?, Compiled::Names(default.names())))
            })
            .or_else(|| {
                log::warn!(target: "input", "cannot compile the default keymap, taking the built-in one");
                Some((fallback(&context)?, Compiled::BuiltIn))
            })?;
        let mut keymap = Keymap {
            context,
            keymap,
            compose: None,
            compiled,
        };
        keymap.set_compose(config);
        Some(keymap)
    }

    /// The built-in keymap alone.
    pub fn fallback() -> Option<Keymap> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = fallback(&context)?;
        Some(Keymap {
            context,
            keymap,
            compose: None,
            compiled: Compiled::BuiltIn,
        })
    }

    /// What this keymap was compiled from, which a fresh start with the
    /// same settings compiles too.
    pub fn compiled(&self) -> &Compiled {
        &self.compiled
    }

    /// The compose table: the compose file, else the locale's, else none,
    /// which turns compose off.
    pub fn set_compose(&mut self, config: &KeymapConfig) {
        self.compose = None;
        if let Some(path) = &config.compose_file {
            match std::fs::read(path) {
                Ok(text) => self.set_compose_table(&text, &config.locale),
                Err(e) => {
                    log::error!(target: "input", "cannot read the compose file {}: {e}", path.display());
                    return;
                }
            }
            if self.compose.is_none() {
                log::warn!(target: "input", "cannot parse the compose file, taking the locale's");
            }
        }
        if self.compose.is_none() && !config.locale.contains('\0') {
            self.compose = compose::Table::new_from_locale(
                &self.context,
                OsStr::new(&config.locale),
                compose::COMPILE_NO_FLAGS,
            )
            .ok();
            if self.compose.is_none() {
                log::warn!(target: "input", "cannot load the locale's compose table, compose is off");
            }
        }
    }

    /// A compose table from `text`, for `locale`.
    pub(crate) fn set_compose_table(&mut self, text: &[u8], locale: &str) {
        if locale.contains('\0') {
            return;
        }
        self.compose = compose::Table::new_from_buffer(
            &self.context,
            text,
            locale,
            compose::FORMAT_TEXT_V1,
            compose::COMPILE_NO_FLAGS,
        )
        .ok();
    }

    /// A keymap compiled from `text`, with no compose table.
    #[cfg(test)]
    pub(crate) fn from_text(text: &str) -> Option<Keymap> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES);
        let keymap = xkb::Keymap::new_from_string(
            &context,
            String::from(text),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        Some(Keymap {
            context,
            keymap,
            compose: None,
            compiled: Compiled::BuiltIn,
        })
    }
}

fn from_file(context: &xkb::Context, path: &Path) -> Option<xkb::Keymap> {
    let Ok(mut file) = std::fs::File::open(path) else {
        log::error!(target: "input", "cannot open the keymap file {}", path.display());
        return None;
    };
    let keymap = xkb::Keymap::new_from_file(
        context,
        &mut file,
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    );
    if keymap.is_none() {
        log::error!(target: "input", "cannot compile the keymap file {}", path.display());
    }
    keymap
}

/// The `evdev` rules with the configured names. libxkbcommon takes empty
/// names as its defaults; a name holding a NUL is refused.
fn from_names(context: &xkb::Context, config: &KeymapConfig) -> Option<xkb::Keymap> {
    let names = [
        &config.model,
        &config.layout,
        &config.variant,
        &config.options,
    ];
    if names.iter().any(|name| name.contains('\0')) {
        return None;
    }
    xkb::Keymap::new_from_names(
        context,
        "evdev",
        &config.model,
        &config.layout,
        &config.variant,
        Some(config.options.clone()),
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
}

fn fallback(context: &xkb::Context) -> Option<xkb::Keymap> {
    let keymap = xkb::Keymap::new_from_string(
        context,
        String::from(FALLBACK),
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    );
    if keymap.is_none() {
        log::error!(target: "input", "cannot compile the built-in keymap");
    }
    keymap
}

/// An evdev key's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyState {
    Released,
    Pressed,
    /// The kernel's own repeat, which vtium ignores: it repeats on a timer.
    Repeated,
}

impl KeyState {
    pub(crate) fn from_value(value: i32) -> Option<KeyState> {
        match value {
            0 => Some(KeyState::Released),
            1 => Some(KeyState::Pressed),
            2 => Some(KeyState::Repeated),
            _ => None,
        }
    }
}

/// What a key did to the keyboard.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Processed {
    /// A press to dispatch.
    pub(crate) key: Option<KeyEvent>,
    /// A release to dispatch, of a key whose press was dispatched.
    pub(crate) release: Option<KeyEvent>,
    /// The repeat to start (`Some`) or stop (`None` inside), if it changed.
    pub(crate) repeat: Option<RepeatChange>,
    /// The LEDs to set, when the xkb state changed them.
    pub(crate) leds: Option<Leds>,
}

/// A change to the keyboard's repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepeatChange {
    Start,
    Stop,
}

/// The keyboard LEDs, on or off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Leds {
    pub num_lock: bool,
    pub caps_lock: bool,
    pub scroll_lock: bool,
}

impl Leds {
    /// The `EV_LED` codes and values for the xkb state: Num Lock, Caps
    /// Lock, Scroll Lock.
    pub(crate) fn codes(self) -> [(u16, bool); 3] {
        [
            (LED_NUML, self.num_lock),
            (LED_CAPSL, self.caps_lock),
            (LED_SCROLLL, self.scroll_lock),
        ]
    }
}

/// The real modifiers behind NumLock, Hyper and Meta in a keymap, where
/// vtium reports them (ADR-1005, D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Special {
    num: xkb::ModMask,
    hyper: xkb::ModMask,
    meta: xkb::ModMask,
    /// The real modifiers behind the modifiers the kitty keyboard
    /// protocol encodes: Shift, Lock, Control, Alt, Super, NumLock, and
    /// Hyper and Meta where vtium reports them. A modifier outside them,
    /// such as AltGr's, chooses a symbol the protocol cannot describe.
    encodable: xkb::ModMask,
}

impl Special {
    /// NumLock's modifier is the one the Num_Lock key locks. Hyper's and
    /// Meta's are the ones their keys set, each kept only where neither
    /// Alt nor Super uses it: in xkeyboard-config's default keymaps Meta
    /// shares Alt's and Hyper Super's, so neither is reported there.
    fn of(keymap: &xkb::Keymap) -> Special {
        let real = |name: &str| match keymap.mod_get_index(name) {
            xkb::MOD_INVALID => 0,
            i => 1u32.checked_shl(i).unwrap_or(0),
        };
        let taken = set_by(keymap, &[keysyms::KEY_Alt_L, keysyms::KEY_Alt_R])
            | set_by(keymap, &[keysyms::KEY_Super_L, keysyms::KEY_Super_R])
            | real(xkb::MOD_NAME_ALT)
            | real(xkb::MOD_NAME_LOGO);
        let own = |mask: xkb::ModMask| if mask & taken == 0 { mask } else { 0 };
        let num = set_by(keymap, &[keysyms::KEY_Num_Lock]);
        let hyper = own(set_by(
            keymap,
            &[keysyms::KEY_Hyper_L, keysyms::KEY_Hyper_R],
        ));
        let meta = own(set_by(keymap, &[keysyms::KEY_Meta_L, keysyms::KEY_Meta_R]));
        Special {
            num,
            hyper,
            meta,
            encodable: taken
                | num
                | hyper
                | meta
                | real(xkb::MOD_NAME_SHIFT)
                | real(xkb::MOD_NAME_CAPS)
                | real(xkb::MOD_NAME_CTRL),
        }
    }
}

/// The real modifiers that a key making one of `syms` sets or locks, pressed
/// at the first layout's level that makes it. The keymap's own actions
/// decide them, which every libxkbcommon applies, where asking the state
/// for a virtual modifier by name needs libxkbcommon 1.8.
fn set_by(keymap: &xkb::Keymap, syms: &[u32]) -> xkb::ModMask {
    let mut mask = 0;
    keymap.key_for_each(|keymap, key| {
        for level in 0..keymap.num_levels_for_key(key, 0) {
            let makes = keymap
                .key_get_syms_by_level(key, 0, level)
                .iter()
                .any(|s| syms.contains(&s.raw()));
            if !makes {
                continue;
            }
            let mut masks = [0; 8];
            let n = keymap.key_get_mods_for_level(key, 0, level, &mut masks);
            let held = masks.get(..n).and_then(|m| m.first()).copied().unwrap_or(0);
            let mut state = xkb::State::new(keymap);
            state.update_mask(held, 0, 0, 0, 0, 0);
            let before = state.serialize_mods(xkb::STATE_MODS_EFFECTIVE);
            state.update_key(key, xkb::KeyDirection::Down);
            mask |= state.serialize_mods(xkb::STATE_MODS_EFFECTIVE) & !before;
        }
    });
    mask
}

/// The character the key with evdev's key code `code` types on the US
/// PC-101 layout, unshifted: the base layout key of the kitty keyboard
/// protocol (ADR-1005, D9). The kernel names its key codes after their
/// places on that keyboard (`linux/input-event-codes.h`), `KEY_1` 2 to
/// `KEY_EQUAL` 13, `KEY_Q` 16 to `KEY_RIGHTBRACE` 27, `KEY_A` 30 to
/// `KEY_GRAVE` 41, `KEY_BACKSLASH` 43, `KEY_Z` 44 to `KEY_SLASH` 53 and
/// `KEY_SPACE` 57, so the table holds whatever layout is configured. A key
/// the layout types no character with, such as `KEY_102ND`, the extra key
/// of ISO keyboards, has none.
pub(crate) fn base_layout_key(code: u32) -> Option<char> {
    const ROWS: [(u32, &str); 5] = [
        (2, "1234567890-="),
        (16, "qwertyuiop[]"),
        (30, "asdfghjkl;'`"),
        (43, "\\zxcvbnm,./"),
        (57, " "),
    ];
    ROWS.iter().find_map(|&(first, keys)| {
        let i = usize::try_from(code.checked_sub(first)?).ok()?;
        keys.chars().nth(i)
    })
}

/// `sym`, or for a keysym outside Latin-1 and below the function keys that
/// stands for a character, the Unicode keysym of that character, which
/// xkb treats as the same symbol: Cyrillic_es, 0x6d3, is 0x1000441.
fn unicode_keysym(sym: u32) -> u32 {
    if !(0x100..0xfd00).contains(&sym) {
        return sym;
    }
    match xkb::keysym_to_utf32(Keysym::new(sym)) {
        0 => sym,
        c => c.saturating_add(0x0100_0000),
    }
}

/// One keyboard's state: modifiers held on it apply to it alone.
pub(crate) struct Keyboard {
    state: xkb::State,
    compose: Option<compose::State>,
    /// The press repeating, while it repeats.
    repeat: Option<KeyEvent>,
    /// The presses dispatched whose keys are still held, each released
    /// once.
    held: Vec<KeyEvent>,
    special: Special,
}

impl Keyboard {
    pub(crate) fn new(keymap: &Keymap) -> Keyboard {
        Keyboard {
            state: xkb::State::new(&keymap.keymap),
            compose: keymap
                .compose
                .as_ref()
                .map(|table| compose::State::new(table, compose::STATE_NO_FLAGS)),
            repeat: None,
            held: Vec::new(),
            special: Special::of(&keymap.keymap),
        }
    }

    /// A key from evdev: `code` is evdev's, `state` its value. The keysyms
    /// are read before the state changes; compose sees presses; a press
    /// finishing no compose sequence is dispatched, and starts or stops the
    /// repeat. The release of a key whose press was dispatched is
    /// dispatched too, with the press's keysyms and the modifiers after it
    /// (ADR-1005, D8).
    pub(crate) fn process(&mut self, state: KeyState, code: u16) -> Processed {
        let mut out = Processed::default();
        if state == KeyState::Repeated {
            return out;
        }
        let keycode = Keycode::new(u32::from(code).saturating_add(EVDEV_KEYCODE_OFFSET));
        let mut syms: Vec<Keysym> = self.state.key_get_syms(keycode).to_vec();
        let mut one = Keysym::new(keysyms::KEY_NoSymbol);
        if syms.len() == 1 {
            // The Lock modifier's case change, which the plain list lacks.
            one = self.state.key_get_one_sym(keycode);
            syms = vec![one];
        }
        let mut status = compose::Status::Nothing;
        if let (Some(compose), KeyState::Pressed) = (self.compose.as_mut(), state) {
            // NoSymbol, for a key of several keysyms, cancels a sequence.
            compose.feed(one);
            status = compose.status();
            if status == compose::Status::Composed {
                match compose.keysym() {
                    Some(sym) => syms = vec![sym],
                    // A sequence that makes a string and no keysym.
                    None => status = compose::Status::Cancelled,
                }
            }
            if matches!(
                status,
                compose::Status::Composed | compose::Status::Cancelled
            ) {
                compose.reset();
            }
        }
        let direction = match state {
            KeyState::Pressed => xkb::KeyDirection::Down,
            _ => xkb::KeyDirection::Up,
        };
        if self.state.update_key(keycode, direction) & xkb::STATE_LEDS != 0 {
            out.leds = Some(self.leds());
        }
        if state == KeyState::Released
            && let Some(i) = self.held.iter().position(|k| k.keycode == keycode.raw())
        {
            let press = self.held.swap_remove(i);
            out.release = Some(KeyEvent {
                mods: self.mods(),
                repeat: false,
                ..press
            });
        }
        if syms.is_empty()
            || matches!(
                status,
                compose::Status::Composing | compose::Status::Cancelled
            )
        {
            return out;
        }
        let event = self.event(keycode, &syms);
        out.repeat = self.repeat_change(state, &event);
        if state == KeyState::Pressed {
            self.held.retain(|k| k.keycode != event.keycode);
            self.held.push(event.clone());
            out.key = Some(event);
        }
        out
    }

    /// The press's event, from the state as it is now.
    fn event(&self, keycode: Keycode, syms: &[Keysym]) -> KeyEvent {
        KeyEvent {
            keycode: keycode.raw(),
            ascii: self.ascii(keycode, syms),
            unshifted: self.unshifted(keycode),
            shifted: self.shifted(keycode),
            base: base_layout_key(keycode.raw().wrapping_sub(EVDEV_KEYCODE_OFFSET)),
            mods: self.mods(),
            keysyms: syms.iter().map(|s| s.raw()).collect(),
            codepoints: syms
                .iter()
                .map(|&s| char::from_u32(xkb::keysym_to_utf32(s)).filter(|&c| c != '\0'))
                .collect(),
            repeat: false,
        }
    }

    /// Starts the repeat for a press of a repeating key, stops it at the
    /// repeating key's release, and while it repeats takes a change of
    /// modifiers into the repeated keysyms.
    fn repeat_change(&mut self, state: KeyState, event: &KeyEvent) -> Option<RepeatChange> {
        let keymap = self.state.get_keymap();
        let keycode = Keycode::new(event.keycode);
        if let Some(repeat) = &self.repeat
            && repeat.keycode == event.keycode
        {
            if state == KeyState::Released {
                self.repeat = None;
                return Some(RepeatChange::Stop);
            }
            return None;
        }
        if state == KeyState::Pressed && keymap.key_repeats(keycode) {
            self.repeat = Some(event.clone());
            return Some(RepeatChange::Start);
        }
        if self.repeat.is_some() && !keymap.key_repeats(keycode) {
            let held = self.repeat.as_ref().map(|r| Keycode::new(r.keycode))?;
            let syms: Vec<Keysym> = self.state.key_get_syms(held).to_vec();
            if !syms.is_empty() {
                self.repeat = Some(self.event(held, &syms));
            }
        }
        None
    }

    /// The press the repeat timer sends again.
    pub(crate) fn repeated(&self) -> Option<KeyEvent> {
        self.repeat.clone().map(|key| KeyEvent {
            repeat: true,
            ..key
        })
    }

    /// Stops the repeat, as when the device sleeps.
    pub(crate) fn stop_repeat(&mut self) {
        self.repeat = None;
    }

    /// Shift, Lock, Control, Alt and Super, as they apply now; NumLock
    /// while it is on; Hyper and Meta where the keymap gives them a
    /// modifier of their own.
    pub(crate) fn mods(&self) -> Mods {
        let effective = self.state.serialize_mods(xkb::STATE_MODS_EFFECTIVE);
        let special = [
            (self.special.num, Mods::NUM),
            (self.special.hyper, Mods::HYPER),
            (self.special.meta, Mods::META),
        ]
        .into_iter()
        .filter(|&(mask, _)| effective & mask != 0)
        .fold(Mods::empty(), |mods, (_, m)| mods.union(m));
        [
            (xkb::MOD_NAME_SHIFT, Mods::SHIFT),
            (xkb::MOD_NAME_CAPS, Mods::LOCK),
            (xkb::MOD_NAME_CTRL, Mods::CTRL),
            (xkb::MOD_NAME_ALT, Mods::ALT),
            (xkb::MOD_NAME_LOGO, Mods::LOGO),
        ]
        .into_iter()
        .filter(|(name, _)| {
            self.state
                .mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE)
        })
        .fold(special, |mods, (_, m)| mods.union(m))
    }

    /// The ASCII keysym for the shortcut keys: the key's own when it is one
    /// below 128, or else the first layout's that is, so Ctrl+C works on
    /// any layout (shl/misc.h:535-561).
    fn ascii(&self, keycode: Keycode, syms: &[Keysym]) -> u32 {
        if let [only] = syms
            && only.raw() < 128
        {
            return only.raw();
        }
        let keymap = self.state.get_keymap();
        (0..keymap.num_layouts_for_key(keycode))
            .find_map(|layout| {
                let level = self.state.key_get_level(keycode, layout);
                match keymap.key_get_syms_by_level(keycode, layout, level) {
                    [sym] if sym.raw() < 128 => Some(sym.raw()),
                    _ => None,
                }
            })
            .unwrap_or(keysyms::KEY_NoSymbol)
    }

    /// The unshifted key: the one keysym the key makes at the first level
    /// of its layout now, or 0 when the key makes several there, or when a
    /// modifier held chose the key's level that the kitty keyboard
    /// protocol cannot encode, such as AltGr's or the level-five shift's.
    /// Without that rule, AltGr+Q, which makes @ on a German layout, would
    /// be reported as Q, and a program could not tell it from Q (ADR-1005,
    /// D9).
    fn unshifted(&self, keycode: Keycode) -> u32 {
        let consumed = self.state.key_get_consumed_mods(keycode)
            & self.state.serialize_mods(xkb::STATE_MODS_EFFECTIVE);
        if consumed & !self.special.encodable != 0 {
            return keysyms::KEY_NoSymbol;
        }
        let keymap = self.state.get_keymap();
        let layout = self.state.key_get_layout(keycode);
        match keymap.key_get_syms_by_level(keycode, layout, 0) {
            [sym] => unicode_keysym(sym.raw()),
            _ => keysyms::KEY_NoSymbol,
        }
    }

    /// The shifted key: the one keysym the key makes at the level of its
    /// layout that Shift alone chooses, or 0 where no level is Shift's
    /// alone, or where a modifier the kitty keyboard protocol cannot
    /// encode chose the key's level, as [`Self::unshifted`] says.
    fn shifted(&self, keycode: Keycode) -> u32 {
        if self.unshifted(keycode) == keysyms::KEY_NoSymbol {
            return keysyms::KEY_NoSymbol;
        }
        let keymap = self.state.get_keymap();
        let shift = match keymap.mod_get_index(xkb::MOD_NAME_SHIFT) {
            xkb::MOD_INVALID => return keysyms::KEY_NoSymbol,
            i => 1u32.checked_shl(i).unwrap_or(0),
        };
        let layout = self.state.key_get_layout(keycode);
        (0..keymap.num_levels_for_key(keycode, layout))
            .find(|&level| {
                let mut masks = [0; 8];
                let n = keymap.key_get_mods_for_level(keycode, layout, level, &mut masks);
                masks.get(..n).is_some_and(|m| m.contains(&shift))
            })
            .and_then(
                |level| match keymap.key_get_syms_by_level(keycode, layout, level) {
                    [sym] => Some(unicode_keysym(sym.raw())),
                    _ => None,
                },
            )
            .unwrap_or(keysyms::KEY_NoSymbol)
    }

    /// The LEDs the xkb state lights.
    pub(crate) fn leds(&self) -> Leds {
        Leds {
            num_lock: self.state.led_name_is_active(xkb::LED_NAME_NUM),
            caps_lock: self.state.led_name_is_active(xkb::LED_NAME_CAPS),
            scroll_lock: self.state.led_name_is_active(xkb::LED_NAME_SCROLL),
        }
    }

    /// A fresh state on waking, keeping the locked modifiers and layout, so
    /// no key held across a VT switch sticks. Keys
    /// held then are pressed again.
    pub(crate) fn wake_up(&mut self, keymap: &Keymap) -> Leds {
        let locked_mods = self.state.serialize_mods(xkb::STATE_MODS_LOCKED);
        let locked_layout = self.state.serialize_layout(xkb::STATE_LAYOUT_LOCKED);
        self.state = xkb::State::new(&keymap.keymap);
        self.state
            .update_mask(0, 0, locked_mods, 0, 0, locked_layout);
        if let Some(compose) = self.compose.as_mut() {
            compose.reset();
        }
        self.held.clear();
        self.leds()
    }

    /// A state on `keymap`, the new layout's keymap, keeping the locked
    /// modifiers, such as Caps Lock and Num Lock. The locked layout group
    /// belongs to the old keymap, so the new state starts on the new
    /// keymap's first layout. A key repeating stops, and a compose sequence
    /// in progress starts again on the new keymap's compose table. The
    /// keys held stay held, so each still reports its release: the
    /// kernel's key codes do not change with the layout, and a release
    /// carries the keysyms its press carried (ADR-1005 D8).
    pub(crate) fn set_keymap(&mut self, keymap: &Keymap) -> Leds {
        let locked_mods = self.state.serialize_mods(xkb::STATE_MODS_LOCKED);
        let held = std::mem::take(&mut self.held);
        *self = Keyboard::new(keymap);
        self.held = held;
        self.state.update_mask(0, 0, locked_mods, 0, 0, 0);
        self.leds()
    }
}
