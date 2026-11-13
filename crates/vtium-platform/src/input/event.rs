//! What the input devices tell their owner: keys, pointer events, and
//! the device each came from.

/// The modifiers held with a key, one bit each. The bits are those of
/// vtium-term's own modifiers, so the binary hands them on unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const SHIFT: Mods = Mods(1);
    pub const LOCK: Mods = Mods(2);
    pub const CTRL: Mods = Mods(4);
    pub const ALT: Mods = Mods(8);
    pub const LOGO: Mods = Mods(16);
    /// NumLock is on: a state, as Lock is.
    pub const NUM: Mods = Mods(32);
    /// Hyper, where the keymap gives it a modifier of its own, one that
    /// neither Alt nor Super uses (ADR-1005, D8).
    pub const HYPER: Mods = Mods(64);
    /// Meta, where the keymap gives it a modifier of its own, one that
    /// neither Alt nor Super uses (ADR-1005, D8).
    pub const META: Mods = Mods(128);

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

/// A key press, its repeat, or its release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// xkb's key code: evdev's plus 8.
    pub keycode: u32,
    /// The ASCII keysym for shortcuts, or 0 (`NoSymbol`).
    pub ascii: u32,
    /// The keysym the key makes at the first shift level of the active
    /// layout, which is the unshifted key, or 0 (`NoSymbol`) when a
    /// modifier the kitty keyboard protocol cannot encode, such as AltGr,
    /// chose the key's symbol (ADR-1005, D9). A keysym outside Latin-1 that
    /// stands for a character is given as that character's Unicode keysym,
    /// 0x1000000 plus the code point, which names the same symbol.
    pub unshifted: u32,
    /// The character the key types on the US PC-101 layout, which is the
    /// base layout key, from the kernel's key code; `None` for a key that
    /// layout does not type a character with (ADR-1005, D9).
    pub base: Option<char>,
    pub mods: Mods,
    /// The keysyms the key makes; a compose sequence's result replaces
    /// them. A release carries its press's.
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
    /// A key pressed, or repeated by the repeat timer.
    Key(KeyEvent),
    /// A key let go whose press came as a `Key` (ADR-1005, D8). It carries
    /// the keysyms its press carried, and the modifiers after the release,
    /// so the release of Shift holds no Shift.
    Release(KeyEvent),
    Pointer(PointerEvent),
    /// A keyboard's effective modifiers changed: a modifier key was
    /// pressed or let go, Caps Lock toggled, or the keyboard closed or
    /// opened, a closed keyboard holding none. It carries the modifiers
    /// that one keyboard holds after the change, not the seat's; an owner
    /// combines a head's through [`super::Input::mods_of`]. A key press
    /// still comes as its own `Key`, before this.
    Mods(Mods),
}

/// An event with the device it came from, as [`super::Input::device_events`]
/// drains them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInput {
    /// `None` for an event of no one device, the pointer's hide.
    pub device: Option<DeviceId>,
    /// A VT key, the rescue key among them, pressed on a device watched
    /// for the VT keys alone.
    pub rescue: bool,
    pub event: InputEvent,
}

/// What vtium does with a device, as a head file's rules place it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Opened, its events delivered, whatever it reports.
    Deliver,
    /// Opened to watch for the VT keys alone (ADR-1401 D3, "Who switches
    /// VTs", and D9): a press of `Ctrl-Alt-F<n>` or `XF86Switch_VT_<n>`
    /// while the VT is ours, the VT's own key, the rescue key, among them,
    /// is drained marked [`DeviceInput::rescue`], and nothing else it
    /// sends is delivered.
    Rescue,
    /// Never opened.
    Closed,
}

/// A device's handle in [`super::Input`], never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(pub(super) u64);
