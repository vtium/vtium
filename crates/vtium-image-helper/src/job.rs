//! One request, as a worker carries it out (ADR-1101 D4, D5, D6): it opens
//! what the request names under the file rules and removes a temporary
//! file that must go, then, once it has locked itself in, reads, inflates
//! or decodes, converts the picture into the store's format and writes
//! the reply's memory file.
//!
//! The steps are split at the lock: [`open`] runs before the worker enters
//! its landlock domain and [`finish`] after it, so that a decoder runs only
//! once the worker can open nothing more. [`finish`] takes the worker's
//! proof of its lock, [`Locked`], which only the lock makes. Neither knows a protocol's keys;
//! each follows the request's mechanical fields.

use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::SealFlags;
use vtium_image::decode::wire::Status;
use vtium_image::decode::{Medium, Request, convert};

use crate::channel::Outcome;
use crate::decode::{self, Bounds};
use crate::files::{self, Opened};
use crate::inflate::{Goal, inflate_bounded};
use crate::sandbox::Locked;

/// An outcome other than OK and its reason, for vtium's debug log.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub status: Status,
    pub reason: String,
}

impl Failure {
    pub fn new(status: Status, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }

    fn unreadable(reason: impl Into<String>) -> Self {
        Self::new(Status::Unreadable, reason)
    }
}

/// What a worker reads from: the inline bytes' memory file, or a file or
/// shared-memory object it opened.
#[derive(Debug)]
pub enum Source {
    Inline(OwnedFd),
    Opened(Opened),
    /// A discard, which read nothing and is done.
    Discarded(String),
}

/// The request's source opened, before the worker locks itself in: inline
/// bytes take their memory file; a file or a temporary file is opened under
/// the file rules, and a temporary file is removed once opened, whatever
/// becomes of the read; a shared-memory object is opened under
/// `/dev/shm`. A discard removes a temporary file that passes the
/// temporary-file rule and reads nothing; a shared-memory object is never
/// removed by a discard, since no valid picture was read from it.
pub fn open(
    request: &Request,
    inline: Option<OwnedFd>,
    tmpdir: Option<&Path>,
) -> Result<Source, Failure> {
    let name = request.medium.name();
    if request.discard {
        return Ok(Source::Discarded(discard(request, tmpdir)));
    }
    match &request.medium {
        Medium::Direct(_) => inline
            .map(Source::Inline)
            .ok_or_else(|| Failure::unreadable("no inline bytes")),
        Medium::File(_) | Medium::TempFile(_) => {
            let temp = matches!(request.medium, Medium::TempFile(_));
            let (opened, _) = files::open_file(name, temp, tmpdir).map_err(Failure::unreadable)?;
            if temp {
                // a failure to remove leaves the file and changes no reply
                let _ = files::remove_checked(&opened.resolved, opened.id, files::me());
            }
            Ok(Source::Opened(opened))
        }
        Medium::SharedMemory(_) => files::open_shm(name)
            .map(Source::Opened)
            .map_err(Failure::unreadable),
    }
}

/// A refused command's object removed as the rules allow, and why.
fn discard(request: &Request, tmpdir: Option<&Path>) -> String {
    match &request.medium {
        Medium::TempFile(name) => match files::open_file(name, false, tmpdir) {
            Ok((opened, true)) => {
                match files::remove_checked(&opened.resolved, opened.id, files::me()) {
                    Ok(()) => format!("discarded {}", opened.resolved.display()),
                    Err(why) => format!("not removed: {why}"),
                }
            }
            Ok((opened, false)) => format!(
                "not removed, not a temporary file: {}",
                opened.resolved.display()
            ),
            Err(why) => format!("not removed: {why}"),
        },
        _ => "a discard of shared memory removes nothing".to_owned(),
    }
}

/// Where the reply's pixels go: a fresh memory file the worker sizes and
/// seals, or a pooled one of a stream, already sized, which is written and
/// never sealed against writing.
#[derive(Debug)]
pub enum ReplyFile {
    Fresh(OwnedFd),
    Pooled(OwnedFd),
}

/// The request carried out from its opened source, once the worker has
/// locked itself in: read, inflated, decoded or converted, and written
/// into `reply`, which only a discard lacks.
pub fn finish(
    request: &Request,
    source: Source,
    reply: Option<&ReplyFile>,
    locked: &Locked,
) -> Outcome {
    let (file, size, shm) = match source {
        Source::Discarded(why) => return Outcome::failed(Status::Ok, why),
        Source::Inline(fd) => {
            let size = rustix::fs::fstat(&fd)
                .ok()
                .and_then(|st| u64::try_from(st.st_size).ok());
            match size {
                Some(size) => (fd, size, None),
                None => return Outcome::failed(Status::Unreadable, "cannot read the inline bytes"),
            }
        }
        Source::Opened(opened) => {
            let shm = matches!(request.medium, Medium::SharedMemory(_)).then_some(opened.id);
            (opened.file, opened.size, shm)
        }
    };
    let Some(reply) = reply else {
        return Outcome::failed(Status::Unreadable, "no reply file");
    };
    match picture(request, &file, size, locked) {
        Ok((width, height, rgba, ignored)) => match write_reply(reply, &rgba) {
            Ok(true) => Outcome {
                status: Status::Ok,
                width,
                height,
                ignored,
                shm,
                mismatch: false,
                locked: false,
                filters: 0,
                reason: String::new(),
            },
            Ok(false) => Outcome {
                mismatch: true,
                ..Outcome::failed(
                    Status::OverBudget,
                    "a frame of another size than its stream",
                )
            },
            Err(e) => Outcome::failed(Status::OverBudget, format!("cannot write the reply: {e}")),
        },
        Err(f) => Outcome::failed(f.status, f.reason),
    }
}

/// The picture of `request` from `file` of `size` bytes: its width and
/// height, its premultiplied pixels and the bytes of the source unused.
fn picture(
    request: &Request,
    file: impl AsFd,
    size: u64,
    locked: &Locked,
) -> Result<(u32, u32, Vec<u8>, u64), Failure> {
    let (data, mut ignored) = read_bounded(request, &file, size)?;
    let bounds = Bounds {
        largest: request.largest,
        max_bytes: request.largest_bytes,
    };
    let data = if request.inflate {
        let goal = if request.exact {
            Goal::Exact(request.min)
        } else if request.pixels.is_some() {
            Goal::Stop(request.min)
        } else {
            Goal::Budget {
                min: request.min,
                max: request.largest_bytes,
            }
        };
        let inflated = inflate_bounded(&data, goal, locked)?;
        drop(data);
        ignored = inflated.rest;
        inflated.bytes
    } else {
        data
    };
    if let Some(d) = request.pixels {
        let len = bounds.check(d.width, d.height)?;
        let rgba = convert::into_reply(d.layout, d.alpha, data, len / 4)
            .ok_or_else(|| Failure::new(Status::TooShort, "fewer bytes than the picture needs"))?;
        return Ok((d.width, d.height, rgba, ignored));
    }
    let pic = if request.png {
        decode::png(&data, bounds, locked)?
    } else {
        decode::any(&data, bounds, locked)?
    };
    if !request.inflate {
        ignored = pic.rest;
    }
    Ok((pic.width, pic.height, pic.rgba, ignored))
}

/// The bytes the request reads from `file` of `size` bytes, and those it
/// leaves unread: from the offset to the lesser of the offset plus the
/// most and the end, or to the end when the most is 0. An offset at or
/// past the end of a file is `UNREADABLE`. Raw pixels read only the bytes
/// the picture needs, and fewer available is `TOO_SHORT` before anything
/// is read; a decode or an inflate reads all the bounds allow, and more
/// than `max_bytes` is `OVER_BUDGET` before it is read.
pub fn read_bounded(
    request: &Request,
    file: impl AsFd,
    size: u64,
) -> Result<(Vec<u8>, u64), Failure> {
    let named = !matches!(request.medium, Medium::Direct(_));
    let offset = if named { request.offset } else { 0 };
    if named && offset >= size {
        return Err(Failure::unreadable(format!(
            "offset {offset} at or past the end, {size} bytes"
        )));
    }
    let mut avail = size.saturating_sub(offset);
    if named && request.max != 0 {
        avail = avail.min(request.max);
    }
    let mut ignored = 0;
    if !(request.inflate || request.png || request.any_image) {
        if avail < request.min {
            return Err(Failure::new(
                Status::TooShort,
                format!("{avail} bytes to read, {} needed", request.min),
            ));
        }
        ignored = avail.saturating_sub(request.min);
        avail = request.min;
    } else if avail > request.largest_bytes {
        return Err(Failure::new(
            Status::OverBudget,
            format!("{avail} bytes to read, past the budget"),
        ));
    }
    let len =
        usize::try_from(avail).map_err(|_| Failure::new(Status::OverBudget, "past memory"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(len)
        .map_err(|_| Failure::new(Status::OverBudget, format!("cannot allocate {len} bytes")))?;
    data.resize(len, 0);
    let mut done = 0usize;
    while done < len {
        let at = offset.saturating_add(u64::try_from(done).unwrap_or(u64::MAX));
        match rustix::io::pread(&file, data.get_mut(done..).unwrap_or_default(), at) {
            Ok(0) => return Err(Failure::unreadable("a short read: the end of the file")),
            Ok(n) => done = done.saturating_add(n),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(Failure::unreadable(format!("a short read: {e}"))),
        }
    }
    Ok((data, ignored))
}

/// Writes all of `data` into `fd` at its start with `pwrite`.
fn pwrite_all(fd: impl AsFd, data: &[u8]) -> std::io::Result<()> {
    let mut done = 0usize;
    while done < data.len() {
        let at = u64::try_from(done).unwrap_or(u64::MAX);
        match rustix::io::pwrite(&fd, data.get(done..).unwrap_or_default(), at) {
            Ok(0) => return Err(std::io::Error::other("no space")),
            Ok(n) => done = done.saturating_add(n),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// The picture written into the reply's memory file: a fresh one sized to
/// it, written and sealed against shrinking, growing and writing; a pooled
/// one written only when its size is the picture's, which `Ok(false)`
/// reports otherwise.
pub fn write_reply(reply: &ReplyFile, rgba: &[u8]) -> std::io::Result<bool> {
    let len = u64::try_from(rgba.len()).unwrap_or(u64::MAX);
    match reply {
        ReplyFile::Fresh(fd) => {
            rustix::fs::ftruncate(fd, len)?;
            pwrite_all(fd, rgba)?;
            rustix::fs::fcntl_add_seals(
                fd,
                SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE,
            )?;
            Ok(true)
        }
        ReplyFile::Pooled(fd) => {
            let size = u64::try_from(rustix::fs::fstat(fd)?.st_size).unwrap_or(0);
            if size != len {
                return Ok(false);
            }
            pwrite_all(fd, rgba)?;
            Ok(true)
        }
    }
}

/// A memory file for a reply or for inline bytes, made with sealing
/// allowed and the kernel's no-execute seal, or without that seal on a
/// kernel before 6.3, which refuses the flag with EINVAL (ADR-1101 D7).
pub fn memory_file(name: &str) -> std::io::Result<OwnedFd> {
    use rustix::fs::MemfdFlags;
    let flags = MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING;
    match rustix::fs::memfd_create(name, flags | MemfdFlags::NOEXEC_SEAL) {
        Err(rustix::io::Errno::INVAL) => Ok(rustix::fs::memfd_create(name, flags)?),
        other => Ok(other?),
    }
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
    use vtium_image::decode::{Alpha, Description, Layout};

    pub(crate) fn raw(medium: Medium, layout: Layout, w: u32, h: u32) -> Request {
        let min = u64::from(w) * u64::from(h) * layout.bytes() as u64;
        Request {
            medium,
            offset: 0,
            max: 0,
            min,
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            any_image: false,
            pixels: Some(Description {
                layout,
                alpha: Alpha::Straight,
                width: w,
                height: h,
                compressed: false,
            }),
            largest: (100, 100),
            largest_bytes: 1 << 20,
        }
    }

    fn inline(data: &[u8]) -> OwnedFd {
        let fd = memory_file("t").unwrap();
        pwrite_all(&fd, data).unwrap();
        fd
    }

    fn sealed_bytes(fd: &OwnedFd) -> Vec<u8> {
        let seals = rustix::fs::fcntl_get_seals(fd).unwrap();
        assert!(seals.contains(SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE));
        let size = rustix::fs::fstat(fd).unwrap().st_size as usize;
        let mut out = vec![0; size];
        rustix::io::pread(fd, &mut out, 0).unwrap();
        out
    }

    #[test]
    fn described_pixels_come_back_premultiplied_in_a_sealed_file() {
        let mut r = raw(Medium::Direct(Vec::new()), Layout::Rgba, 2, 1);
        r.inflate = true;
        r.pixels.as_mut().unwrap().compressed = true;
        let z = miniz_oxide::deflate::compress_to_vec_zlib(
            &[0xff, 0x80, 0, 0xff, 0xff, 0xff, 0xff, 0x80],
            6,
        );
        let reply = ReplyFile::Fresh(memory_file("r").unwrap());
        let source = open(&r, Some(inline(&z)), None).unwrap();
        let o = finish(&r, source, Some(&reply), &Locked::unsandboxed());
        assert_eq!(
            (o.status, o.width, o.height),
            (Status::Ok, 2, 1),
            "{}",
            o.reason
        );
        let ReplyFile::Fresh(fd) = &reply else {
            unreachable!()
        };
        assert_eq!(
            sealed_bytes(fd),
            [0xff, 0x80, 0, 0xff, 0x80, 0x80, 0x80, 0x80]
        );
    }

    #[test]
    fn a_read_takes_its_bounds_and_counts_what_it_leaves() {
        let file = inline(b"0123456789");
        let mut r = raw(Medium::File(b"/x".to_vec()), Layout::Rgb, 1, 1);
        r.offset = 2;
        r.max = 5;
        assert_eq!(read_bounded(&r, &file, 10).unwrap(), (b"234".to_vec(), 2));
        r.offset = 10;
        assert_eq!(
            read_bounded(&r, &file, 10).unwrap_err().status,
            Status::Unreadable
        );
        r.offset = 8;
        assert_eq!(
            read_bounded(&r, &file, 10).unwrap_err().status,
            Status::TooShort
        );
        let mut png = r.clone();
        png.pixels = None;
        png.png = true;
        png.min = 0;
        png.offset = 0;
        png.max = 0;
        png.largest_bytes = 9;
        assert_eq!(
            read_bounded(&png, &file, 10).unwrap_err().status,
            Status::OverBudget
        );
    }

    #[test]
    fn a_pooled_file_of_another_size_is_written_nothing() {
        let fd = memory_file("p").unwrap();
        rustix::fs::ftruncate(&fd, 8).unwrap();
        let pooled = ReplyFile::Pooled(fd);
        assert!(!write_reply(&pooled, &[1; 4]).unwrap());
        assert!(write_reply(&pooled, &[1; 8]).unwrap());
        // twice, as a stream's next frame writes it again
        assert!(write_reply(&pooled, &[2; 8]).unwrap());
    }

    #[test]
    fn a_shared_memory_picture_reports_the_object_it_came_from() {
        /// The object, removed when the test ends, whether it passes or
        /// panics.
        struct Object(std::path::PathBuf);
        impl Drop for Object {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let name = format!("vtium-image-helper-test-{}", std::process::id());
        let object = Object(Path::new("/dev/shm").join(&name));
        std::fs::write(&object.0, [1, 2, 3, 4, 5, 6]).unwrap();
        let r = raw(
            Medium::SharedMemory(name.clone().into_bytes()),
            Layout::Rgb,
            2,
            1,
        );
        let source = open(&r, None, None).unwrap();
        let o = finish(
            &r,
            source,
            Some(&ReplyFile::Fresh(memory_file("r").unwrap())),
            &Locked::unsandboxed(),
        );
        assert_eq!(o.status, Status::Ok, "{}", o.reason);
        assert!(o.shm.is_some());
        // too short, so no valid picture and no object to remove
        let r = raw(Medium::SharedMemory(name.into_bytes()), Layout::Rgb, 3, 1);
        let source = open(&r, None, None).unwrap();
        let o = finish(
            &r,
            source,
            Some(&ReplyFile::Fresh(memory_file("r").unwrap())),
            &Locked::unsandboxed(),
        );
        assert_eq!((o.status, o.shm), (Status::TooShort, None));
    }
}
