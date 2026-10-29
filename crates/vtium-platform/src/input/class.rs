//! An input device's classes, as udev names them (ADR-1401 D4).
//!
//! Where udev runs, its `input_id` builtin has tagged the device
//! (`ID_INPUT=1`), and the `ID_INPUT_*` tags it and hwdb left are the
//! device's classes, so a hwdb correction applies to vtium as it does to
//! libinput and the desktop ([`Classes::from_properties`]). Without a udev
//! daemon the device carries no tags, and vtium applies udev's rule itself
//! to the capability bitmaps sysfs shows, so the device is classed without
//! being opened ([`Classes::from_bitmaps`]). The rule follows the record's
//! account of udev's: a keyboard declares every key code 1 to 31, Escape
//! through S; a key device declares any key; the pointer classes follow the
//! kernel's device types (`Documentation/input/event-codes.rst`).

#[cfg(test)]
mod tests;

use crate::ioctl::{
    ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_PRESSURE, ABS_RX, ABS_WHEEL, ABS_X, ABS_Y, AbsBits,
    BTN_1, BTN_DIGI, BTN_DPAD_RIGHT, BTN_DPAD_UP, BTN_JOYSTICK, BTN_LEFT, BTN_MISC, BTN_STYLUS,
    BTN_TASK, BTN_TOOL_FINGER, BTN_TOUCH, BTN_TRIGGER_HAPPY, BTN_TRIGGER_HAPPY40, EV_ABS, EV_KEY,
    EV_REL, EV_SW, EvBits, INPUT_PROP_ACCELEROMETER, INPUT_PROP_DIRECT, INPUT_PROP_POINTING_STICK,
    KEY_ALS_TOGGLE, KEY_ESC, KEY_OK, KEY_S, KeyBits, PropBits, REL_WHEEL, REL_X, REL_Y, RelBits,
    bit_is_set,
};

/// One of udev's input classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Keyboard,
    Key,
    Mouse,
    Touchpad,
    Touchscreen,
    Trackball,
    PointingStick,
    Tablet,
    TabletPad,
    Joystick,
    Accelerometer,
    Switch,
}

impl Class {
    /// Every class, in the record's order.
    pub const ALL: [Class; 12] = [
        Class::Keyboard,
        Class::Key,
        Class::Mouse,
        Class::Touchpad,
        Class::Touchscreen,
        Class::Trackball,
        Class::PointingStick,
        Class::Tablet,
        Class::TabletPad,
        Class::Joystick,
        Class::Accelerometer,
        Class::Switch,
    ];

    /// The name a head file's `class:` takes.
    pub const fn name(self) -> &'static str {
        match self {
            Class::Keyboard => "keyboard",
            Class::Key => "key",
            Class::Mouse => "mouse",
            Class::Touchpad => "touchpad",
            Class::Touchscreen => "touchscreen",
            Class::Trackball => "trackball",
            Class::PointingStick => "pointingstick",
            Class::Tablet => "tablet",
            Class::TabletPad => "tablet-pad",
            Class::Joystick => "joystick",
            Class::Accelerometer => "accelerometer",
            Class::Switch => "switch",
        }
    }

    /// The udev property that tags it.
    pub const fn property(self) -> &'static str {
        match self {
            Class::Keyboard => "ID_INPUT_KEYBOARD",
            Class::Key => "ID_INPUT_KEY",
            Class::Mouse => "ID_INPUT_MOUSE",
            Class::Touchpad => "ID_INPUT_TOUCHPAD",
            Class::Touchscreen => "ID_INPUT_TOUCHSCREEN",
            Class::Trackball => "ID_INPUT_TRACKBALL",
            Class::PointingStick => "ID_INPUT_POINTINGSTICK",
            Class::Tablet => "ID_INPUT_TABLET",
            Class::TabletPad => "ID_INPUT_TABLET_PAD",
            Class::Joystick => "ID_INPUT_JOYSTICK",
            Class::Accelerometer => "ID_INPUT_ACCELEROMETER",
            Class::Switch => "ID_INPUT_SWITCH",
        }
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A device's classes: none, one, or several, as a keyboard is also a key
/// device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Classes(u16);

impl Classes {
    pub const fn empty() -> Classes {
        Classes(0)
    }

    pub const fn contains(self, class: Class) -> bool {
        self.0 & class.bit() != 0
    }

    #[must_use]
    pub const fn with(self, class: Class) -> Classes {
        Classes(self.0 | class.bit())
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its classes, in the record's order.
    pub fn iter(self) -> impl Iterator<Item = Class> {
        Class::ALL.into_iter().filter(move |&c| self.contains(c))
    }

    /// The classes udev tagged, read through `property`; `None` when udev's
    /// `input_id` has not run on the device (no `ID_INPUT=1`), as without a
    /// udev daemon. A tag counts when its value is `1`, so a hwdb entry
    /// setting one to `0` takes the class away.
    pub fn from_properties<'a>(property: impl Fn(&str) -> Option<&'a str>) -> Option<Classes> {
        if property("ID_INPUT") != Some("1") {
            return None;
        }
        Some(
            Class::ALL
                .into_iter()
                .filter(|c| property(c.property()) == Some("1"))
                .fold(Classes::empty(), Classes::with),
        )
    }

    /// The classes udev's rule gives a device with these bitmaps.
    pub(crate) fn from_bitmaps(b: &Bitmaps) -> Classes {
        rule(b)
    }
}

/// A device's capability bitmaps, in the kernel's layout: what sysfs shows
/// under `capabilities/` and in `properties`, and what `EVIOCGBIT` and
/// `EVIOCGPROP` fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Bitmaps {
    pub(crate) ev: EvBits,
    pub(crate) key: KeyBits,
    pub(crate) rel: RelBits,
    pub(crate) abs: AbsBits,
    pub(crate) props: PropBits,
}

impl Bitmaps {
    /// The bitmaps sysfs shows for an input device, each attribute read
    /// through `attribute`; one it cannot read is empty.
    pub(crate) fn from_sysfs(attribute: impl Fn(&str) -> Option<String>) -> Bitmaps {
        let mut b = Bitmaps::default();
        let read = |name: &str, out: &mut [usize]| {
            if let Some(text) = attribute(name) {
                parse_bitmap(&text, out);
            }
        };
        read("capabilities/ev", &mut b.ev);
        read("capabilities/key", &mut b.key);
        read("capabilities/rel", &mut b.rel);
        read("capabilities/abs", &mut b.abs);
        read("properties", &mut b.props);
        b
    }
}

/// Fills `out` from a sysfs bitmap: hex words separated by spaces, the most
/// significant first, each as wide as the kernel's `long`. The width is
/// vtium's `usize`, as on the targets vtium builds for, unless a word is
/// wider than 32 bits, which a 64-bit kernel under a 32-bit vtium shows.
/// Bits past `out` and words that are not hex are dropped.
pub(crate) fn parse_bitmap(text: &str, out: &mut [usize]) {
    let words: Vec<u64> = text
        .split_ascii_whitespace()
        .rev()
        .map(|w| u64::from_str_radix(w, 16).unwrap_or(0))
        .collect();
    let width: u32 = if usize::BITS == 64 || words.iter().any(|&w| w > u64::from(u32::MAX)) {
        64
    } else {
        32
    };
    for (index, word) in words.into_iter().enumerate() {
        for bit in 0..width {
            if word.checked_shr(bit).is_some_and(|w| w & 1 == 1) {
                let at = usize::try_from(width)
                    .ok()
                    .and_then(|w| index.checked_mul(w))
                    .and_then(|at| at.checked_add(usize::try_from(bit).ok()?));
                if let Some(at) = at {
                    set_bit(out, at);
                }
            }
        }
    }
}

fn set_bit(bits: &mut [usize], bit: usize) {
    let long = usize::BITS as usize;
    let (Some(word), Some(shift)) = (bit.checked_div(long), bit.checked_rem(long)) else {
        return;
    };
    if let Some(w) = bits.get_mut(word) {
        *w |= 1usize
            .checked_shl(u32::try_from(shift).unwrap_or(u32::MAX))
            .unwrap_or(0);
    }
}

/// Whether any code from `from` up to, not including, `to` is set.
fn any(bits: &[usize], from: u16, to: u16) -> bool {
    (from..to).any(|code| bit_is_set(bits, code))
}

/// Whether the device declares a key, a `KEY_*` code rather than a button:
/// below the buttons, or in the key ranges above them.
pub(crate) fn any_key(key: &KeyBits) -> bool {
    any(key, 1, BTN_MISC)
        || any(key, KEY_OK, BTN_DPAD_UP)
        || any(key, KEY_ALS_TOGGLE, BTN_TRIGGER_HAPPY)
}

/// Whether the device declares every key from Escape through S.
pub(crate) fn full_keyboard(key: &KeyBits) -> bool {
    (KEY_ESC..=KEY_S).all(|code| bit_is_set(key, code))
}

/// udev's rule over the bitmaps. A device is a switch for `EV_SW`, a
/// keyboard and a key device by its keys, and at most one kind of pointer:
/// an accelerometer by its property, else a tablet by its pen, a touchpad
/// by a finger tool without `INPUT_PROP_DIRECT`, a mouse by its buttons
/// with relative axes or none, or with absolute axes and no touch (a
/// virtual machine's tablet), a touchscreen by touch or
/// `INPUT_PROP_DIRECT`, a joystick by its buttons or axes; a pointing
/// stick by its property, and a tablet's pad by its numbered buttons.
fn rule(b: &Bitmaps) -> Classes {
    let ev = |t: u16| bit_is_set(&b.ev, t);
    let key = |c: u16| ev(EV_KEY) && bit_is_set(&b.key, c);
    let abs = |c: u16| ev(EV_ABS) && bit_is_set(&b.abs, c);
    let rel = |c: u16| ev(EV_REL) && bit_is_set(&b.rel, c);
    let prop = |p: u16| bit_is_set(&b.props, p);
    let mut classes = Classes::empty();
    let mut add = |class: Class, when: bool| {
        if when {
            classes = classes.with(class);
        }
    };
    add(Class::Switch, ev(EV_SW));
    if ev(EV_KEY) {
        add(Class::Keyboard, full_keyboard(&b.key));
        add(Class::Key, any_key(&b.key));
    }
    if prop(INPUT_PROP_ACCELEROMETER) {
        add(Class::Accelerometer, true);
        return classes;
    }
    let abs_xy = abs(ABS_X) && abs(ABS_Y);
    let placed = abs_xy || (abs(ABS_MT_POSITION_X) && abs(ABS_MT_POSITION_Y));
    let rel_xy = rel(REL_X) && rel(REL_Y);
    let pen = key(BTN_DIGI) || key(BTN_STYLUS);
    let finger = key(BTN_TOOL_FINGER) && !pen;
    let direct = prop(INPUT_PROP_DIRECT);
    let touch = key(BTN_TOUCH);
    let buttons = ev(EV_KEY) && (BTN_LEFT..=BTN_TASK).any(|c| bit_is_set(&b.key, c));
    let joystick_buttons = ev(EV_KEY)
        && (any(&b.key, BTN_JOYSTICK, BTN_DIGI)
            || any(&b.key, BTN_DPAD_UP, BTN_DPAD_RIGHT.saturating_add(1))
            || any(
                &b.key,
                BTN_TRIGGER_HAPPY,
                BTN_TRIGGER_HAPPY40.saturating_add(1),
            ));
    let joystick_axes = ev(EV_ABS) && any(&b.abs, ABS_RX, ABS_PRESSURE);
    let tablet = placed && pen;
    let touchpad = placed && finger && !direct;
    let mouse = !tablet && !touchpad && buttons && (rel_xy || !abs_xy || (!touch && !direct));
    let touchscreen = placed && !tablet && !touchpad && !mouse && (touch || direct);
    let joystick =
        !tablet && !touchpad && !mouse && !touchscreen && (joystick_buttons || joystick_axes);
    let pad = key(BTN_MISC)
        && key(BTN_1)
        && !pen
        && !rel_xy
        && (placed || abs(ABS_WHEEL) || rel(REL_WHEEL));
    add(Class::Tablet, tablet || pad);
    add(Class::TabletPad, pad);
    add(Class::Touchpad, touchpad);
    add(Class::Mouse, mouse);
    add(Class::Touchscreen, touchscreen);
    add(Class::Joystick, joystick);
    add(Class::PointingStick, prop(INPUT_PROP_POINTING_STICK));
    classes
}
