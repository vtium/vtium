//! A device as the rules see it: its kind and every fact a match key reads
//! (ADR-1401 D4), with the facts the display guard reads beside them.
//!
//! The crate reads no device itself. vtium fills an [`Identity`] from udev,
//! sysfs, the device and the KMS connector, and `vtiumctl` does the same,
//! so the verdict before install is the verdict at boot.

use std::collections::BTreeMap;

/// Whether a device is a display or an input; a rule names the kind it
/// places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Display,
    Input,
}

impl Kind {
    /// The rule's first word for this kind.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Display => "display",
            Kind::Input => "input",
        }
    }
}

/// udev's input classes, the `ID_INPUT_*` tags of its `input_id` builtin,
/// by the names `class:` takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// `ID_INPUT_KEYBOARD`: every key code 1 to 31, Escape through S.
    Keyboard,
    /// `ID_INPUT_KEY`: any key.
    Key,
    Mouse,
    Touchpad,
    Touchscreen,
    Trackball,
    Pointingstick,
    Tablet,
    TabletPad,
    Joystick,
    Accelerometer,
    Switch,
}

impl Class {
    /// Every class, in the order D4 lists them.
    pub const ALL: [Class; 12] = [
        Class::Keyboard,
        Class::Key,
        Class::Mouse,
        Class::Touchpad,
        Class::Touchscreen,
        Class::Trackball,
        Class::Pointingstick,
        Class::Tablet,
        Class::TabletPad,
        Class::Joystick,
        Class::Accelerometer,
        Class::Switch,
    ];

    /// The name `class:` takes.
    pub fn name(self) -> &'static str {
        match self {
            Class::Keyboard => "keyboard",
            Class::Key => "key",
            Class::Mouse => "mouse",
            Class::Touchpad => "touchpad",
            Class::Touchscreen => "touchscreen",
            Class::Trackball => "trackball",
            Class::Pointingstick => "pointingstick",
            Class::Tablet => "tablet",
            Class::TabletPad => "tablet-pad",
            Class::Joystick => "joystick",
            Class::Accelerometer => "accelerometer",
            Class::Switch => "switch",
        }
    }

    /// The class `class:NAME` names.
    pub fn from_name(name: &str) -> Option<Class> {
        Class::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Whether the class is a pointer's, as the shipped rules send to the
    /// focus beside the keyboards (D3).
    pub fn is_pointer(self) -> bool {
        matches!(
            self,
            Class::Mouse
                | Class::Touchpad
                | Class::Touchscreen
                | Class::Trackball
                | Class::Pointingstick
        )
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// The classes one device has; udev may tag a device with several.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Classes(u16);

impl Classes {
    /// These classes and `class`.
    #[must_use]
    pub fn with(self, class: Class) -> Classes {
        Classes(self.0 | class.bit())
    }

    pub fn has(self, class: Class) -> bool {
        self.0 & class.bit() != 0
    }

    /// Whether any class is a pointer's.
    pub fn any_pointer(self) -> bool {
        Class::ALL
            .into_iter()
            .any(|c| c.is_pointer() && self.has(c))
    }
}

/// A USB unit's identity, from udev's `usb_id` builtin: `ID_VENDOR_ID`,
/// `ID_MODEL_ID` and `ID_SERIAL_SHORT`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsbSerial {
    pub vendor: u16,
    pub product: u16,
    pub serial: String,
}

/// A monitor's identity from its EDID: the PNP maker id, the product code
/// and the serial string, else the serial number.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edid {
    pub maker: String,
    pub product: u16,
    pub serial: String,
}

/// One device: a display or an input, and what each match key reads.
///
/// A fact the device does not have is `None` (or empty), and a key that
/// reads it matches nothing. Without a udev daemon `path`, `serial`,
/// `card`, `connector`'s card and `properties` are empty (D4).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub kind: Kind,
    /// An input's classes: udev's tags, or vtium's own `input_id` rule
    /// where no daemon runs.
    pub classes: Classes,
    /// udev's `ID_PATH` of an input's interface.
    pub path: Option<String>,
    pub serial: Option<UsbSerial>,
    /// The unique id the device reports, a Bluetooth device's address.
    pub unique: Option<String>,
    /// `VENDOR:PRODUCT` as the device reports them.
    pub id: Option<(u16, u16)>,
    /// The device's name as it reports it.
    pub name: Option<String>,
    /// Every udev property of the device, which `udev:` reads.
    pub properties: BTreeMap<String, String>,
    pub edid: Option<Edid>,
    /// A display's connector name as the kernel gives it (`DP-8`).
    pub connector: Option<String>,
    /// A display's KMS connector `PATH` property (`mst:...`).
    pub connector_path: Option<String>,
    /// A display's card's udev `ID_PATH`.
    pub card: Option<String>,
    /// Whether a display's connector was lit at start; the display guard
    /// reads it, never a rule.
    pub lit_at_start: bool,
    /// Whether a display is on the boot card (`boot_vga`); the display
    /// guard reads it, never a rule.
    pub on_boot_card: bool,
    /// A display's card's place in the kernel's order of cards (its minor
    /// number); the display guard reads it, never a rule, since a card's
    /// number can change between boots.
    pub card_order: u32,
    /// A display's place in its card's connector list, the kernel's order;
    /// the display guard reads it, never a rule.
    pub order: u32,
}

impl Identity {
    fn of(kind: Kind) -> Identity {
        Identity {
            kind,
            classes: Classes::default(),
            path: None,
            serial: None,
            unique: None,
            id: None,
            name: None,
            properties: BTreeMap::new(),
            edid: None,
            connector: None,
            connector_path: None,
            card: None,
            lit_at_start: false,
            on_boot_card: false,
            card_order: 0,
            order: 0,
        }
    }

    /// A display with no facts yet.
    pub fn display() -> Identity {
        Identity::of(Kind::Display)
    }

    /// An input with no facts yet.
    pub fn input() -> Identity {
        Identity::of(Kind::Input)
    }

    /// This identity with `class` among its classes.
    #[must_use]
    pub fn with_class(mut self, class: Class) -> Identity {
        self.classes = self.classes.with(class);
        self
    }

    /// Whether the anti-lockout guard counts the device as a keyboard:
    /// `class:keyboard`, unless udev marks it `VTIUM_ROLE=token` or
    /// `VTIUM_ROLE=numpad` (D9).
    pub fn counts_as_keyboard(&self) -> bool {
        self.classes.has(Class::Keyboard)
            && !matches!(
                self.properties.get("VTIUM_ROLE").map(String::as_str),
                Some("token" | "numpad")
            )
    }
}
