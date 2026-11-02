//! Packets with descriptors on a Unix socket pair of the sequenced-packet
//! kind, and the two packets between the helper and its workers.
//!
//! The helper sends a worker the request, encoded as vtium sent it, with
//! the reply's memory file and, for inline bytes, their memory file. The
//! worker answers with its [`Outcome`]: the status, the picture's size,
//! the bytes ignored, the reason, and, for a shared-memory object it read
//! a valid picture from, the object's device and inode, which the helper
//! needs to remove it. The outcome also says what the worker found of its
//! own sandbox, which the helper checks (ADR-1101 D9).

use std::io::{IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use vtium_image::decode::wire::{REASON_MAX, Status};

use crate::files::Identity;

/// The most descriptors a packet is read with: more than any packet
/// carries, so one too many is seen and refused rather than dropped.
pub const FDS_MAX: usize = 4;

/// One packet received: its bytes and the descriptors it carried.
#[derive(Debug)]
pub struct Packet {
    pub bytes: Vec<u8>,
    pub fds: Vec<OwnedFd>,
}

/// Why a packet could not be received.
#[derive(Debug, PartialEq, Eq)]
pub enum Received {
    /// The other end closed the socket.
    Closed,
    /// The packet or its descriptors did not fit, or the socket failed.
    Broken(String),
}

/// Sends `bytes` with `fds` as one packet, never raising SIGPIPE.
pub fn send(socket: impl AsFd, bytes: &[u8], fds: &[BorrowedFd<'_>]) -> std::io::Result<()> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(FDS_MAX))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !fds.is_empty() && !control.push(SendAncillaryMessage::ScmRights(fds)) {
        return Err(std::io::Error::other("too many descriptors for one packet"));
    }
    loop {
        match rustix::net::sendmsg(
            &socket,
            &[IoSlice::new(bytes)],
            &mut control,
            SendFlags::NOSIGNAL,
        ) {
            Ok(n) if n == bytes.len() => return Ok(()),
            Ok(_) => return Err(std::io::Error::other("a packet sent in part")),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Receives one packet of at most `max` bytes, its descriptors made
/// close-on-exec as they arrive. A packet longer than `max`, or with more
/// descriptors than [`FDS_MAX`], is broken.
pub fn receive(socket: impl AsFd, max: usize) -> Result<Packet, Received> {
    let mut bytes = vec![0u8; max.saturating_add(1)];
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(FDS_MAX + 1))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let got = loop {
        match rustix::net::recvmsg(
            &socket,
            &mut [IoSliceMut::new(&mut bytes)],
            &mut control,
            RecvFlags::CMSG_CLOEXEC,
        ) {
            Ok(got) => break got,
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(Received::Broken(format!("the socket failed: {e}"))),
        }
    };
    let mut fds = Vec::new();
    let mut foreign = false;
    for message in control.drain() {
        match message {
            RecvAncillaryMessage::ScmRights(received) => fds.extend(received),
            _ => foreign = true,
        }
    }
    if got
        .flags
        .intersects(ReturnFlags::TRUNC | ReturnFlags::CTRUNC)
        || got.bytes > max
    {
        return Err(Received::Broken(
            "a packet too long or with too many descriptors".to_owned(),
        ));
    }
    if foreign {
        return Err(Received::Broken(
            "a packet with a control message other than descriptors".to_owned(),
        ));
    }
    if got.bytes == 0 && fds.is_empty() {
        return Err(Received::Closed);
    }
    bytes.truncate(got.bytes);
    Ok(Packet { bytes, fds })
}

/// What a worker reports for one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: Status,
    pub width: u32,
    pub height: u32,
    /// Bytes of the source the worker did not use.
    pub ignored: u64,
    /// The shared-memory object a valid picture was read from, which the
    /// helper removes.
    pub shm: Option<Identity>,
    /// A pooled reply file was of another size than the picture, so the
    /// worker wrote nothing (ADR-1101 D4).
    pub mismatch: bool,
    /// The worker could open no directory when it answered: its landlock
    /// domain or its third filter was in force.
    pub locked: bool,
    /// The seccomp filters the worker ran under once its second was in,
    /// 0 where the kernel does not count them or none is built.
    pub filters: u32,
    pub reason: String,
}

impl Outcome {
    /// An outcome other than OK, for `reason`.
    pub fn failed(status: Status, reason: impl Into<String>) -> Self {
        Self {
            status,
            width: 0,
            height: 0,
            ignored: 0,
            shm: None,
            mismatch: false,
            locked: false,
            filters: 0,
            reason: reason.into(),
        }
    }
}

/// The bytes of an outcome before its reason.
const OUTCOME_FIXED: usize = 48;
/// The most bytes an outcome takes.
pub const OUTCOME_MAX: usize = OUTCOME_FIXED + REASON_MAX;

/// The outcome's packet; the reason is cut at [`REASON_MAX`] bytes, on a
/// character's boundary.
pub fn encode_outcome(o: &Outcome) -> Vec<u8> {
    let mut end = o.reason.len().min(REASON_MAX);
    while !o.reason.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let reason = o.reason.get(..end).unwrap_or_default();
    let (dev, ino) = o.shm.map_or((0, 0), |id| (id.dev, id.ino));
    let flags = u32::from(o.shm.is_some())
        | u32::from(o.mismatch).wrapping_shl(1)
        | u32::from(o.locked).wrapping_shl(2);
    let mut out = Vec::with_capacity(OUTCOME_FIXED.saturating_add(reason.len()));
    for v in [
        o.status as u32,
        o.width,
        o.height,
        u32::try_from(reason.len()).unwrap_or(0),
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [o.ignored, dev, ino] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&o.filters.to_le_bytes());
    out.extend_from_slice(reason.as_bytes());
    out
}

/// The outcome in `packet`, or `None` for one that is malformed.
pub fn decode_outcome(packet: &[u8]) -> Option<Outcome> {
    let u32_at = |at: usize| {
        packet
            .get(at..at.checked_add(4)?)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_le_bytes)
    };
    let u64_at = |at: usize| {
        packet
            .get(at..at.checked_add(8)?)
            .and_then(|b| <[u8; 8]>::try_from(b).ok())
            .map(u64::from_le_bytes)
    };
    let status = Status::from_wire(u32_at(0)?)?;
    let reason_len = usize::try_from(u32_at(12)?).ok()?;
    if reason_len > REASON_MAX || OUTCOME_FIXED.checked_add(reason_len)? != packet.len() {
        return None;
    }
    let flags = u32_at(40)?;
    if flags & !7 != 0 {
        return None;
    }
    let shm = (flags & 1 != 0).then(|| Identity {
        dev: u64_at(24).unwrap_or(0),
        ino: u64_at(32).unwrap_or(0),
    });
    Some(Outcome {
        status,
        width: u32_at(4)?,
        height: u32_at(8)?,
        ignored: u64_at(16)?,
        shm,
        mismatch: flags & 2 != 0,
        locked: flags & 4 != 0,
        filters: u32_at(44)?,
        reason: String::from_utf8_lossy(packet.get(OUTCOME_FIXED..)?).into_owned(),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use rustix::net::{AddressFamily, SocketFlags, SocketType};

    fn pair() -> (OwnedFd, OwnedFd) {
        rustix::net::socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_packet_and_its_descriptors_arrive_whole_and_a_long_one_is_refused() {
        let (a, b) = pair();
        let memfd = rustix::fs::memfd_create("t", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
        send(&a, b"hello", &[memfd.as_fd()]).unwrap();
        let got = receive(&b, 16).unwrap();
        assert_eq!((got.bytes.as_slice(), got.fds.len()), (&b"hello"[..], 1));
        send(&a, &[0; 17], &[]).unwrap();
        assert!(matches!(receive(&b, 16), Err(Received::Broken(_))));
        drop(a);
        assert_eq!(receive(&b, 16).unwrap_err(), Received::Closed);
    }

    #[test]
    fn an_outcome_comes_back_field_for_field() {
        let o = Outcome {
            status: Status::Ok,
            width: 3,
            height: 4,
            ignored: 5,
            shm: Some(Identity { dev: 6, ino: 7 }),
            mismatch: true,
            locked: true,
            filters: 2,
            reason: "é".repeat(200),
        };
        let packet = encode_outcome(&o);
        let back = decode_outcome(&packet).unwrap();
        assert_eq!(back.reason.len(), 254, "cut on a character's boundary");
        assert_eq!(
            Outcome {
                reason: o.reason.clone(),
                ..back
            },
            o
        );
        assert_eq!(decode_outcome(&packet[..packet.len() - 1]), None);
        let mut bad = packet.clone();
        bad[40] = 8;
        assert_eq!(decode_outcome(&bad), None);
    }
}
