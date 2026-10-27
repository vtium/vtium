//! The ioctls rustix does not wrap: one definition each, through rustix's
//! `ioctl` module (vtium/vtium#6).
//!
//! Each request number is the kernel header's, and
//! `request_numbers_match_the_headers` reads the headers and fails when one
//! differs. The fbdev, VT and keyboard numbers predate size encoding, so
//! nothing else would catch a wrong one. DRM's ioctls come wrapped from
//! `drm-ffi`.

use std::ffi::{c_int, c_short};
use std::io;
use std::os::fd::BorrowedFd;

use rustix::ioctl::{Getter, IntegerSetter, Opcode, Setter, Updater, ioctl, opcode};

/// `FBIOGET_VSCREENINFO`, `linux/fb.h`.
const FBIOGET_VSCREENINFO: Opcode = 0x4600;
/// `FBIOPUT_VSCREENINFO`, `linux/fb.h`.
const FBIOPUT_VSCREENINFO: Opcode = 0x4601;
/// `FBIOGET_FSCREENINFO`, `linux/fb.h`.
const FBIOGET_FSCREENINFO: Opcode = 0x4602;
/// `FBIOBLANK`, `linux/fb.h`.
const FBIOBLANK: Opcode = 0x4611;

/// `FB_ACTIVATE_NOW`, `linux/fb.h`.
pub(crate) const FB_ACTIVATE_NOW: u32 = 0;
/// `FB_ACTIVATE_FORCE`, `linux/fb.h`.
pub(crate) const FB_ACTIVATE_FORCE: u32 = 128;
/// `FB_VISUAL_TRUECOLOR`, `linux/fb.h`.
pub(crate) const FB_VISUAL_TRUECOLOR: u32 = 2;
/// `FB_BLANK_UNBLANK`, `linux/fb.h`: `VESA_NO_BLANKING`.
pub(crate) const FB_BLANK_UNBLANK: u32 = 0;
/// `FB_BLANK_NORMAL`, `linux/fb.h`: `VESA_NO_BLANKING + 1`.
pub(crate) const FB_BLANK_NORMAL: u32 = 1;
/// `FB_BLANK_POWERDOWN`, `linux/fb.h`: `VESA_POWERDOWN + 1`.
pub(crate) const FB_BLANK_POWERDOWN: u32 = 4;

/// `struct fb_bitfield`, `linux/fb.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FbBitfield {
    pub(crate) offset: u32,
    pub(crate) length: u32,
    pub(crate) msb_right: u32,
}

/// `struct fb_var_screeninfo`, `linux/fb.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FbVarScreeninfo {
    pub(crate) xres: u32,
    pub(crate) yres: u32,
    pub(crate) xres_virtual: u32,
    pub(crate) yres_virtual: u32,
    pub(crate) xoffset: u32,
    pub(crate) yoffset: u32,
    pub(crate) bits_per_pixel: u32,
    pub(crate) grayscale: u32,
    pub(crate) red: FbBitfield,
    pub(crate) green: FbBitfield,
    pub(crate) blue: FbBitfield,
    pub(crate) transp: FbBitfield,
    pub(crate) nonstd: u32,
    pub(crate) activate: u32,
    pub(crate) height: u32,
    pub(crate) width: u32,
    pub(crate) accel_flags: u32,
    pub(crate) pixclock: u32,
    pub(crate) left_margin: u32,
    pub(crate) right_margin: u32,
    pub(crate) upper_margin: u32,
    pub(crate) lower_margin: u32,
    pub(crate) hsync_len: u32,
    pub(crate) vsync_len: u32,
    pub(crate) sync: u32,
    pub(crate) vmode: u32,
    pub(crate) rotate: u32,
    pub(crate) colorspace: u32,
    pub(crate) reserved: [u32; 4],
}

/// `struct fb_fix_screeninfo`, `linux/fb.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FbFixScreeninfo {
    pub(crate) id: [u8; 16],
    pub(crate) smem_start: usize,
    pub(crate) smem_len: u32,
    pub(crate) kind: u32,
    pub(crate) type_aux: u32,
    pub(crate) visual: u32,
    pub(crate) xpanstep: u16,
    pub(crate) ypanstep: u16,
    pub(crate) ywrapstep: u16,
    pub(crate) line_length: u32,
    pub(crate) mmio_start: usize,
    pub(crate) mmio_len: u32,
    pub(crate) accel: u32,
    pub(crate) capabilities: u16,
    pub(crate) reserved: [u16; 2],
}

pub(crate) fn fb_get_var(fd: BorrowedFd<'_>) -> io::Result<FbVarScreeninfo> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: FBIOGET_VSCREENINFO writes one `struct fb_var_screeninfo`,
    // which `FbVarScreeninfo` lays out field for field.
    let var = unsafe { ioctl(fd, Getter::<FBIOGET_VSCREENINFO, FbVarScreeninfo>::new())? };
    Ok(var)
}

/// Sets the variable screen information, which the kernel writes back as
/// it applied it.
pub(crate) fn fb_put_var(fd: BorrowedFd<'_>, var: &mut FbVarScreeninfo) -> io::Result<()> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: FBIOPUT_VSCREENINFO reads and writes back one
    // `struct fb_var_screeninfo`, which `FbVarScreeninfo` lays out field for
    // field, through a pointer that lives for the call.
    unsafe {
        ioctl(
            fd,
            Updater::<FBIOPUT_VSCREENINFO, FbVarScreeninfo>::new(var),
        )?;
    }
    Ok(())
}

pub(crate) fn fb_get_fix(fd: BorrowedFd<'_>) -> io::Result<FbFixScreeninfo> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: FBIOGET_FSCREENINFO writes one `struct fb_fix_screeninfo`,
    // which `FbFixScreeninfo` lays out field for field.
    let fix = unsafe { ioctl(fd, Getter::<FBIOGET_FSCREENINFO, FbFixScreeninfo>::new())? };
    Ok(fix)
}

/// Blanks the framebuffer at `level`, one of the `FB_BLANK_*` values.
pub(crate) fn fb_blank(fd: BorrowedFd<'_>, level: u32) -> io::Result<()> {
    let level = usize::try_from(level).map_err(|_| io::ErrorKind::InvalidInput)?;
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: FBIOBLANK takes its level as the integer argument itself, and
    // the kernel refuses a level out of its range.
    unsafe {
        ioctl(fd, IntegerSetter::<FBIOBLANK>::new_usize(level))?;
    }
    Ok(())
}

/// `VT_OPENQRY`, `linux/vt.h`.
const VT_OPENQRY: Opcode = 0x5600;
/// `VT_SETMODE`, `linux/vt.h`.
const VT_SETMODE: Opcode = 0x5602;
/// `VT_GETSTATE`, `linux/vt.h`.
const VT_GETSTATE: Opcode = 0x5603;
/// `VT_RELDISP`, `linux/vt.h`.
const VT_RELDISP: Opcode = 0x5605;
/// `VT_ACTIVATE`, `linux/vt.h`.
const VT_ACTIVATE: Opcode = 0x5606;
/// `KDSETMODE`, `linux/kd.h`.
const KDSETMODE: Opcode = 0x4B3A;
/// `KDGKBMODE`, `linux/kd.h`.
const KDGKBMODE: Opcode = 0x4B44;
/// `KDSKBMODE`, `linux/kd.h`.
const KDSKBMODE: Opcode = 0x4B45;

/// `VT_AUTO`, `linux/vt.h`.
pub(crate) const VT_AUTO: u8 = 0;
/// `VT_PROCESS`, `linux/vt.h`.
pub(crate) const VT_PROCESS: u8 = 1;
/// `KD_TEXT`, `linux/kd.h`.
pub(crate) const KD_TEXT: u32 = 0;
/// `KD_GRAPHICS`, `linux/kd.h`.
pub(crate) const KD_GRAPHICS: u32 = 1;
/// `K_RAW`, `linux/kd.h`.
pub(crate) const K_RAW: u32 = 0;
/// `K_XLATE`, `linux/kd.h`.
pub(crate) const K_XLATE: u32 = 1;
/// `K_MEDIUMRAW`, `linux/kd.h`.
pub(crate) const K_MEDIUMRAW: u32 = 2;
/// `K_UNICODE`, `linux/kd.h`.
pub(crate) const K_UNICODE: u32 = 3;
/// `K_OFF`, `linux/kd.h`.
pub(crate) const K_OFF: u32 = 4;

/// `struct vt_mode`, `linux/vt.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VtMode {
    pub(crate) mode: u8,
    pub(crate) waitv: u8,
    pub(crate) relsig: c_short,
    pub(crate) acqsig: c_short,
    pub(crate) frsig: c_short,
}

/// `struct vt_stat`, `linux/vt.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VtStat {
    pub(crate) v_active: u16,
    pub(crate) v_signal: u16,
    pub(crate) v_state: u16,
}

/// The number of the VT on screen.
pub(crate) fn vt_active(fd: BorrowedFd<'_>) -> io::Result<u16> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: VT_GETSTATE writes one `struct vt_stat`, which `VtStat` lays
    // out field for field.
    let stat = unsafe { ioctl(fd, Getter::<VT_GETSTATE, VtStat>::new())? };
    Ok(stat.v_active)
}

pub(crate) fn vt_set_mode(fd: BorrowedFd<'_>, mode: VtMode) -> io::Result<()> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: VT_SETMODE reads one `struct vt_mode`, which `VtMode` lays out
    // field for field, through a pointer that lives for the call.
    unsafe {
        ioctl(fd, Setter::<VT_SETMODE, VtMode>::new(mode))?;
    }
    Ok(())
}

/// `VT_RELDISP`: 0 refuses a release, 1 allows it.
pub(crate) fn vt_reldisp(fd: BorrowedFd<'_>, arg: u32) -> io::Result<()> {
    let arg = usize::try_from(arg).map_err(|_| io::ErrorKind::InvalidInput)?;
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: VT_RELDISP takes its argument as the integer itself.
    unsafe {
        ioctl(fd, IntegerSetter::<VT_RELDISP>::new_usize(arg))?;
    }
    Ok(())
}

/// Asks the kernel to switch to VT `num`.
pub(crate) fn vt_activate(fd: BorrowedFd<'_>, num: u16) -> io::Result<()> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: VT_ACTIVATE takes the VT's number as the integer argument
    // itself, and the kernel refuses one out of range.
    unsafe {
        ioctl(
            fd,
            IntegerSetter::<VT_ACTIVATE>::new_usize(usize::from(num)),
        )?;
    }
    Ok(())
}

/// The first VT no process has open.
pub(crate) fn vt_openqry(fd: BorrowedFd<'_>) -> io::Result<c_int> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: VT_OPENQRY writes one `int`.
    let num = unsafe { ioctl(fd, Getter::<VT_OPENQRY, c_int>::new())? };
    Ok(num)
}

/// `KDSETMODE`: `KD_TEXT` or `KD_GRAPHICS`.
pub(crate) fn kd_set_mode(fd: BorrowedFd<'_>, mode: u32) -> io::Result<()> {
    let mode = usize::try_from(mode).map_err(|_| io::ErrorKind::InvalidInput)?;
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: KDSETMODE takes the mode as the integer argument itself, and
    // the kernel refuses one out of range.
    unsafe {
        ioctl(fd, IntegerSetter::<KDSETMODE>::new_usize(mode))?;
    }
    Ok(())
}

/// `KDGKBMODE`: the keyboard mode, one of the `K_*` values.
pub(crate) fn kd_get_kb_mode(fd: BorrowedFd<'_>) -> io::Result<c_int> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: KDGKBMODE writes one `int`.
    let mode = unsafe { ioctl(fd, Getter::<KDGKBMODE, c_int>::new())? };
    Ok(mode)
}

/// `KDSKBMODE`: one of the `K_*` values.
pub(crate) fn kd_set_kb_mode(fd: BorrowedFd<'_>, mode: u32) -> io::Result<()> {
    let mode = usize::try_from(mode).map_err(|_| io::ErrorKind::InvalidInput)?;
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: KDSKBMODE takes the mode as the integer argument itself, and
    // the kernel refuses one out of range.
    unsafe {
        ioctl(fd, IntegerSetter::<KDSKBMODE>::new_usize(mode))?;
    }
    Ok(())
}

/// `EV_SYN`, `linux/input-event-codes.h`.
pub(crate) const EV_SYN: u16 = 0x00;
/// `EV_KEY`, `linux/input-event-codes.h`.
pub(crate) const EV_KEY: u16 = 0x01;
/// `EV_REL`, `linux/input-event-codes.h`.
pub(crate) const EV_REL: u16 = 0x02;
/// `EV_ABS`, `linux/input-event-codes.h`.
pub(crate) const EV_ABS: u16 = 0x03;
/// `EV_LED`, `linux/input-event-codes.h`.
pub(crate) const EV_LED: u16 = 0x11;
/// `KEY_MIN_INTERESTING`, `linux/input-event-codes.h`: `KEY_MUTE`.
pub(crate) const KEY_MIN_INTERESTING: u16 = 113;
/// `BTN_LEFT`, `linux/input-event-codes.h`.
pub(crate) const BTN_LEFT: u16 = 0x110;
/// `BTN_RIGHT`, `linux/input-event-codes.h`.
pub(crate) const BTN_RIGHT: u16 = 0x111;
/// `BTN_MIDDLE`, `linux/input-event-codes.h`.
pub(crate) const BTN_MIDDLE: u16 = 0x112;
/// `BTN_TOUCH`, `linux/input-event-codes.h`.
pub(crate) const BTN_TOUCH: u16 = 0x14a;
/// `BTN_TOOL_DOUBLETAP`, `linux/input-event-codes.h`.
pub(crate) const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
/// `BTN_TOOL_TRIPLETAP`, `linux/input-event-codes.h`.
pub(crate) const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
/// `REL_X`, `linux/input-event-codes.h`.
pub(crate) const REL_X: u16 = 0x00;
/// `REL_Y`, `linux/input-event-codes.h`.
pub(crate) const REL_Y: u16 = 0x01;
/// `REL_WHEEL`, `linux/input-event-codes.h`.
pub(crate) const REL_WHEEL: u16 = 0x08;
/// `ABS_X`, `linux/input-event-codes.h`.
pub(crate) const ABS_X: u16 = 0x00;
/// `ABS_Y`, `linux/input-event-codes.h`.
pub(crate) const ABS_Y: u16 = 0x01;
/// `INPUT_PROP_DIRECT`, `linux/input-event-codes.h`.
pub(crate) const INPUT_PROP_DIRECT: u16 = 0x01;
/// `LED_NUML`, `linux/input-event-codes.h`.
pub(crate) const LED_NUML: u16 = 0x00;
/// `LED_CAPSL`, `linux/input-event-codes.h`.
pub(crate) const LED_CAPSL: u16 = 0x01;
/// `LED_SCROLLL`, `linux/input-event-codes.h`.
pub(crate) const LED_SCROLLL: u16 = 0x02;
/// `EV_CNT`, `linux/input-event-codes.h`: `EV_MAX + 1`.
const EV_CNT: usize = 0x20;
/// `KEY_CNT`, `linux/input-event-codes.h`: `KEY_MAX + 1`.
const KEY_CNT: usize = 0x300;
/// `REL_CNT`, `linux/input-event-codes.h`: `REL_MAX + 1`.
const REL_CNT: usize = 0x10;
/// `ABS_CNT`, `linux/input-event-codes.h`: `ABS_MAX + 1`.
const ABS_CNT: usize = 0x40;
/// `INPUT_PROP_CNT`, `linux/input-event-codes.h`: `INPUT_PROP_MAX + 1`.
const INPUT_PROP_CNT: usize = 0x20;

/// The longs a bitmap of `bits` bits takes, as the kernel's `unsigned long`
/// arrays hold them.
const fn longs(bits: usize) -> usize {
    bits.div_ceil(usize::BITS as usize)
}

/// A device's bitmap of the codes it reports, in the kernel's layout.
pub(crate) type EvBits = [usize; longs(EV_CNT)];
pub(crate) type KeyBits = [usize; longs(KEY_CNT)];
pub(crate) type RelBits = [usize; longs(REL_CNT)];
pub(crate) type AbsBits = [usize; longs(ABS_CNT)];
pub(crate) type PropBits = [usize; longs(INPUT_PROP_CNT)];

/// Whether bit `bit` is set in a bitmap the kernel filled.
pub(crate) fn bit_is_set(bits: &[usize], bit: u16) -> bool {
    let bit = usize::from(bit);
    let (Some(word), Some(shift)) = (bit.checked_div(LONG_BITS), bit.checked_rem(LONG_BITS)) else {
        return false;
    };
    bits.get(word)
        .and_then(|w| w.checked_shr(u32::try_from(shift).ok()?))
        .is_some_and(|w| w & 1 == 1)
}

/// The bits in one of a bitmap's `unsigned long`s.
const LONG_BITS: usize = usize::BITS as usize;

/// `EVIOCGNAME(len)`: `_IOC(_IOC_READ, 'E', 0x06, len)`.
const EVIOCGNAME: Opcode = opcode::read::<[u8; 256]>(b'E', 0x06);
/// `EVIOCGPROP(len)`: `_IOC(_IOC_READ, 'E', 0x09, len)`.
const EVIOCGPROP: Opcode = opcode::read::<PropBits>(b'E', 0x09);
/// `EVIOCGBIT(0, len)`: `_IOC(_IOC_READ, 'E', 0x20 + ev, len)`.
const EVIOCGBIT_EV: Opcode = opcode::read::<EvBits>(b'E', 0x20);
/// `EVIOCGBIT(EV_KEY, len)`.
const EVIOCGBIT_KEY: Opcode = opcode::read::<KeyBits>(b'E', 0x21);
/// `EVIOCGBIT(EV_REL, len)`.
const EVIOCGBIT_REL: Opcode = opcode::read::<RelBits>(b'E', 0x22);
/// `EVIOCGBIT(EV_ABS, len)`.
const EVIOCGBIT_ABS: Opcode = opcode::read::<AbsBits>(b'E', 0x23);
/// `EVIOCGABS(ABS_X)`: `_IOR('E', 0x40 + abs, struct input_absinfo)`.
const EVIOCGABS_X: Opcode = opcode::read::<InputAbsinfo>(b'E', 0x40);
/// `EVIOCGABS(ABS_Y)`.
const EVIOCGABS_Y: Opcode = opcode::read::<InputAbsinfo>(b'E', 0x41);

/// `struct input_absinfo`, `linux/input.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InputAbsinfo {
    pub(crate) value: i32,
    pub(crate) minimum: i32,
    pub(crate) maximum: i32,
    pub(crate) fuzz: i32,
    pub(crate) flat: i32,
    pub(crate) resolution: i32,
}

/// The device's name, up to its first NUL.
pub(crate) fn evdev_name(fd: BorrowedFd<'_>) -> io::Result<String> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: EVIOCGNAME writes at most the length its number encodes, the
    // size of the array.
    let name = unsafe { ioctl(fd, Getter::<EVIOCGNAME, [u8; 256]>::new())? };
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    Ok(String::from_utf8_lossy(name.get(..end).unwrap_or_default()).into_owned())
}

/// The event types the device reports.
pub(crate) fn evdev_ev_bits(fd: BorrowedFd<'_>) -> io::Result<EvBits> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: EVIOCGBIT writes at most the length its number encodes, the
    // size of the array, as `unsigned long`s.
    let bits = unsafe { ioctl(fd, Getter::<EVIOCGBIT_EV, EvBits>::new())? };
    Ok(bits)
}

/// The keys and buttons the device reports.
pub(crate) fn evdev_key_bits(fd: BorrowedFd<'_>) -> io::Result<KeyBits> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: as in `evdev_ev_bits`.
    let bits = unsafe { ioctl(fd, Getter::<EVIOCGBIT_KEY, KeyBits>::new())? };
    Ok(bits)
}

/// The relative axes the device reports.
pub(crate) fn evdev_rel_bits(fd: BorrowedFd<'_>) -> io::Result<RelBits> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: as in `evdev_ev_bits`.
    let bits = unsafe { ioctl(fd, Getter::<EVIOCGBIT_REL, RelBits>::new())? };
    Ok(bits)
}

/// The absolute axes the device reports.
pub(crate) fn evdev_abs_bits(fd: BorrowedFd<'_>) -> io::Result<AbsBits> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: as in `evdev_ev_bits`.
    let bits = unsafe { ioctl(fd, Getter::<EVIOCGBIT_ABS, AbsBits>::new())? };
    Ok(bits)
}

/// The device's properties, `INPUT_PROP_*`.
pub(crate) fn evdev_props(fd: BorrowedFd<'_>) -> io::Result<PropBits> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: as in `evdev_ev_bits`.
    let bits = unsafe { ioctl(fd, Getter::<EVIOCGPROP, PropBits>::new())? };
    Ok(bits)
}

/// The range of the X and Y axes.
pub(crate) fn evdev_abs_xy(fd: BorrowedFd<'_>) -> io::Result<(InputAbsinfo, InputAbsinfo)> {
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: EVIOCGABS writes one `struct input_absinfo`, which
    // `InputAbsinfo` lays out field for field.
    let x = unsafe { ioctl(fd, Getter::<EVIOCGABS_X, InputAbsinfo>::new())? };
    #[expect(unsafe_code, reason = "an ioctl rustix does not wrap")]
    // SAFETY: as above.
    let y = unsafe { ioctl(fd, Getter::<EVIOCGABS_Y, InputAbsinfo>::new())? };
    Ok((x, y))
}

#[cfg(test)]
#[expect(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// The kernel's UAPI headers: `VTIUM_KERNEL_HEADERS`, or
    /// `/usr/include`, where the linux-api-headers (Arch) and
    /// linux-libc-dev (Debian) packages put them.
    fn header(name: &str) -> String {
        let root = std::env::var_os("VTIUM_KERNEL_HEADERS")
            .map_or_else(|| PathBuf::from("/usr/include"), PathBuf::from);
        let path = root.join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e}; install the kernel's UAPI headers or set VTIUM_KERNEL_HEADERS",
                path.display()
            )
        })
    }

    /// The value of `#define NAME VALUE` in `text`, decimal or hex.
    fn define(text: &str, name: &str) -> u32 {
        text.lines()
            .find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("#define") && words.next() == Some(name))
                    .then(|| words.next())
                    .flatten()
            })
            .map(|value| match value.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => value.parse(),
            })
            .unwrap_or_else(|| panic!("{name} is not defined"))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// The value of an enumerator `NAME = EXPR,` in `text`, where `EXPR`
    /// joins numbers and other enumerators with `+` or `|`.
    fn enumerator(text: &str, name: &str) -> u32 {
        let expr = text
            .lines()
            .find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some(name) && words.next() == Some("="))
                    .then(|| words.collect::<Vec<_>>().join(" "))
            })
            .unwrap_or_else(|| panic!("{name} is not an enumerator"));
        let mut value = 0u32;
        let mut op = "|";
        for word in expr.trim_end_matches(',').split_whitespace() {
            if word == "+" || word == "|" {
                op = word;
                continue;
            }
            let term = word.parse().unwrap_or_else(|_| enumerator(text, word));
            value = if op == "+" {
                value.saturating_add(term)
            } else {
                value | term
            };
        }
        value
    }

    #[test]
    fn request_numbers_match_the_headers() {
        let fb = header("linux/fb.h");
        for (name, ours) in [
            ("FBIOGET_VSCREENINFO", FBIOGET_VSCREENINFO),
            ("FBIOPUT_VSCREENINFO", FBIOPUT_VSCREENINFO),
            ("FBIOGET_FSCREENINFO", FBIOGET_FSCREENINFO),
            ("FBIOBLANK", FBIOBLANK),
            ("FB_ACTIVATE_NOW", FB_ACTIVATE_NOW),
            ("FB_ACTIVATE_FORCE", FB_ACTIVATE_FORCE),
            ("FB_VISUAL_TRUECOLOR", FB_VISUAL_TRUECOLOR),
        ] {
            assert_eq!(define(&fb, name), ours, "{name}");
        }
        // The blank levels are enumerators over the VESA levels.
        let levels = fb + &header("linux/vesa.h");
        for (name, ours) in [
            ("FB_BLANK_UNBLANK", FB_BLANK_UNBLANK),
            ("FB_BLANK_NORMAL", FB_BLANK_NORMAL),
            ("FB_BLANK_POWERDOWN", FB_BLANK_POWERDOWN),
        ] {
            assert_eq!(enumerator(&levels, name), ours, "{name}");
        }
    }

    #[test]
    fn vt_and_keyboard_numbers_match_the_headers() {
        let vt = header("linux/vt.h");
        for (name, ours) in [
            ("VT_OPENQRY", VT_OPENQRY),
            ("VT_SETMODE", VT_SETMODE),
            ("VT_GETSTATE", VT_GETSTATE),
            ("VT_RELDISP", VT_RELDISP),
            ("VT_ACTIVATE", VT_ACTIVATE),
        ] {
            assert_eq!(define(&vt, name), ours, "{name}");
        }
        for (name, ours) in [
            ("VT_AUTO", VT_AUTO.into()),
            ("VT_PROCESS", VT_PROCESS.into()),
        ] {
            assert_eq!(define(&vt, name), ours, "{name}");
        }
        let kd = header("linux/kd.h");
        for (name, ours) in [
            ("KDSETMODE", KDSETMODE),
            ("KDGKBMODE", KDGKBMODE),
            ("KDSKBMODE", KDSKBMODE),
        ] {
            assert_eq!(define(&kd, name), ours, "{name}");
        }
        for (name, ours) in [
            ("KD_TEXT", KD_TEXT),
            ("KD_GRAPHICS", KD_GRAPHICS),
            ("K_RAW", K_RAW),
            ("K_XLATE", K_XLATE),
            ("K_MEDIUMRAW", K_MEDIUMRAW),
            ("K_UNICODE", K_UNICODE),
            ("K_OFF", K_OFF),
        ] {
            assert_eq!(define(&kd, name), ours, "{name}");
        }
        assert_eq!(size_of::<VtMode>(), 8);
        assert_eq!(size_of::<VtStat>(), 6);
    }

    /// The `_IOC` read direction and size fields, as `asm-generic/ioctl.h`
    /// lays them out on the architectures vtium builds for.
    fn ioc_read(group: u8, number: u8, size: usize) -> Opcode {
        let size = Opcode::try_from(size).unwrap_or_else(|e| panic!("{e}"));
        (2 << 30) | (size << 16) | (Opcode::from(group) << 8) | Opcode::from(number)
    }

    #[test]
    fn evdev_numbers_match_the_headers() {
        let input = header("linux/input.h");
        for (macro_, body) in [
            ("EVIOCGNAME(len)", "_IOC(_IOC_READ, 'E', 0x06, len)"),
            ("EVIOCGPROP(len)", "_IOC(_IOC_READ, 'E', 0x09, len)"),
            (
                "EVIOCGBIT(ev,len)",
                "_IOC(_IOC_READ, 'E', 0x20 + (ev), len)",
            ),
            (
                "EVIOCGABS(abs)",
                "_IOR('E', 0x40 + (abs), struct input_absinfo)",
            ),
        ] {
            let line = input
                .lines()
                .find(|l| l.split_whitespace().nth(1) == Some(macro_))
                .unwrap_or_else(|| panic!("{macro_} is not defined"));
            assert!(line.contains(body), "{macro_}: {line}");
        }
        assert_eq!(EVIOCGNAME, ioc_read(b'E', 0x06, 256));
        assert_eq!(EVIOCGPROP, ioc_read(b'E', 0x09, size_of::<PropBits>()));
        assert_eq!(EVIOCGBIT_EV, ioc_read(b'E', 0x20, size_of::<EvBits>()));
        assert_eq!(EVIOCGBIT_KEY, ioc_read(b'E', 0x21, size_of::<KeyBits>()));
        assert_eq!(EVIOCGBIT_REL, ioc_read(b'E', 0x22, size_of::<RelBits>()));
        assert_eq!(EVIOCGBIT_ABS, ioc_read(b'E', 0x23, size_of::<AbsBits>()));
        assert_eq!(EVIOCGABS_X, ioc_read(b'E', 0x40, 24));
        assert_eq!(EVIOCGABS_Y, ioc_read(b'E', 0x41, 24));
        assert_eq!(size_of::<InputAbsinfo>(), 24);

        let codes = header("linux/input-event-codes.h");
        for (name, ours) in [
            ("EV_SYN", EV_SYN),
            ("EV_KEY", EV_KEY),
            ("EV_REL", EV_REL),
            ("EV_ABS", EV_ABS),
            ("EV_LED", EV_LED),
            ("KEY_MUTE", KEY_MIN_INTERESTING),
            ("BTN_LEFT", BTN_LEFT),
            ("BTN_RIGHT", BTN_RIGHT),
            ("BTN_MIDDLE", BTN_MIDDLE),
            ("BTN_TOUCH", BTN_TOUCH),
            ("BTN_TOOL_DOUBLETAP", BTN_TOOL_DOUBLETAP),
            ("BTN_TOOL_TRIPLETAP", BTN_TOOL_TRIPLETAP),
            ("REL_X", REL_X),
            ("REL_Y", REL_Y),
            ("REL_WHEEL", REL_WHEEL),
            ("ABS_X", ABS_X),
            ("ABS_Y", ABS_Y),
            ("INPUT_PROP_DIRECT", INPUT_PROP_DIRECT),
            ("LED_NUML", LED_NUML),
            ("LED_CAPSL", LED_CAPSL),
            ("LED_SCROLLL", LED_SCROLLL),
        ] {
            assert_eq!(define(&codes, name), u32::from(ours), "{name}");
        }
        assert!(codes.contains("#define KEY_MIN_INTERESTING\tKEY_MUTE"));
        for (name, count) in [
            ("EV_MAX", EV_CNT),
            ("KEY_MAX", KEY_CNT),
            ("REL_MAX", REL_CNT),
            ("ABS_MAX", ABS_CNT),
            ("INPUT_PROP_MAX", INPUT_PROP_CNT),
        ] {
            let max = usize::try_from(define(&codes, name)).unwrap_or(0);
            assert_eq!(max + 1, count, "{name}");
        }
    }

    #[test]
    fn a_bitmap_reads_bits_across_its_words() {
        let mut bits: KeyBits = [0; longs(KEY_CNT)];
        bits[0] = 1 << 1;
        bits[BTN_LEFT as usize / usize::BITS as usize] |=
            1 << (BTN_LEFT as usize % usize::BITS as usize);
        assert!(bit_is_set(&bits, 1));
        assert!(!bit_is_set(&bits, 0));
        assert!(bit_is_set(&bits, BTN_LEFT));
        assert!(!bit_is_set(&bits, BTN_RIGHT));
        assert!(!bit_is_set(&bits, u16::MAX));
    }

    /// The structures' sizes as the header lays them out: 40 32-bit words
    /// for `fb_var_screeninfo`, and `fb_fix_screeninfo` with its two
    /// `unsigned long` fields.
    #[test]
    fn structures_have_the_headers_sizes() {
        assert_eq!(size_of::<FbVarScreeninfo>(), 160);
        let fix = if size_of::<usize>() == 8 { 80 } else { 68 };
        assert_eq!(size_of::<FbFixScreeninfo>(), fix);
        let line_length = if size_of::<usize>() == 8 { 48 } else { 44 };
        assert_eq!(
            std::mem::offset_of!(FbFixScreeninfo, line_length),
            line_length
        );
    }
}
