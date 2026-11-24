//! The messages between the broker and the drawer, and the socket that
//! carries them.
//!
//! The broker runs as root and the drawer runs as the account `vtium`, so
//! every message the drawer sends is untrusted input to a root process.
//! Each message is exactly [`SIZE`] bytes, and the reader checks every
//! field before it hands the message on: a kind it does not know, a field
//! outside its range, a byte set past the fields, a message of another
//! size, or a descriptor where none belongs refuses the whole message.
//! The encoding is canonical, so a message the reader accepts encodes back
//! to the same bytes, which the fuzz target `protocol` checks.
//!
//! The two ends share one `SOCK_SEQPACKET` socket, which keeps each
//! message whole. A descriptor travels with the one message it belongs to,
//! as `SCM_RIGHTS`, and only the broker sends descriptors.
//!
//! All numbers are little-endian. Bytes 0 and 1 hold the kind, bytes 2
//! and 3 are zero, and the body follows from byte 4. Each kind's body is
//! laid out in the order of its fields below, and the rest of the message
//! is zero.
//!
//! This file depends on std and rustix only, so the fuzz target includes
//! it as it is.
//!
//! References:
//! - ADR-1400 D4, "The device protocol": the requests and orders, the
//!   fixed size, the checks before the broker acts, and `SCM_RIGHTS`.

// The drawer's side of the orders and statuses is wired by the split's
// later cards; until then only the tests and the fuzz target read them.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the drawer reads the orders once the split's later cards land"
    )
)]

use std::fmt;
use std::io::{self, IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::net::{
    AddressFamily, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags,
    SendAncillaryBuffer, SendAncillaryMessage, SendFlags, SocketFlags, SocketType,
};

/// The size of every message, in bytes.
pub const SIZE: usize = 128;

/// The longest device path, in bytes. A device node's path under `/dev/`
/// is short, such as `/dev/dri/card0` or `/dev/input/event12`.
pub const PATH_MAX: usize = 63;

/// The longest head name, in characters, the same bound vtium.conf puts
/// on a `[head NAME]` section.
pub const NAME_MAX: usize = 64;

/// The highest VT number. The kernel has 63 VTs, `MAX_NR_CONSOLES` in
/// `include/uapi/linux/vt.h`, and the lowest is 1.
pub const VT_MAX: u8 = 63;

/// The widest or tallest character cell, in pixels, that a session's
/// request may name. vtium's largest font at its largest zoom stays far
/// below it.
pub const CELL_MAX: u32 = 1024;

/// The highest signal number. Linux numbers its signals from 1 to 64.
pub const SIGNAL_MAX: u8 = 64;

/// The highest error number. Linux's errors run from 1 to 4095.
pub const ERRNO_MAX: u16 = 4095;

/// `EINVAL`, the error in the answer to a refused request.
const EINVAL: u16 = 22;

/// Why the reader refused a message. The broker acts on no part of a
/// refused message, apart from answering a request that waits for an
/// answer with an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The message is not [`SIZE`] bytes long.
    Size(usize),
    /// The socket cut the message or its descriptors short.
    Truncated,
    /// The message's kind is not one this direction carries.
    Kind(u16),
    /// The named field is outside the values it allows.
    Field(&'static str),
    /// A byte past the message's fields is not zero.
    Padding,
    /// The message came with this many descriptors, where its kind
    /// carries a different number.
    Descriptors(usize),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Size(n) => write!(f, "the message has {n} bytes, not {SIZE}"),
            Refusal::Truncated => write!(f, "the socket cut the message short"),
            Refusal::Kind(k) => write!(f, "the message's kind {k} is not one vtium knows"),
            Refusal::Field(name) => write!(f, "the message's {name} is out of range"),
            Refusal::Padding => write!(f, "the message has bytes set past its fields"),
            Refusal::Descriptors(n) => {
                write!(f, "the message came with {n} descriptors, the wrong number")
            }
        }
    }
}

/// A head's name: 1 to [`NAME_MAX`] characters, each one of
/// `A-Z a-z 0-9 . _ + -`, as vtium.conf allows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Head(String);

impl Head {
    /// The name, or `None` when vtium.conf would not allow it.
    pub fn new(name: &str) -> Option<Head> {
        let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-');
        (!name.is_empty() && name.len() <= NAME_MAX && name.bytes().all(allowed))
            .then(|| Head(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A device node's path: under `/dev/`, at most [`PATH_MAX`] bytes, each
/// one of `A-Z a-z 0-9 / _ - . :`, with no empty component, and no
/// component that starts with a dot, so `.` and `..` never appear. The
/// broker checks the node itself before it opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePath(String);

impl DevicePath {
    /// The path, or `None` when it is not a plain path under `/dev/`.
    pub fn new(path: &str) -> Option<DevicePath> {
        let rest = path.strip_prefix("/dev/")?;
        let allowed =
            |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.' | b':');
        let plain = path.len() <= PATH_MAX
            && path.bytes().all(allowed)
            && rest
                .split('/')
                .all(|part| !part.is_empty() && !part.starts_with('.'));
        plain.then(|| DevicePath(path.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One console of one head: the head's name and the console's id, which
/// vtium never gives another console of that head. Any id is allowed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConsoleRef {
    pub head: Head,
    pub console: u64,
}

/// A session's grid and character cell. The broker sets them on the pty
/// before the session starts, so the session's first program sees its
/// size at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    /// 1 to 65535 columns.
    pub cols: u16,
    /// 1 to 65535 rows.
    pub rows: u16,
    /// 1 to [`CELL_MAX`] pixels.
    pub cell_width: u32,
    /// 1 to [`CELL_MAX`] pixels.
    pub cell_height: u32,
}

/// The kind of display a session's head draws on, which the session's
/// `TERM_SESSION_TYPE` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Kms,
    Fbdev,
}

/// The helper the drawer asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Helper {
    /// The image helper of one console.
    Image(ConsoleRef),
    /// A font helper.
    Font,
}

/// A power key that vtium.conf binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerKey {
    Reboot,
    PowerOff,
    Suspend,
}

/// How a process the broker started ended: its exit code, or the signal
/// that killed it, 1 to [`SIGNAL_MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Code(u8),
    Signal(u8),
}

/// An error number from the broker, 1 to [`ERRNO_MAX`], in a reply that
/// would otherwise carry a descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errno(pub u16);

/// What the drawer asks of the broker. The drawer names each device and
/// each helper by an id it chooses, never 0, and the broker's answer
/// carries the same id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Open the device at `path`; the broker answers [`Order::Device`].
    OpenDevice { device: u32, path: DevicePath },
    /// Close the device.
    CloseDevice { device: u32 },
    /// Set DRM master on the broker's copy of a card; the broker answers
    /// [`Order::Master`].
    TakeMaster { device: u32 },
    /// Drop DRM master on the broker's copy of a card.
    DropMaster { device: u32 },
    /// The drawer has let its devices go after [`Order::Release`].
    Released,
    /// Switch to VT `vt`, 1 to [`VT_MAX`].
    SwitchVt { vt: u8 },
    /// Put the VT's modes back at the end of a display hand-over.
    RestoreVt,
    /// Open a console's session; the broker answers [`Order::Pty`].
    OpenSession {
        console: ConsoleRef,
        grid: Grid,
        display: Display,
        truecolor: bool,
    },
    /// Hang a console's session up.
    CloseSession { console: ConsoleRef },
    /// Send `SIGWINCH` to the foreground process group of a console's
    /// session.
    Winch { console: ConsoleRef },
    /// Start a helper; the broker answers [`Order::Helper`].
    StartHelper { helper: u32, kind: Helper },
    /// Stop a helper.
    StopHelper { helper: u32 },
    /// Run a power key.
    Power { key: PowerKey },
    /// The drawer runs.
    Ready,
}

/// What the broker tells the drawer. The four kinds that answer with a
/// descriptor carry one exactly when they succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    /// A device's descriptor, or why the broker did not open it. After
    /// [`Order::Release`], the broker sends one for each input device it
    /// opened anew, and then [`Order::Back`].
    Device {
        device: u32,
        opened: Result<(), Errno>,
    },
    /// Whether master on a card was taken.
    Master { device: u32, taken: bool },
    /// Let the devices go: the VT is switching away.
    Release,
    /// The devices are back, and the new input descriptors have come.
    Back,
    /// A console's pty master, or why the broker did not make it.
    Pty {
        console: ConsoleRef,
        opened: Result<(), Errno>,
    },
    /// A helper's socket, or why the broker did not start it.
    Helper {
        helper: u32,
        started: Result<(), Errno>,
    },
    /// A console's session ended.
    SessionExit { console: ConsoleRef, status: Status },
    /// A helper ended.
    HelperExit { helper: u32, status: Status },
    /// Blank the displays, for `vtium blank`.
    Blank,
    /// Wake the displays, for `vtium wake`.
    Wake,
    /// Stop: the unit is stopping.
    Stop,
}

impl Order {
    /// Whether the order carries a descriptor.
    pub fn carries_descriptor(&self) -> bool {
        match self {
            Order::Device { opened, .. } | Order::Pty { opened, .. } => opened.is_ok(),
            Order::Helper { started, .. } => started.is_ok(),
            _ => false,
        }
    }
}

// The kinds. The broker's orders start at 0x100, so a request read as an
// order, or an order as a request, is refused as an unknown kind.
const OPEN_DEVICE: u16 = 1;
const CLOSE_DEVICE: u16 = 2;
const TAKE_MASTER: u16 = 3;
const DROP_MASTER: u16 = 4;
const RELEASED: u16 = 5;
const SWITCH_VT: u16 = 6;
const RESTORE_VT: u16 = 7;
const OPEN_SESSION: u16 = 8;
const CLOSE_SESSION: u16 = 9;
const WINCH: u16 = 10;
const START_HELPER: u16 = 11;
const STOP_HELPER: u16 = 12;
const POWER: u16 = 13;
const READY: u16 = 14;

const DEVICE: u16 = 0x101;
const MASTER: u16 = 0x102;
const RELEASE: u16 = 0x103;
const BACK: u16 = 0x104;
const PTY: u16 = 0x105;
const HELPER: u16 = 0x106;
const SESSION_EXIT: u16 = 0x107;
const HELPER_EXIT: u16 = 0x108;
const BLANK: u16 = 0x109;
const WAKE: u16 = 0x10a;
const STOP: u16 = 0x10b;

/// A message being written. Every kind's fields fit in [`SIZE`] bytes,
/// which a test checks for the largest of each, so no write runs past the
/// end.
struct Out {
    bytes: [u8; SIZE],
    at: usize,
}

impl Out {
    fn new(kind: u16) -> Out {
        let mut out = Out {
            bytes: [0; SIZE],
            at: 0,
        };
        out.put(&kind.to_le_bytes());
        out.put(&[0, 0]);
        out
    }

    fn put(&mut self, bytes: &[u8]) {
        let end = self.at.saturating_add(bytes.len());
        if let Some(slot) = self.bytes.get_mut(self.at..end) {
            slot.copy_from_slice(bytes);
        }
        self.at = end;
    }

    fn u8(&mut self, v: u8) {
        self.put(&[v]);
    }

    fn u16(&mut self, v: u16) {
        self.put(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.put(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.put(&v.to_le_bytes());
    }

    /// A string in a field of `room` bytes: its length in one byte, then
    /// its bytes, then zeros.
    fn text(&mut self, text: &str, room: usize) {
        let len = text.len().min(room);
        self.u8(u8::try_from(len).unwrap_or(u8::MAX));
        self.put(text.as_bytes().get(..len).unwrap_or_default());
        self.at = self.at.saturating_add(room.saturating_sub(len));
    }

    fn console(&mut self, c: &ConsoleRef) {
        self.text(c.head.as_str(), NAME_MAX);
        self.u64(c.console);
    }

    fn outcome(&mut self, r: Result<(), Errno>) {
        self.u16(match r {
            Ok(()) => 0,
            Err(Errno(e)) => e,
        });
    }

    fn status(&mut self, s: Status) {
        let (tag, value) = match s {
            Status::Code(c) => (1, c),
            Status::Signal(n) => (2, n),
        };
        self.u8(tag);
        self.u8(value);
    }
}

/// A message being read, checked field by field.
struct In<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    /// The message's kind, once its size and reserved bytes are checked.
    fn new(bytes: &'a [u8]) -> Result<(In<'a>, u16), Refusal> {
        if bytes.len() != SIZE {
            return Err(Refusal::Size(bytes.len()));
        }
        let mut r = In { bytes, at: 0 };
        let kind = r.u16("kind")?;
        if r.u16("kind")? != 0 {
            return Err(Refusal::Padding);
        }
        Ok((r, kind))
    }

    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], Refusal> {
        let end = self.at.saturating_add(n);
        let got = self.bytes.get(self.at..end).ok_or(Refusal::Field(field))?;
        self.at = end;
        Ok(got)
    }

    fn array<const N: usize>(&mut self, field: &'static str) -> Result<[u8; N], Refusal> {
        self.take(N, field)?
            .try_into()
            .map_err(|_| Refusal::Field(field))
    }

    fn u8(&mut self, field: &'static str) -> Result<u8, Refusal> {
        Ok(u8::from_le_bytes(self.array(field)?))
    }

    fn u16(&mut self, field: &'static str) -> Result<u16, Refusal> {
        Ok(u16::from_le_bytes(self.array(field)?))
    }

    fn u32(&mut self, field: &'static str) -> Result<u32, Refusal> {
        Ok(u32::from_le_bytes(self.array(field)?))
    }

    fn u64(&mut self, field: &'static str) -> Result<u64, Refusal> {
        Ok(u64::from_le_bytes(self.array(field)?))
    }

    fn bool(&mut self, field: &'static str) -> Result<bool, Refusal> {
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Refusal::Field(field)),
        }
    }

    /// A string in a field of `room` bytes, as [`Out::text`] writes it,
    /// with the bytes past its length zero.
    fn text(&mut self, room: usize, field: &'static str) -> Result<&'a str, Refusal> {
        let len = usize::from(self.u8(field)?);
        let all = self.take(room, field)?;
        let (text, rest) = all.split_at_checked(len).ok_or(Refusal::Field(field))?;
        if rest.iter().any(|&b| b != 0) {
            return Err(Refusal::Field(field));
        }
        std::str::from_utf8(text).map_err(|_| Refusal::Field(field))
    }

    /// An id the drawer chose: never 0.
    fn id(&mut self, field: &'static str) -> Result<u32, Refusal> {
        match self.u32(field)? {
            0 => Err(Refusal::Field(field)),
            id => Ok(id),
        }
    }

    fn console(&mut self) -> Result<ConsoleRef, Refusal> {
        let head = Head::new(self.text(NAME_MAX, "head")?).ok_or(Refusal::Field("head"))?;
        let console = self.u64("console")?;
        Ok(ConsoleRef { head, console })
    }

    fn outcome(&mut self) -> Result<Result<(), Errno>, Refusal> {
        match self.u16("errno")? {
            0 => Ok(Ok(())),
            e if e <= ERRNO_MAX => Ok(Err(Errno(e))),
            _ => Err(Refusal::Field("errno")),
        }
    }

    fn status(&mut self) -> Result<Status, Refusal> {
        let tag = self.u8("status")?;
        let value = self.u8("status")?;
        match tag {
            1 => Ok(Status::Code(value)),
            2 if (1..=SIGNAL_MAX).contains(&value) => Ok(Status::Signal(value)),
            2 => Err(Refusal::Field("signal")),
            _ => Err(Refusal::Field("status")),
        }
    }

    fn cell(&mut self, field: &'static str) -> Result<u32, Refusal> {
        match self.u32(field)? {
            c @ 1..=CELL_MAX => Ok(c),
            _ => Err(Refusal::Field(field)),
        }
    }

    fn cells(&mut self, field: &'static str) -> Result<u16, Refusal> {
        match self.u16(field)? {
            0 => Err(Refusal::Field(field)),
            n => Ok(n),
        }
    }

    /// The message once every byte past its fields is checked to be zero.
    fn end<T>(self, message: T) -> Result<T, Refusal> {
        let rest = self.bytes.get(self.at..).unwrap_or_default();
        if rest.iter().any(|&b| b != 0) {
            return Err(Refusal::Padding);
        }
        Ok(message)
    }
}

impl Request {
    /// The request's bytes.
    pub fn encode(&self) -> [u8; SIZE] {
        let out = match self {
            Request::OpenDevice { device, path } => {
                let mut o = Out::new(OPEN_DEVICE);
                o.u32(*device);
                o.text(path.as_str(), PATH_MAX);
                o
            }
            Request::CloseDevice { device } => id_only(CLOSE_DEVICE, *device),
            Request::TakeMaster { device } => id_only(TAKE_MASTER, *device),
            Request::DropMaster { device } => id_only(DROP_MASTER, *device),
            Request::Released => Out::new(RELEASED),
            Request::SwitchVt { vt } => {
                let mut o = Out::new(SWITCH_VT);
                o.u8(*vt);
                o
            }
            Request::RestoreVt => Out::new(RESTORE_VT),
            Request::OpenSession {
                console,
                grid,
                display,
                truecolor,
            } => {
                let mut o = Out::new(OPEN_SESSION);
                o.console(console);
                o.u16(grid.cols);
                o.u16(grid.rows);
                o.u32(grid.cell_width);
                o.u32(grid.cell_height);
                o.u8(match display {
                    Display::Kms => 1,
                    Display::Fbdev => 2,
                });
                o.u8(u8::from(*truecolor));
                o
            }
            Request::CloseSession { console } => console_only(CLOSE_SESSION, console),
            Request::Winch { console } => console_only(WINCH, console),
            Request::StartHelper { helper, kind } => {
                let mut o = Out::new(START_HELPER);
                o.u32(*helper);
                match kind {
                    Helper::Image(console) => {
                        o.u8(1);
                        o.console(console);
                    }
                    Helper::Font => o.u8(2),
                }
                o
            }
            Request::StopHelper { helper } => id_only(STOP_HELPER, *helper),
            Request::Power { key } => {
                let mut o = Out::new(POWER);
                o.u8(match key {
                    PowerKey::Reboot => 1,
                    PowerKey::PowerOff => 2,
                    PowerKey::Suspend => 3,
                });
                o
            }
            Request::Ready => Out::new(READY),
        };
        out.bytes
    }

    /// The request in `bytes`, once every field is checked.
    pub fn decode(bytes: &[u8]) -> Result<Request, Refusal> {
        let (mut r, kind) = In::new(bytes)?;
        let request = match kind {
            OPEN_DEVICE => {
                let device = r.id("device")?;
                let path =
                    DevicePath::new(r.text(PATH_MAX, "path")?).ok_or(Refusal::Field("path"))?;
                Request::OpenDevice { device, path }
            }
            CLOSE_DEVICE => Request::CloseDevice {
                device: r.id("device")?,
            },
            TAKE_MASTER => Request::TakeMaster {
                device: r.id("device")?,
            },
            DROP_MASTER => Request::DropMaster {
                device: r.id("device")?,
            },
            RELEASED => Request::Released,
            SWITCH_VT => match r.u8("vt")? {
                vt @ 1..=VT_MAX => Request::SwitchVt { vt },
                _ => return Err(Refusal::Field("vt")),
            },
            RESTORE_VT => Request::RestoreVt,
            OPEN_SESSION => {
                let console = r.console()?;
                let grid = Grid {
                    cols: r.cells("columns")?,
                    rows: r.cells("rows")?,
                    cell_width: r.cell("cell width")?,
                    cell_height: r.cell("cell height")?,
                };
                let display = match r.u8("display")? {
                    1 => Display::Kms,
                    2 => Display::Fbdev,
                    _ => return Err(Refusal::Field("display")),
                };
                let truecolor = r.bool("truecolor")?;
                Request::OpenSession {
                    console,
                    grid,
                    display,
                    truecolor,
                }
            }
            CLOSE_SESSION => Request::CloseSession {
                console: r.console()?,
            },
            WINCH => Request::Winch {
                console: r.console()?,
            },
            START_HELPER => {
                let helper = r.id("helper")?;
                let kind = match r.u8("helper kind")? {
                    1 => Helper::Image(r.console()?),
                    2 => Helper::Font,
                    _ => return Err(Refusal::Field("helper kind")),
                };
                Request::StartHelper { helper, kind }
            }
            STOP_HELPER => Request::StopHelper {
                helper: r.id("helper")?,
            },
            POWER => Request::Power {
                key: match r.u8("power key")? {
                    1 => PowerKey::Reboot,
                    2 => PowerKey::PowerOff,
                    3 => PowerKey::Suspend,
                    _ => return Err(Refusal::Field("power key")),
                },
            },
            READY => Request::Ready,
            other => return Err(Refusal::Kind(other)),
        };
        r.end(request)
    }

    /// The answer to a refused request whose kind waits for one: the
    /// request's own id with the error `EINVAL`, so the drawer stops
    /// waiting for the answer. `bytes` are what the drawer sent, and only
    /// the kind and the id are read from them, each checked as for an
    /// accepted request. A request whose id is itself out of range, or
    /// whose kind waits for no answer, gets none.
    pub fn refused_answer(bytes: &[u8]) -> Option<Order> {
        let mut r = In { bytes, at: 0 };
        let kind = r.u16("kind").ok()?;
        r.u16("kind").ok()?;
        let refused = Err(Errno(EINVAL));
        Some(match kind {
            OPEN_DEVICE => Order::Device {
                device: r.id("device").ok()?,
                opened: refused,
            },
            TAKE_MASTER => Order::Master {
                device: r.id("device").ok()?,
                taken: false,
            },
            OPEN_SESSION => Order::Pty {
                console: r.console().ok()?,
                opened: refused,
            },
            START_HELPER => Order::Helper {
                helper: r.id("helper").ok()?,
                started: refused,
            },
            _ => return None,
        })
    }
}

impl Order {
    /// The order's bytes.
    pub fn encode(&self) -> [u8; SIZE] {
        let out = match self {
            Order::Device { device, opened } => {
                let mut o = Out::new(DEVICE);
                o.u32(*device);
                o.outcome(*opened);
                o
            }
            Order::Master { device, taken } => {
                let mut o = Out::new(MASTER);
                o.u32(*device);
                o.u8(u8::from(*taken));
                o
            }
            Order::Release => Out::new(RELEASE),
            Order::Back => Out::new(BACK),
            Order::Pty { console, opened } => {
                let mut o = Out::new(PTY);
                o.console(console);
                o.outcome(*opened);
                o
            }
            Order::Helper { helper, started } => {
                let mut o = Out::new(HELPER);
                o.u32(*helper);
                o.outcome(*started);
                o
            }
            Order::SessionExit { console, status } => {
                let mut o = Out::new(SESSION_EXIT);
                o.console(console);
                o.status(*status);
                o
            }
            Order::HelperExit { helper, status } => {
                let mut o = Out::new(HELPER_EXIT);
                o.u32(*helper);
                o.status(*status);
                o
            }
            Order::Blank => Out::new(BLANK),
            Order::Wake => Out::new(WAKE),
            Order::Stop => Out::new(STOP),
        };
        out.bytes
    }

    /// The order in `bytes`, once every field is checked. The caller
    /// checks the descriptors against [`Order::carries_descriptor`].
    pub fn decode(bytes: &[u8]) -> Result<Order, Refusal> {
        let (mut r, kind) = In::new(bytes)?;
        let order = match kind {
            DEVICE => Order::Device {
                device: r.id("device")?,
                opened: r.outcome()?,
            },
            MASTER => Order::Master {
                device: r.id("device")?,
                taken: r.bool("taken")?,
            },
            RELEASE => Order::Release,
            BACK => Order::Back,
            PTY => Order::Pty {
                console: r.console()?,
                opened: r.outcome()?,
            },
            HELPER => Order::Helper {
                helper: r.id("helper")?,
                started: r.outcome()?,
            },
            SESSION_EXIT => Order::SessionExit {
                console: r.console()?,
                status: r.status()?,
            },
            HELPER_EXIT => Order::HelperExit {
                helper: r.id("helper")?,
                status: r.status()?,
            },
            BLANK => Order::Blank,
            WAKE => Order::Wake,
            STOP => Order::Stop,
            other => return Err(Refusal::Kind(other)),
        };
        r.end(order)
    }
}

fn id_only(kind: u16, id: u32) -> Out {
    let mut o = Out::new(kind);
    o.u32(id);
    o
}

fn console_only(kind: u16, console: &ConsoleRef) -> Out {
    let mut o = Out::new(kind);
    o.console(console);
    o
}

/// What one read of the socket found.
#[derive(Debug)]
pub enum Got<T, E = Refusal> {
    /// A message, or why the reader refused it.
    Message(Result<T, E>),
    /// Nothing waits on the socket.
    Nothing,
    /// The other end closed the socket.
    Closed,
}

/// A request the reader refused: why, and the answer that tells the
/// drawer the request failed, when the request's kind waits for one.
#[derive(Debug)]
pub struct Refused {
    pub why: Refusal,
    pub answer: Option<Order>,
}

/// What one read of the socket found, before the reader checks it.
enum Received {
    Message(Raw),
    Nothing,
    Closed,
}

/// A message's bytes and the descriptors that came with it, before the
/// reader checks them.
struct Raw {
    bytes: [u8; SIZE],
    /// How many of `bytes` the message filled.
    len: usize,
    /// Whether the socket cut the message or its descriptors short.
    truncated: bool,
    fds: Vec<OwnedFd>,
}

impl Raw {
    /// The message's bytes, once the socket is known to have carried
    /// exactly [`SIZE`] of them and every descriptor.
    fn whole(&self) -> Result<&[u8; SIZE], Refusal> {
        if self.truncated {
            return Err(Refusal::Truncated);
        }
        if self.len != SIZE {
            return Err(Refusal::Size(self.len));
        }
        Ok(&self.bytes)
    }

    /// The request, checked. The descriptors of a refused request are
    /// closed as this returns.
    fn request(self) -> Result<Request, Refused> {
        let checked = self.whole().and_then(|bytes| match self.fds.len() {
            0 => Request::decode(bytes),
            n => Err(Refusal::Descriptors(n)),
        });
        checked.map_err(|why| Refused {
            why,
            answer: Request::refused_answer(self.bytes.get(..self.len).unwrap_or_default()),
        })
    }

    /// The order and its descriptor, checked.
    fn order(mut self) -> Result<(Order, Option<OwnedFd>), Refusal> {
        let order = Order::decode(self.whole()?)?;
        let wanted = usize::from(order.carries_descriptor());
        if self.fds.len() != wanted {
            return Err(Refusal::Descriptors(self.fds.len()));
        }
        Ok((order, self.fds.pop()))
    }
}

/// One end of the socket between the broker and the drawer.
#[derive(Debug)]
pub struct Link(OwnedFd);

impl Link {
    /// A new socket pair, both ends close-on-exec: the broker's link and
    /// the drawer's end, which the drawer's start puts on descriptor 3.
    ///
    /// The broker's end never blocks: a send to a full socket fails at
    /// once with [`io::ErrorKind::WouldBlock`], and the broker's outbox
    /// keeps the order until the socket has room. The drawer's end blocks
    /// as usual.
    pub fn pair() -> io::Result<(Link, OwnedFd)> {
        let (ours, theirs) = rustix::net::socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            None,
        )?;
        rustix::io::ioctl_fionbio(&theirs, false)?;
        Ok((Link(ours), theirs))
    }

    /// The drawer's link over `fd`, once `fd` is checked to be a Unix
    /// `SOCK_SEQPACKET` socket.
    pub fn passed(fd: OwnedFd) -> io::Result<Link> {
        let unix = rustix::net::sockopt::socket_domain(&fd)? == AddressFamily::UNIX;
        let seqpacket = rustix::net::sockopt::socket_type(&fd)? == SocketType::SEQPACKET;
        if !(unix && seqpacket) {
            return Err(io::Error::other(
                "descriptor 3 is not the socket from vtium's root process",
            ));
        }
        Ok(Link(fd))
    }

    /// Sends a request. The drawer never sends a descriptor.
    pub fn request(&self, request: &Request) -> io::Result<()> {
        self.send(&request.encode(), None)
    }

    /// Sends an order, with `fd` exactly when the order carries one. On
    /// the broker's end, a full socket fails the send with
    /// [`io::ErrorKind::WouldBlock`].
    pub fn order(&self, order: &Order, fd: Option<BorrowedFd<'_>>) -> io::Result<()> {
        if order.carries_descriptor() != fd.is_some() {
            return Err(io::Error::other(
                "an order goes with a descriptor exactly when it carries one",
            ));
        }
        self.send(&order.encode(), fd)
    }

    fn send(&self, bytes: &[u8; SIZE], fd: Option<BorrowedFd<'_>>) -> io::Result<()> {
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        let fds;
        if let Some(fd) = fd {
            fds = [fd];
            if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
                return Err(io::Error::other("no room for the descriptor"));
            }
        }
        let sent = rustix::net::sendmsg(
            &self.0,
            &[IoSlice::new(bytes)],
            &mut control,
            SendFlags::NOSIGNAL,
        )?;
        if sent != SIZE {
            return Err(io::Error::other("the socket took part of a message"));
        }
        Ok(())
    }

    /// Reads the next request without waiting. A request that came with
    /// any descriptor is refused, and the descriptors are closed at once.
    pub fn next_request(&self) -> io::Result<Got<Request, Refused>> {
        Ok(match self.receive()? {
            Received::Message(raw) => Got::Message(raw.request()),
            Received::Nothing => Got::Nothing,
            Received::Closed => Got::Closed,
        })
    }

    /// Reads the next order without waiting, with its descriptor.
    pub fn next_order(&self) -> io::Result<Got<(Order, Option<OwnedFd>)>> {
        Ok(match self.receive()? {
            Received::Message(raw) => Got::Message(raw.order()),
            Received::Nothing => Got::Nothing,
            Received::Closed => Got::Closed,
        })
    }

    /// Reads one message and the descriptors that came with it, room made
    /// for one. A longer message, or more descriptors, sets a truncation
    /// flag; the kernel closes the descriptors that did not fit.
    fn receive(&self) -> io::Result<Received> {
        let mut bytes = [0u8; SIZE];
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let got = match rustix::net::recvmsg(
            &self.0,
            &mut [IoSliceMut::new(&mut bytes)],
            &mut control,
            RecvFlags::DONTWAIT | RecvFlags::CMSG_CLOEXEC,
        ) {
            Ok(got) => got,
            Err(rustix::io::Errno::AGAIN) => return Ok(Received::Nothing),
            Err(e) => return Err(e.into()),
        };
        let mut fds = Vec::new();
        for message in control.drain() {
            if let RecvAncillaryMessage::ScmRights(passed) = message {
                fds.extend(passed);
            }
        }
        if got.bytes == 0 && fds.is_empty() && self.hung_up()? {
            return Ok(Received::Closed);
        }
        Ok(Received::Message(Raw {
            bytes,
            len: got.bytes.min(SIZE),
            truncated: got
                .flags
                .intersects(ReturnFlags::TRUNC | ReturnFlags::CTRUNC),
            fds,
        }))
    }

    /// Whether the other end has closed the socket. A read of 0 bytes
    /// means either a closed socket or an empty message, which a
    /// `SOCK_SEQPACKET` socket carries like any other, so the reader asks
    /// `poll` without waiting: only a closed socket sets `POLLHUP`.
    fn hung_up(&self) -> io::Result<bool> {
        self.hung_up_within(0)
    }

    /// Whether the other end closes the socket within `seconds`.
    fn hung_up_within(&self, seconds: i64) -> io::Result<bool> {
        use rustix::event::{PollFd, PollFlags};
        let mut fds = [PollFd::new(&self.0, PollFlags::IN)];
        let bound = rustix::event::Timespec {
            tv_sec: seconds,
            tv_nsec: 0,
        };
        loop {
            match rustix::event::poll(&mut fds, Some(&bound)) {
                Ok(_) => break,
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(fds.iter().any(|fd| fd.revents().contains(PollFlags::HUP)))
    }

    /// Waits for the other end's close to reach this end. A test that
    /// closes one end waits for this, because a child that another test
    /// forks holds a copy of every descriptor until it runs its program.
    /// The wait is bounded only so that a test fails instead of hanging.
    #[cfg(test)]
    pub fn wait_for_close(&self) {
        assert!(
            matches!(self.hung_up_within(30), Ok(true)),
            "the other end stays open"
        );
    }
}

impl AsFd for Link {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn console() -> ConsoleRef {
        ConsoleRef {
            head: Head::new("main").unwrap(),
            console: 7,
        }
    }

    /// The longest head name vtium.conf allows, so each message is tried
    /// at its largest.
    fn long_console() -> ConsoleRef {
        ConsoleRef {
            head: Head::new(&"h".repeat(NAME_MAX)).unwrap(),
            console: u64::MAX,
        }
    }

    fn grid() -> Grid {
        Grid {
            cols: 80,
            rows: 25,
            cell_width: 8,
            cell_height: 16,
        }
    }

    /// Every request, at its largest.
    fn requests() -> Vec<Request> {
        vec![
            Request::OpenDevice {
                device: u32::MAX,
                path: DevicePath::new(&format!("/dev/{}", "d".repeat(PATH_MAX - 5))).unwrap(),
            },
            Request::OpenDevice {
                device: 1,
                path: DevicePath::new("/dev/input/event12").unwrap(),
            },
            Request::CloseDevice { device: 3 },
            Request::TakeMaster { device: 4 },
            Request::DropMaster { device: 4 },
            Request::Released,
            Request::SwitchVt { vt: 1 },
            Request::SwitchVt { vt: VT_MAX },
            Request::RestoreVt,
            Request::OpenSession {
                console: long_console(),
                grid: Grid {
                    cols: u16::MAX,
                    rows: u16::MAX,
                    cell_width: CELL_MAX,
                    cell_height: CELL_MAX,
                },
                display: Display::Fbdev,
                truecolor: true,
            },
            Request::OpenSession {
                console: console(),
                grid: grid(),
                display: Display::Kms,
                truecolor: false,
            },
            Request::CloseSession {
                console: long_console(),
            },
            Request::Winch { console: console() },
            Request::StartHelper {
                helper: 9,
                kind: Helper::Image(long_console()),
            },
            Request::StartHelper {
                helper: 10,
                kind: Helper::Font,
            },
            Request::StopHelper { helper: 9 },
            Request::Power {
                key: PowerKey::Reboot,
            },
            Request::Power {
                key: PowerKey::PowerOff,
            },
            Request::Power {
                key: PowerKey::Suspend,
            },
            Request::Ready,
        ]
    }

    /// Every order, at its largest.
    fn orders() -> Vec<Order> {
        vec![
            Order::Device {
                device: 2,
                opened: Ok(()),
            },
            Order::Device {
                device: u32::MAX,
                opened: Err(Errno(ERRNO_MAX)),
            },
            Order::Master {
                device: 2,
                taken: true,
            },
            Order::Master {
                device: 2,
                taken: false,
            },
            Order::Release,
            Order::Back,
            Order::Pty {
                console: long_console(),
                opened: Ok(()),
            },
            Order::Pty {
                console: console(),
                opened: Err(Errno(1)),
            },
            Order::Helper {
                helper: 9,
                started: Ok(()),
            },
            Order::Helper {
                helper: 9,
                started: Err(Errno(13)),
            },
            Order::SessionExit {
                console: long_console(),
                status: Status::Code(255),
            },
            Order::SessionExit {
                console: console(),
                status: Status::Signal(SIGNAL_MAX),
            },
            Order::HelperExit {
                helper: 9,
                status: Status::Code(0),
            },
            Order::HelperExit {
                helper: 9,
                status: Status::Signal(1),
            },
            Order::Blank,
            Order::Wake,
            Order::Stop,
        ]
    }

    /// Each request and each order encodes to [`SIZE`] bytes and decodes
    /// back to itself, the largest of each kind included, so no kind's
    /// fields run past the message's end.
    #[test]
    fn every_message_goes_round_and_comes_back_the_same() {
        for request in requests() {
            let bytes = request.encode();
            assert_eq!(Request::decode(&bytes), Ok(request.clone()), "{request:?}");
            assert!(
                Order::decode(&bytes).is_err(),
                "a request is never an order: {request:?}"
            );
        }
        for order in orders() {
            let bytes = order.encode();
            assert_eq!(Order::decode(&bytes), Ok(order.clone()), "{order:?}");
            assert!(
                Request::decode(&bytes).is_err(),
                "an order is never a request: {order:?}"
            );
        }
    }

    /// `bytes` with the little-endian `value` written at `at`.
    fn with(bytes: [u8; SIZE], at: usize, value: &[u8]) -> [u8; SIZE] {
        let mut b = bytes;
        b[at..at + value.len()].copy_from_slice(value);
        b
    }

    /// Where a console's fields start, after the 4-byte header, and where
    /// the console's id starts after the head's length byte and its room.
    const BODY: usize = 4;
    const AFTER_HEAD: usize = BODY + 1 + NAME_MAX;

    /// Each field of each request is refused outside its range, by the
    /// field's own name.
    #[test]
    fn each_request_field_is_refused_out_of_range() {
        let open = Request::OpenDevice {
            device: 1,
            path: DevicePath::new("/dev/dri/card0").unwrap(),
        }
        .encode();
        let session = Request::OpenSession {
            console: console(),
            grid: grid(),
            display: Display::Kms,
            truecolor: false,
        }
        .encode();
        // the grid follows the console: columns, rows, cell width, cell
        // height, then the display and truecolor
        let g = AFTER_HEAD + 8;
        let helper = Request::StartHelper {
            helper: 1,
            kind: Helper::Image(console()),
        }
        .encode();
        let path = |p: &[u8]| {
            let mut b = with(open, BODY + 4, &[u8::try_from(p.len()).unwrap()]);
            b[BODY + 5..BODY + 5 + PATH_MAX].fill(0);
            b[BODY + 5..BODY + 5 + p.len()].copy_from_slice(p);
            b
        };
        let head = |name: &[u8]| {
            let mut b = with(session, BODY, &[u8::try_from(name.len()).unwrap()]);
            b[BODY + 1..AFTER_HEAD].fill(0);
            let n = name.len().min(NAME_MAX);
            b[BODY + 1..BODY + 1 + n].copy_from_slice(&name[..n]);
            b
        };
        let cases: Vec<([u8; SIZE], Refusal)> = vec![
            (
                with(open, BODY, &0u32.to_le_bytes()),
                Refusal::Field("device"),
            ),
            (path(b"/etc/shadow"), Refusal::Field("path")),
            (path(b"/dev/../etc/shadow"), Refusal::Field("path")),
            (path(b"/dev/dri//card0"), Refusal::Field("path")),
            (path(b"/dev/dri/"), Refusal::Field("path")),
            (path(b"/dev/a b"), Refusal::Field("path")),
            (path(b""), Refusal::Field("path")),
            // a length past the field's room
            (with(open, BODY + 4, &[64]), Refusal::Field("path")),
            // a byte after the path's end
            (
                with(path(b"/dev/fb0"), BODY + 5 + 9, b"x"),
                Refusal::Field("path"),
            ),
            (
                with(Request::CloseDevice { device: 1 }.encode(), BODY, &[0; 4]),
                Refusal::Field("device"),
            ),
            (
                with(Request::TakeMaster { device: 1 }.encode(), BODY, &[0; 4]),
                Refusal::Field("device"),
            ),
            (
                with(Request::DropMaster { device: 1 }.encode(), BODY, &[0; 4]),
                Refusal::Field("device"),
            ),
            (
                with(Request::SwitchVt { vt: 1 }.encode(), BODY, &[0]),
                Refusal::Field("vt"),
            ),
            (
                with(Request::SwitchVt { vt: 1 }.encode(), BODY, &[VT_MAX + 1]),
                Refusal::Field("vt"),
            ),
            (head(b""), Refusal::Field("head")),
            (head(b"a/b"), Refusal::Field("head")),
            (head(&[0xff]), Refusal::Field("head")),
            (with(session, BODY, &[65]), Refusal::Field("head")),
            (
                with(session, g, &0u16.to_le_bytes()),
                Refusal::Field("columns"),
            ),
            (
                with(session, g + 2, &0u16.to_le_bytes()),
                Refusal::Field("rows"),
            ),
            (
                with(session, g + 4, &0u32.to_le_bytes()),
                Refusal::Field("cell width"),
            ),
            (
                with(session, g + 4, &(CELL_MAX + 1).to_le_bytes()),
                Refusal::Field("cell width"),
            ),
            (
                with(session, g + 8, &0u32.to_le_bytes()),
                Refusal::Field("cell height"),
            ),
            (
                with(session, g + 8, &(CELL_MAX + 1).to_le_bytes()),
                Refusal::Field("cell height"),
            ),
            (with(session, g + 12, &[0]), Refusal::Field("display")),
            (with(session, g + 12, &[3]), Refusal::Field("display")),
            (with(session, g + 13, &[2]), Refusal::Field("truecolor")),
            (
                with(
                    Request::CloseSession { console: console() }.encode(),
                    BODY,
                    &[0],
                ),
                Refusal::Field("head"),
            ),
            (
                with(Request::Winch { console: console() }.encode(), BODY, &[0]),
                Refusal::Field("head"),
            ),
            (with(helper, BODY, &[0; 4]), Refusal::Field("helper")),
            (with(helper, BODY + 4, &[0]), Refusal::Field("helper kind")),
            (with(helper, BODY + 4, &[3]), Refusal::Field("helper kind")),
            (with(helper, BODY + 5, &[0]), Refusal::Field("head")),
            (
                with(Request::StopHelper { helper: 1 }.encode(), BODY, &[0; 4]),
                Refusal::Field("helper"),
            ),
            (
                with(
                    Request::Power {
                        key: PowerKey::Reboot,
                    }
                    .encode(),
                    BODY,
                    &[0],
                ),
                Refusal::Field("power key"),
            ),
            (
                with(
                    Request::Power {
                        key: PowerKey::Reboot,
                    }
                    .encode(),
                    BODY,
                    &[4],
                ),
                Refusal::Field("power key"),
            ),
        ];
        for (bytes, refusal) in cases {
            assert_eq!(Request::decode(&bytes), Err(refusal), "{bytes:?}");
        }
    }

    /// Each field of each order is refused outside its range, by the
    /// field's own name.
    #[test]
    fn each_order_field_is_refused_out_of_range() {
        let device = Order::Device {
            device: 1,
            opened: Ok(()),
        }
        .encode();
        let exit = Order::SessionExit {
            console: console(),
            status: Status::Code(0),
        }
        .encode();
        let helper_exit = Order::HelperExit {
            helper: 1,
            status: Status::Code(0),
        }
        .encode();
        let pty = Order::Pty {
            console: console(),
            opened: Ok(()),
        }
        .encode();
        let helper = Order::Helper {
            helper: 1,
            started: Ok(()),
        }
        .encode();
        let master = Order::Master {
            device: 1,
            taken: true,
        }
        .encode();
        let errno = (ERRNO_MAX + 1).to_le_bytes();
        let cases: Vec<([u8; SIZE], Refusal)> = vec![
            (with(device, BODY, &[0; 4]), Refusal::Field("device")),
            (with(device, BODY + 4, &errno), Refusal::Field("errno")),
            (with(master, BODY, &[0; 4]), Refusal::Field("device")),
            (with(master, BODY + 4, &[2]), Refusal::Field("taken")),
            (with(pty, BODY, &[0]), Refusal::Field("head")),
            (with(pty, AFTER_HEAD + 8, &errno), Refusal::Field("errno")),
            (with(helper, BODY, &[0; 4]), Refusal::Field("helper")),
            (with(helper, BODY + 4, &errno), Refusal::Field("errno")),
            (with(exit, BODY, &[0]), Refusal::Field("head")),
            (with(exit, AFTER_HEAD + 8, &[0]), Refusal::Field("status")),
            (with(exit, AFTER_HEAD + 8, &[3]), Refusal::Field("status")),
            (
                with(exit, AFTER_HEAD + 8, &[2, 0]),
                Refusal::Field("signal"),
            ),
            (
                with(exit, AFTER_HEAD + 8, &[2, SIGNAL_MAX + 1]),
                Refusal::Field("signal"),
            ),
            (with(helper_exit, BODY, &[0; 4]), Refusal::Field("helper")),
            (with(helper_exit, BODY + 4, &[0]), Refusal::Field("status")),
            (
                with(helper_exit, BODY + 4, &[2, 0]),
                Refusal::Field("signal"),
            ),
        ];
        for (bytes, refusal) in cases {
            assert_eq!(Order::decode(&bytes), Err(refusal), "{bytes:?}");
        }
    }

    /// A message of another size, an unknown kind, a reserved byte set, or
    /// a byte set past a message's fields refuses the whole message.
    #[test]
    fn the_frame_around_the_fields_is_checked() {
        let ready = Request::Ready.encode();
        assert_eq!(
            Request::decode(&ready[..SIZE - 1]),
            Err(Refusal::Size(SIZE - 1))
        );
        assert_eq!(Request::decode(&[]), Err(Refusal::Size(0)));
        let mut longer = ready.to_vec();
        longer.push(0);
        assert_eq!(Request::decode(&longer), Err(Refusal::Size(SIZE + 1)));
        assert_eq!(
            Request::decode(&with(ready, 0, &[0, 0])),
            Err(Refusal::Kind(0))
        );
        assert_eq!(
            Request::decode(&with(ready, 0, &[15, 0])),
            Err(Refusal::Kind(15))
        );
        assert_eq!(
            Order::decode(&with(ready, 0, &[0x0c, 1])),
            Err(Refusal::Kind(0x10c))
        );
        assert_eq!(
            Request::decode(&with(ready, 2, &[1])),
            Err(Refusal::Padding)
        );
        assert_eq!(
            Request::decode(&with(ready, 3, &[1])),
            Err(Refusal::Padding)
        );
        assert_eq!(
            Request::decode(&with(ready, SIZE - 1, &[1])),
            Err(Refusal::Padding)
        );
        let close = Request::CloseDevice { device: 1 }.encode();
        assert_eq!(
            Request::decode(&with(close, 8, &[1])),
            Err(Refusal::Padding)
        );
        // a font helper's request carries no console
        let font = Request::StartHelper {
            helper: 1,
            kind: Helper::Font,
        }
        .encode();
        assert_eq!(Request::decode(&with(font, 9, &[1])), Err(Refusal::Padding));
        assert_eq!(
            Order::decode(&with(Order::Stop.encode(), 4, &[1])),
            Err(Refusal::Padding)
        );
    }

    /// Over a real socket pair: a request arrives whole and checked; an
    /// order and its descriptor arrive together; a request that comes
    /// with a descriptor, an order without the descriptor it carries, a
    /// short message, a long one and an empty one are each refused; and
    /// the end of the other side reads as closed.
    #[test]
    fn the_socket_carries_messages_and_descriptors_and_refuses_the_rest() {
        let (broker, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        assert!(matches!(broker.next_request().unwrap(), Got::Nothing));
        drawer.request(&Request::Ready).unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::Ready))
        ));

        let (file, _keep) = Link::pair().unwrap();
        let device = Order::Device {
            device: 5,
            opened: Ok(()),
        };
        broker.order(&device, Some(file.as_fd())).unwrap();
        match drawer.next_order().unwrap() {
            Got::Message(Ok((order, Some(_fd)))) => assert_eq!(order, device),
            other => panic!("{other:?}"),
        }
        assert!(broker.order(&device, None).is_err());
        assert!(broker.order(&Order::Stop, Some(file.as_fd())).is_err());

        // what a hostile drawer could send: a descriptor with a request
        drawer
            .send(&Request::Ready.encode(), Some(file.as_fd()))
            .unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Err(Refused {
                why: Refusal::Descriptors(1),
                answer: None
            }))
        ));
        // and an order whose descriptor is missing
        broker.send(&device.encode(), None).unwrap();
        assert!(matches!(
            drawer.next_order().unwrap(),
            Got::Message(Err(Refusal::Descriptors(0)))
        ));
        rustix::net::send(&drawer, &[1u8; 5], SendFlags::empty()).unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Err(Refused {
                why: Refusal::Size(5),
                answer: None
            }))
        ));
        rustix::net::send(&drawer, &[0u8; SIZE + 1], SendFlags::empty()).unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Err(Refused {
                why: Refusal::Truncated,
                answer: None
            }))
        ));
        // A SOCK_SEQPACKET socket carries an empty message, which reads 0
        // bytes as a closed socket does. While the drawer's end stays open,
        // it is a message of the wrong size, and the socket still works.
        rustix::net::send(&drawer, &[], SendFlags::empty()).unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Err(Refused {
                why: Refusal::Size(0),
                answer: None
            }))
        ));
        drawer.request(&Request::Ready).unwrap();
        assert!(matches!(
            broker.next_request().unwrap(),
            Got::Message(Ok(Request::Ready))
        ));
        drop(drawer);
        broker.wait_for_close();
        assert!(matches!(broker.next_request().unwrap(), Got::Closed));
    }

    /// A refused request whose kind waits for an answer gets one, with
    /// the request's own id and `EINVAL`, whichever field the reader
    /// refused and even over a socket that cut the message short. A
    /// request whose id is out of range, or whose kind waits for no
    /// answer, gets none.
    #[test]
    fn a_refused_request_that_waits_for_an_answer_gets_an_error() {
        assert_eq!(i32::from(EINVAL), libc::EINVAL);
        let einval = Err(Errno(EINVAL));
        let console = ConsoleRef {
            head: Head::new("left").unwrap(),
            console: 7,
        };
        let path = DevicePath::new("/dev/dri/card0").unwrap();
        let cases = [
            (
                Request::OpenDevice {
                    device: 3,
                    path: path.clone(),
                },
                Order::Device {
                    device: 3,
                    opened: einval,
                },
            ),
            (
                Request::TakeMaster { device: 4 },
                Order::Master {
                    device: 4,
                    taken: false,
                },
            ),
            (
                Request::OpenSession {
                    console: console.clone(),
                    grid: Grid {
                        cols: 80,
                        rows: 24,
                        cell_width: 8,
                        cell_height: 16,
                    },
                    display: Display::Kms,
                    truecolor: true,
                },
                Order::Pty {
                    console: console.clone(),
                    opened: einval,
                },
            ),
            (
                Request::StartHelper {
                    helper: 6,
                    kind: Helper::Font,
                },
                Order::Helper {
                    helper: 6,
                    started: einval,
                },
            ),
        ];
        for (request, answer) in cases {
            let mut bytes = request.encode();
            // a byte set past the fields
            bytes[SIZE - 1] = 1;
            assert_eq!(Request::decode(&bytes), Err(Refusal::Padding));
            assert_eq!(Request::refused_answer(&bytes), Some(answer.clone()));
            // the kind and the id only, as from a message cut short
            let short = &request.encode()[..12];
            if !matches!(request, Request::OpenSession { .. }) {
                assert_eq!(Request::refused_answer(short), Some(answer));
            }
        }
        // an id of 0, which the drawer never chooses
        let mut zero = Request::TakeMaster { device: 1 }.encode();
        zero[4] = 0;
        assert_eq!(Request::refused_answer(&zero), None);
        // a session whose head name vtium.conf does not allow
        let session = Request::OpenSession {
            console: console.clone(),
            grid: Grid {
                cols: 80,
                rows: 24,
                cell_width: 8,
                cell_height: 16,
            },
            display: Display::Kms,
            truecolor: true,
        };
        let mut bad_head = session.encode();
        bad_head[5] = b'/';
        assert_eq!(Request::decode(&bad_head), Err(Refusal::Field("head")));
        assert_eq!(Request::refused_answer(&bad_head), None);
        // kinds that wait for no answer, and an unknown kind
        let mut vt = Request::SwitchVt { vt: 1 }.encode();
        vt[4] = 0;
        assert_eq!(Request::refused_answer(&vt), None);
        assert_eq!(Request::refused_answer(&[0xff; SIZE]), None);
        assert_eq!(Request::refused_answer(&[]), None);
        // the broker's reader hands the answer on with the refusal
        let (broker, theirs) = Link::pair().unwrap();
        let drawer = Link::passed(theirs).unwrap();
        let mut bytes = Request::OpenDevice { device: 3, path }.encode();
        bytes[9] = b'e';
        rustix::net::send(&drawer, &bytes, SendFlags::empty()).unwrap();
        match broker.next_request().unwrap() {
            Got::Message(Err(refused)) => {
                assert_eq!(refused.why, Refusal::Field("path"));
                assert_eq!(
                    refused.answer,
                    Some(Order::Device {
                        device: 3,
                        opened: einval
                    })
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// Each seed of the fuzz target is a message one reader accepts, so a
    /// run starts from whole messages of every layout the seeds cover.
    #[test]
    fn the_fuzz_seeds_are_accepted_messages() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/seeds/protocol");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(
                Request::decode(&bytes).is_ok() || Order::decode(&bytes).is_ok(),
                "{bytes:?}"
            );
            seen += 1;
        }
        assert_eq!(seen, 10);
    }

    /// The drawer takes descriptor 3 only when it is a Unix
    /// `SOCK_SEQPACKET` socket.
    #[test]
    fn the_drawer_takes_only_a_seqpacket_socket() {
        let (a, _b) = rustix::net::socketpair(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        assert!(Link::passed(a).is_err());
        let file = std::fs::File::open("/dev/null").unwrap();
        assert!(Link::passed(file.into()).is_err());
    }
}
