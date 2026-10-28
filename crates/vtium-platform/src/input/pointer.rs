//! The pointer: a mouse's relative motion, a touchpad's or touchscreen's
//! absolute motion, buttons with double clicks, and the wheel
//! (input_pointer.c).

use std::time::{Duration, Instant};

use super::{Button, PointerEvent};
use crate::ioctl::{
    ABS_X, ABS_Y, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_TOOL_DOUBLETAP, BTN_TOOL_TRIPLETAP,
    BTN_TOUCH, REL_WHEEL, REL_X, REL_Y,
};

/// Two left presses closer than this are a double click
/// (input_pointer.c:193).
pub(crate) const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// What moves the pointer (input.c:279-292).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    None,
    Mouse,
    Touchpad,
    Touchscreen,
    /// Absolute axes without touch, as a virtual machine's tablet.
    VirtualMouse,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::None => "",
            Kind::Mouse => "Mouse",
            Kind::Touchpad => "Touchpad",
            Kind::Touchscreen => "Touchscreen",
            Kind::VirtualMouse => "Virtual mouse",
        }
    }
}

/// An absolute axis's range, `EVIOCGABS`'s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Range {
    pub(crate) min: i32,
    pub(crate) max: i32,
}

/// One device's pointer.
#[derive(Debug)]
pub(crate) struct Pointer {
    pub(crate) kind: Kind,
    x: i32,
    y: i32,
    /// A finger is on the touchpad.
    touching: bool,
    /// The next absolute value after a touch starts sets the offset, so
    /// the pointer moves from where it is.
    sync_x: bool,
    sync_y: bool,
    off_x: i32,
    off_y: i32,
    pub(crate) range_x: Range,
    pub(crate) range_y: Range,
    held: Option<Button>,
    last_click: Option<Instant>,
}

impl Pointer {
    pub(crate) fn new(kind: Kind) -> Pointer {
        Pointer {
            kind,
            x: 0,
            y: 0,
            touching: false,
            sync_x: false,
            sync_y: false,
            off_x: 0,
            off_y: 0,
            range_x: Range::default(),
            range_y: Range::default(),
            held: None,
            last_click: None,
        }
    }

    /// The device closed, as the devices sleep: a button or a finger
    /// down now may come up while it is closed, so neither is held when
    /// it opens again.
    pub(crate) fn let_go(&mut self) {
        self.held = None;
        self.touching = false;
    }

    fn moved(&self) -> PointerEvent {
        PointerEvent::Moved {
            x: self.x,
            y: self.y,
            held: self.held,
        }
    }

    /// `EV_REL`: motion clamped to the screen, `max` its last pixel, and
    /// the wheel.
    pub(crate) fn rel(&mut self, code: u16, value: i32, max: (i32, i32)) -> Option<PointerEvent> {
        match code {
            REL_X => {
                self.x = self.x.saturating_add(value).clamp(0, max.0.max(0));
                Some(self.moved())
            }
            REL_Y => {
                self.y = self.y.saturating_add(value).clamp(0, max.1.max(0));
                Some(self.moved())
            }
            REL_WHEEL => Some(PointerEvent::Wheel(value)),
            _ => None,
        }
    }

    /// `EV_ABS`: a touchpad moves the pointer by the finger's motion from
    /// where it is; a touchscreen or a virtual mouse puts it where the axis
    /// says, scaled to the screen.
    pub(crate) fn abs(&mut self, code: u16, value: i32, max: (i32, i32)) -> Option<PointerEvent> {
        match code {
            ABS_X => {
                let (range, max) = (self.range_x, max.0);
                let (pos, off, sync) = (&mut self.x, &mut self.off_x, &mut self.sync_x);
                axis(self.kind, self.touching, range, max, value, pos, off, sync)?;
            }
            ABS_Y => {
                let (range, max) = (self.range_y, max.1);
                let (pos, off, sync) = (&mut self.y, &mut self.off_y, &mut self.sync_y);
                axis(self.kind, self.touching, range, max, value, pos, off, sync)?;
            }
            _ => return None,
        }
        Some(self.moved())
    }

    /// `EV_KEY` for a button: press and release, a left press soon after
    /// the last a double click, and a touch starting or ending.
    pub(crate) fn button(&mut self, code: u16, value: i32, now: Instant) -> Option<PointerEvent> {
        if value > 1 {
            return None;
        }
        let pressed = value == 1;
        let (button, double_click) = match code {
            BTN_LEFT => {
                let double = pressed
                    && self
                        .last_click
                        .is_some_and(|last| now.saturating_duration_since(last) < DOUBLE_CLICK);
                if pressed {
                    self.last_click = Some(now);
                }
                (Button::Left, double)
            }
            BTN_RIGHT => (Button::Right, false),
            BTN_MIDDLE | BTN_TOOL_DOUBLETAP | BTN_TOOL_TRIPLETAP => (Button::Middle, false),
            BTN_TOUCH => {
                self.touching = pressed;
                if pressed {
                    self.sync_x = true;
                    self.sync_y = true;
                }
                return None;
            }
            _ => return None,
        };
        self.held = pressed.then_some(button);
        Some(PointerEvent::Button {
            button,
            pressed,
            double_click,
        })
    }
}

/// One absolute axis: `pos` moves, `off` and `sync` are a touchpad's.
/// `None` when nothing moves.
#[expect(clippy::too_many_arguments, reason = "one axis's state, in place")]
fn axis(
    kind: Kind,
    touching: bool,
    range: Range,
    max: i32,
    value: i32,
    pos: &mut i32,
    off: &mut i32,
    sync: &mut bool,
) -> Option<()> {
    match kind {
        Kind::Touchpad => {
            if std::mem::take(sync) {
                *off = pos.saturating_sub(value);
            }
            if touching {
                *pos = off.saturating_add(value);
                if *pos < 0 {
                    *pos = 0;
                    *off = value.saturating_neg();
                }
                if *pos > max {
                    *pos = max;
                    *off = max.saturating_sub(value);
                }
            }
        }
        Kind::Touchscreen | Kind::VirtualMouse => {
            // An axis whose range is one value moves nothing; the C
            // reference divided by zero.
            let span = i64::from(range.max).checked_sub(i64::from(range.min))?;
            let scaled = i64::from(value)
                .saturating_sub(i64::from(range.min))
                .saturating_mul(i64::from(max))
                .checked_div(span)?;
            *pos = i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX });
        }
        Kind::Mouse | Kind::None => return None,
    }
    Some(())
}
