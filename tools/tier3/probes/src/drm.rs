//! KMS through a card's descriptor: whether the file holds master, and a
//! drawing with two dumb buffers and page flips, with the kernel's own
//! structures (include/uapi/drm/drm.h and drm_mode.h).

use std::ffi::c_int;

use crate::sys::last_error;

const fn iowr(nr: u64, size: usize) -> libc::c_ulong {
    ((3u64 << 30) | ((size as u64) << 16) | ((b'd' as u64) << 8) | nr) as libc::c_ulong
}
const fn io(nr: u64) -> libc::c_ulong {
    (((b'd' as u64) << 8) | nr) as libc::c_ulong
}

pub const SET_MASTER: libc::c_ulong = io(0x1e);
pub const DROP_MASTER: libc::c_ulong = io(0x1f);
const GETRESOURCES: libc::c_ulong = iowr(0xA0, std::mem::size_of::<CardRes>());
const GETCRTC: libc::c_ulong = iowr(0xA1, std::mem::size_of::<Crtc>());
const SETCRTC: libc::c_ulong = iowr(0xA2, std::mem::size_of::<Crtc>());
const GETENCODER: libc::c_ulong = iowr(0xA6, std::mem::size_of::<Encoder>());
const GETCONNECTOR: libc::c_ulong = iowr(0xA7, std::mem::size_of::<Connector>());
const ADDFB: libc::c_ulong = iowr(0xAE, std::mem::size_of::<FbCmd>());
const PAGE_FLIP: libc::c_ulong = iowr(0xB0, std::mem::size_of::<PageFlip>());
const CREATE_DUMB: libc::c_ulong = iowr(0xB2, std::mem::size_of::<CreateDumb>());
const MAP_DUMB: libc::c_ulong = iowr(0xB3, std::mem::size_of::<MapDumb>());

#[repr(C)]
#[derive(Default)]
struct CardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ModeInfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    kind: u32,
    name: [u8; 32],
}

#[repr(C)]
#[derive(Default)]
struct Crtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: ModeInfo,
}

#[repr(C)]
#[derive(Default)]
struct Encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Default)]
struct Connector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    pad: u32,
}

#[repr(C)]
#[derive(Default)]
struct FbCmd {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
}

#[repr(C)]
#[derive(Default)]
struct PageFlip {
    crtc_id: u32,
    fb_id: u32,
    flags: u32,
    reserved: u32,
    user_data: u64,
}

#[repr(C)]
#[derive(Default)]
struct CreateDumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

#[repr(C)]
#[derive(Default)]
struct MapDumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

fn ioctl<T>(fd: c_int, req: libc::c_ulong, arg: &mut T) -> Result<(), String> {
    // SAFETY: each request is paired with the structure the kernel expects
    // for it.
    if unsafe { libc::ioctl(fd, req, arg as *mut T) } == 0 {
        Ok(())
    } else {
        Err(last_error())
    }
}

/// Whether the file behind `fd` holds master: SETCRTC is a call only
/// master may make, so the kernel refuses it with EACCES before it looks at
/// the arguments when the file is not master. A CRTC id that names nothing
/// makes the call fail harmlessly when it is.
pub fn is_master(fd: c_int) -> String {
    let mut c = Crtc {
        crtc_id: 0xffff_fff0,
        ..Default::default()
    };
    // SAFETY: SETCRTC with its own structure.
    let r = unsafe { libc::ioctl(fd, SETCRTC, &mut c as *mut Crtc) };
    if r == 0 {
        return "yes".into();
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EACCES) => "no (SETCRTC: EACCES)".into(),
        Some(e) => format!("yes (SETCRTC on no CRTC: {})", crate::sys::errno_text(e)),
        None => "?".into(),
    }
}

/// The first connected connector, its first mode and a CRTC for it.
struct Output {
    connector: u32,
    crtc: u32,
    mode: ModeInfo,
}

fn find_output(fd: c_int) -> Result<Output, String> {
    let mut res = CardRes::default();
    ioctl(fd, GETRESOURCES, &mut res).map_err(|e| format!("GETRESOURCES: {e}"))?;
    let mut crtcs = vec![0u32; res.count_crtcs as usize];
    let mut conns = vec![0u32; res.count_connectors as usize];
    let mut res2 = CardRes {
        crtc_id_ptr: crtcs.as_mut_ptr() as u64,
        connector_id_ptr: conns.as_mut_ptr() as u64,
        count_crtcs: res.count_crtcs,
        count_connectors: res.count_connectors,
        ..Default::default()
    };
    ioctl(fd, GETRESOURCES, &mut res2).map_err(|e| format!("GETRESOURCES: {e}"))?;
    for &id in &conns {
        let mut c = Connector {
            connector_id: id,
            ..Default::default()
        };
        ioctl(fd, GETCONNECTOR, &mut c).map_err(|e| format!("GETCONNECTOR: {e}"))?;
        if c.connection != 1 || c.count_modes == 0 {
            continue;
        }
        let mut modes = vec![ModeInfo::default(); c.count_modes as usize];
        let mut encs = vec![0u32; c.count_encoders as usize];
        let mut c2 = Connector {
            connector_id: id,
            modes_ptr: modes.as_mut_ptr() as u64,
            count_modes: c.count_modes,
            encoders_ptr: encs.as_mut_ptr() as u64,
            count_encoders: c.count_encoders,
            ..Default::default()
        };
        ioctl(fd, GETCONNECTOR, &mut c2).map_err(|e| format!("GETCONNECTOR: {e}"))?;
        for &e in &encs {
            let mut enc = Encoder {
                encoder_id: e,
                ..Default::default()
            };
            if ioctl(fd, GETENCODER, &mut enc).is_err() {
                continue;
            }
            if let Some((i, _)) = crtcs
                .iter()
                .enumerate()
                .find(|(i, _)| enc.possible_crtcs & (1 << i) != 0)
            {
                return Ok(Output {
                    connector: id,
                    crtc: crtcs[i],
                    mode: modes[0],
                });
            }
        }
    }
    Err("no connected connector".into())
}

/// A dumb buffer with a framebuffer, mapped and filled with one color.
struct Buffer {
    fb: u32,
}

fn buffer(fd: c_int, w: u32, h: u32, color: u32) -> Result<Buffer, String> {
    let mut cd = CreateDumb {
        width: w,
        height: h,
        bpp: 32,
        ..Default::default()
    };
    ioctl(fd, CREATE_DUMB, &mut cd).map_err(|e| format!("CREATE_DUMB: {e}"))?;
    let mut fb = FbCmd {
        width: w,
        height: h,
        pitch: cd.pitch,
        bpp: 32,
        depth: 24,
        handle: cd.handle,
        ..Default::default()
    };
    ioctl(fd, ADDFB, &mut fb).map_err(|e| format!("ADDFB: {e}"))?;
    let mut md = MapDumb {
        handle: cd.handle,
        ..Default::default()
    };
    ioctl(fd, MAP_DUMB, &mut md).map_err(|e| format!("MAP_DUMB: {e}"))?;
    // SAFETY: maps the dumb buffer at the offset the kernel gave, writes
    // within its size, and unmaps it.
    unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            cd.size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            md.offset as libc::off_t,
        );
        if p == libc::MAP_FAILED {
            return Err(format!("mmap: {}", last_error()));
        }
        let px = std::slice::from_raw_parts_mut(p.cast::<u32>(), cd.size as usize / 4);
        px.fill(color);
        libc::munmap(p, cd.size as usize);
    }
    Ok(Buffer { fb: fb.fb_id })
}

/// The drawing: SETCRTC with one buffer, then `flips` page flips between
/// two, each waited for through its event on `fd`.
pub struct Drawing {
    out: Output,
    bufs: [Buffer; 2],
    front: usize,
}

impl Drawing {
    pub fn new(fd: c_int) -> Result<(Drawing, String), String> {
        let out = find_output(fd)?;
        let (w, h) = (u32::from(out.mode.hdisplay), u32::from(out.mode.vdisplay));
        let bufs = [
            buffer(fd, w, h, 0x00_20_60_c0)?,
            buffer(fd, w, h, 0x00_c0_60_20)?,
        ];
        let mut conn = out.connector;
        let mut c = Crtc {
            set_connectors_ptr: (&raw mut conn) as u64,
            count_connectors: 1,
            crtc_id: out.crtc,
            fb_id: bufs[0].fb,
            mode_valid: 1,
            mode: out.mode,
            ..Default::default()
        };
        ioctl(fd, SETCRTC, &mut c).map_err(|e| format!("SETCRTC: {e}"))?;
        let what = format!(
            "connector {} crtc {} mode {}x{}",
            out.connector, out.crtc, w, h
        );
        Ok((
            Drawing {
                out,
                bufs,
                front: 0,
            },
            what,
        ))
    }

    /// One page flip to the other buffer, waited for through its event.
    pub fn flip(&mut self, fd: c_int) -> Result<(), String> {
        let next = 1 - self.front;
        let mut pf = PageFlip {
            crtc_id: self.out.crtc,
            fb_id: self.bufs[next].fb,
            flags: 1,
            user_data: 7,
            ..Default::default()
        };
        ioctl(fd, PAGE_FLIP, &mut pf).map_err(|e| format!("PAGE_FLIP: {e}"))?;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: polls one descriptor, bounded so a lost event fails the
        // probe instead of hanging it.
        if unsafe { libc::poll(&mut pfd, 1, 5000) } != 1 {
            return Err("no flip event within 5 s".into());
        }
        let mut ev = [0u8; 64];
        // SAFETY: reads into a valid buffer.
        let n = unsafe { libc::read(fd, ev.as_mut_ptr().cast(), ev.len()) };
        if n < 8 || u32::from_ne_bytes([ev[0], ev[1], ev[2], ev[3]]) != 2 {
            return Err(format!("no DRM_EVENT_FLIP_COMPLETE (read {n})"));
        }
        self.front = next;
        Ok(())
    }

    /// Whether the CRTC scans out the buffer the last flip showed.
    pub fn shows_front(&self, fd: c_int) -> String {
        let mut c = Crtc {
            crtc_id: self.out.crtc,
            ..Default::default()
        };
        match ioctl(fd, GETCRTC, &mut c) {
            Ok(()) => format!(
                "{} (crtc fb {}, front fb {})",
                c.fb_id == self.bufs[self.front].fb,
                c.fb_id,
                self.bufs[self.front].fb
            ),
            Err(e) => format!("GETCRTC: {e}"),
        }
    }
}

/// SET_MASTER or DROP_MASTER on `fd`.
pub fn master_call(fd: c_int, req: libc::c_ulong) -> String {
    // SAFETY: these requests take no argument.
    crate::sys::result(unsafe { libc::ioctl(fd, req, 0) } as i64)
}
