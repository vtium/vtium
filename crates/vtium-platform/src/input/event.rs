//! What the input devices tell their owner: keys, pointer events, and
//! the device each came from.

/// The modifiers held with a key, as the C reference numbered them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const SHIFT: Mods = Mods(1);
    pub const LOCK: Mods = Mods(2);
    pub const CTRL: Mods = Mods(4);
    pub const ALT: Mods = Mods(8);
    pub const LOGO: Mods = Mods(16);

    pub const fn empty() -> Mods {
        Mods(0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
}

/// A key press, or its repeat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// xkb's key code: evdev's plus 8.
    pub keycode: u32,
    /// The ASCII keysym for shortcuts, or 0 (`NoSymbol`).
    pub ascii: u32,
    pub mods: Mods,
    /// The keysyms the key makes; a compose sequence's result replaces
    /// them.
    pub keysyms: Vec<u32>,
    /// Each keysym's character, where it has one.
    pub codepoints: Vec<Option<char>>,
    /// Made by the repeat timer of a held key.
    pub repeat: bool,
}

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEvent {
    /// The pointer is at `x`, `y`, with `held` pressed.
    Moved {
        x: i32,
        y: i32,
        held: Option<Button>,
    },
    Button {
        button: Button,
        pressed: bool,
        double_click: bool,
    },
    /// Wheel steps, positive up.
    Wheel(i32),
    /// The end of one evdev frame.
    Sync,
    /// No pointer event for [`super::POINTER_HIDE`]: hide the pointer.
    Hide,
}

/// What the input devices tell their owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
    Pointer(PointerEvent),
}

/// An event with the device it came from, as [`super::Input::device_events`]
/// drains them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInput {
    /// `None` for an event of no one device, the pointer's hide.
    pub device: Option<DeviceId>,
    /// The rescue key, pressed on a device watched for it alone.
    pub rescue: bool,
    pub event: InputEvent,
}

/// What vtium does with a device, as a head file's rules place it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Opened, its events delivered, whatever it reports.
    Deliver,
    /// Opened to watch for the rescue key alone: a press of the VT's own
    /// key is drained marked [`DeviceInput::rescue`], and nothing else it
    /// sends is delivered (ADR-1401 D9).
    Rescue,
    /// Never opened.
    Closed,
}

/// A device's handle in [`super::Input`], never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(pub(super) u64);
