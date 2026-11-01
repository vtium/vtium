//! Keys through libxkbcommon: the keymap and compose table the seat
//! shares, and each keyboard's own state, compose state and repeat
//! (input_uxkb.c).

use std::ffi::OsStr;
use std::path::Path;

use xkbcommon::xkb::{self, Keycode, Keysym, compose, keysyms};

use super::{KeyEvent, Mods};
use crate::ioctl::{LED_CAPSL, LED_NUML, LED_SCROLLL};

/// evdev's key codes start 8 below xkb's.
const EVDEV_KEYCODE_OFFSET: u32 = 8;

/// The keymap the C reference built in when nothing else compiled
/// (input_fallback.xkb).
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

/// The locale for compose, found as locale(7) says without setting it:
/// `LC_ALL`, then `LC_CTYPE`, then `LANG`, then `C` (seat.c:639-657).
pub fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .unwrap_or_else(|| String::from("C"))
}

/// The seat's keymap and compose table, which every keyboard shares.
pub struct Keymap {
    context: xkb::Context,
    keymap: xkb::Keymap,
    compose: Option<compose::Table>,
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
    /// system's default names, else the built-in fallback
    /// (input_uxkb.c:89-156). `None` only when even the fallback fails.
    pub fn new(config: &KeymapConfig) -> Option<Keymap> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = config
            .keymap_file
            .as_deref()
            .and_then(|path| from_file(&context, path))
            .or_else(|| from_names(&context, config))
            .or_else(|| {
                log::warn!(
                    target: "input",
                    "cannot compile the keymap ({}, {}, {}, {}), taking the system's default",
                    config.model, config.layout, config.variant, config.options
                );
                from_names(&context, &KeymapConfig::default())
            })
            .or_else(|| {
                log::warn!(target: "input", "cannot compile the default keymap, taking the built-in one");
                fallback(&context)
            })?;
        let mut keymap = Keymap {
            context,
            keymap,
            compose: None,
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
        })
    }

    /// The compose table: the compose file, else the locale's, else none,
    /// which turns compose off (input_uxkb.c:158-189).
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
    /// The `EV_LED` codes and values, in the order the C reference wrote
    /// them for the xkb state (input_uxkb.c:260-288).
    pub(crate) fn codes(self) -> [(u16, bool); 3] {
        [
            (LED_NUML, self.num_lock),
            (LED_CAPSL, self.caps_lock),
            (LED_SCROLLL, self.scroll_lock),
        ]
    }
}

/// One keyboard's state: modifiers held on it apply to it alone.
pub(crate) struct Keyboard {
    state: xkb::State,
    compose: Option<compose::State>,
    /// The press repeating, while it repeats.
    repeat: Option<KeyEvent>,
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
        }
    }

    /// A key from evdev: `code` is evdev's, `state` its value
    /// (input_uxkb.c:431-545). The keysyms are read before the state
    /// changes; compose sees presses; a press finishing no compose sequence
    /// is dispatched, and starts or stops the repeat.
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
            out.key = Some(event);
        }
        out
    }

    /// The press's event, from the state as it is now.
    fn event(&self, keycode: Keycode, syms: &[Keysym]) -> KeyEvent {
        KeyEvent {
            keycode: keycode.raw(),
            ascii: self.ascii(keycode, syms),
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
    /// modifiers into the repeated keysyms (input_uxkb.c:381-429).
    fn repeat_change(&mut self, state: KeyState, event: &KeyEvent) -> Option<RepeatChange> {
        let keymap = self.state.get_keymap();
        let keycode = Keycode::new(event.keycode);
        if let Some(repeat) = &self.repeat {
            if repeat.keycode == event.keycode {
                if state == KeyState::Released {
                    self.repeat = None;
                    return Some(RepeatChange::Stop);
                }
                return None;
            }
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

    /// Shift, Lock, Control, Alt and Super, as they apply now.
    pub(crate) fn mods(&self) -> Mods {
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
        .fold(Mods::empty(), |mods, (_, m)| mods.union(m))
    }

    /// The ASCII keysym for the shortcut keys: the key's own when it is one
    /// below 128, or else the first layout's that is, so Ctrl+C works on
    /// any layout (shl/misc.h:535-561).
    fn ascii(&self, keycode: Keycode, syms: &[Keysym]) -> u32 {
        if let [only] = syms {
            if only.raw() < 128 {
                return only.raw();
            }
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

    /// The LEDs the xkb state lights.
    pub(crate) fn leds(&self) -> Leds {
        Leds {
            num_lock: self.state.led_name_is_active(xkb::LED_NAME_NUM),
            caps_lock: self.state.led_name_is_active(xkb::LED_NAME_CAPS),
            scroll_lock: self.state.led_name_is_active(xkb::LED_NAME_SCROLL),
        }
    }

    /// A fresh state on waking, keeping the locked modifiers and layout, so
    /// no key held across a VT switch sticks (input_uxkb.c:555-585). Keys
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
        self.leds()
    }
}
