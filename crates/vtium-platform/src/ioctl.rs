//! The ioctls rustix does not wrap: one definition each, through rustix's
//! `ioctl` module (vtium/vtium#6).
//!
//! Each request number is the kernel header's, and
//! `request_numbers_match_the_headers` reads the headers and fails when one
//! differs. The fbdev numbers predate size encoding, so nothing else would
//! catch a wrong one. DRM's ioctls come wrapped from `drm-ffi`.

use std::io;
use std::os::fd::BorrowedFd;

use rustix::ioctl::{Getter, IntegerSetter, Opcode, Updater, ioctl};

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

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
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
