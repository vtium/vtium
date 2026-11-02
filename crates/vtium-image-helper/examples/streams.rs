//! The measurement ADR-1101 D4 asks for before any way of serving a stream
//! is built past the first: the time a frame takes, for each way, on real
//! frames, at 1366x768 and 7680x2160.
//!
//! 1. A fresh worker and a fresh reply file for each frame.
//! 2. A fresh worker for each frame, writing into a reply file from a pool
//!    of `REPLY_POOL_FILES`, faulted in once.
//! 3. One long-lived worker for the stream, locked in before it reads
//!    anything, writing into the pool.
//!
//! It compares 1 with 2 on raw 32-bit frames read from shared memory
//! (`t=s,f=32`), and 3 with 1 and 2 on inline frames, the only frames 3
//! serves: raw 32-bit frames compressed with zlib (`f=32,o=z`) and PNG
//! frames as timg sends them. Each worker runs the real request path
//! (`job::open` and `job::finish`) with the worker's second seccomp filter
//! and its landlock domain, behind the real channel. A frame's time runs
//! from sending the worker its request to its outcome, with the fork, the
//! reply file and the worker's setup counted for the ways that pay them;
//! the console's copy of the reply, the same for every way, is left out.
//!
//! Run it with `cargo run --release -p vtium-image-helper --example
//! streams`; `--frames N` sets the frames each case times.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "a measurement fails by panicking and reads the clock"
)]

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::time::{Duration, Instant};

use rustix::net::{AddressFamily, SocketFlags, SocketType};
use vtium_image::decode::wire::{self, PACKET_MAX, Status};
use vtium_image::decode::{Alpha, Description, Layout, Medium, Request};
use vtium_image_helper::bounds::REPLY_POOL_FILES;
use vtium_image_helper::channel::{self, Outcome};
use vtium_image_helper::fork::{Forked, fork, one_thread};
use vtium_image_helper::job::{self, ReplyFile};
use vtium_image_helper::sandbox;

/// How a stream is served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Fresh,
    Pool,
    Stream,
}

/// A frame of video-like content: gradients with a moving band and some
/// noise, RGBA, so zlib and PNG compress it as they would a real frame.
fn frame(w: u32, h: u32, n: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    let mut seed = 0x1234_5678u32.wrapping_add(n);
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let noise = (seed >> 28) as u8;
            let band = if (x + n * 7) % 97 < 12 { 0x60 } else { 0 };
            out.extend_from_slice(&[
                ((x * 255 / w) as u8).wrapping_add(noise),
                ((y * 255 / h) as u8).wrapping_add(band),
                ((x + y + n) % 256) as u8,
                0xff,
            ]);
        }
    }
    out
}

fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(rgba).unwrap();
    writer.finish().unwrap();
    out
}

fn base(medium: Medium, w: u32, h: u32) -> Request {
    Request {
        medium,
        offset: 0,
        max: 0,
        min: 0,
        inflate: false,
        png: false,
        exact: false,
        discard: false,
        any_image: false,
        pixels: None,
        largest: (w, h),
        largest_bytes: u64::from(w) * u64::from(h) * 4 * 2,
    }
}

fn raw(medium: Medium, w: u32, h: u32, compressed: bool) -> Request {
    Request {
        min: u64::from(w) * u64::from(h) * 4,
        inflate: compressed,
        pixels: Some(Description {
            layout: Layout::Rgba,
            alpha: Alpha::Straight,
            width: w,
            height: h,
            compressed,
        }),
        ..base(medium, w, h)
    }
}

/// A worker on `channel`: its second filter, then requests carried out
/// into a fresh or a pooled reply file. A stream worker enters its
/// landlock domain before it reads anything; any other enters it after
/// it opens its source, and handles one request.
fn worker(channel: &OwnedFd, way: Way, filter: &sandbox::WorkerFilter) -> i32 {
    if filter.install().is_err() {
        return 1;
    }
    if way == Way::Stream && sandbox::lock_in().is_err() {
        return 1;
    }
    loop {
        let packet = match channel::receive(channel, PACKET_MAX) {
            Ok(p) => p,
            Err(_) => return 0,
        };
        let (_, request) = wire::decode_request(&packet.bytes).unwrap();
        let mut fds = packet.fds.into_iter();
        let reply = fds.next().unwrap();
        let reply = if way == Way::Fresh {
            ReplyFile::Fresh(reply)
        } else {
            ReplyFile::Pooled(reply)
        };
        let source = job::open(&request, fds.next(), None);
        if way != Way::Stream && sandbox::lock_in().is_err() {
            return 1;
        }
        let outcome = match source {
            Ok(s) => job::finish(&request, s, Some(&reply)),
            Err(f) => Outcome::failed(f.status, f.reason),
        };
        channel::send(channel, &channel::encode_outcome(&outcome), &[]).unwrap();
        if way != Way::Stream {
            return 0;
        }
    }
}

fn pair() -> (OwnedFd, OwnedFd) {
    rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .unwrap()
}

fn spawn(way: Way, filter: &sandbox::WorkerFilter) -> (OwnedFd, rustix::process::Pid) {
    let (ours, theirs) = pair();
    match fork(&[std::os::fd::AsRawFd::as_raw_fd(&ours)]).unwrap() {
        Forked::Child => std::process::exit(worker(&theirs, way, filter)),
        Forked::Parent(pid) => (ours, pid),
    }
}

/// A reply file of `len` bytes, every page faulted in.
fn pooled(len: usize) -> OwnedFd {
    let fd = job::memory_file("pool").unwrap();
    rustix::fs::ftruncate(&fd, len as u64).unwrap();
    rustix::io::pwrite(&fd, &vec![0u8; len], 0).unwrap();
    rustix::fs::fcntl_add_seals(
        &fd,
        rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::GROW,
    )
    .unwrap();
    fd
}

/// Times one way over `frames`, each request and its inline bytes from
/// `make`; the median and the mean milliseconds a frame.
fn time(
    way: Way,
    w: u32,
    h: u32,
    frames: u32,
    make: &dyn Fn(u32) -> (Request, Option<&'static OwnedFd>),
) -> (f64, f64) {
    let len = (w * h * 4) as usize;
    let pool: Vec<OwnedFd> = if way == Way::Fresh {
        Vec::new()
    } else {
        (0..REPLY_POOL_FILES).map(|_| pooled(len)).collect()
    };
    let filter = sandbox::WorkerFilter::build().unwrap();
    let mut stream = (way == Way::Stream).then(|| spawn(way, &filter));
    let mut times = Vec::new();
    for n in 0..frames + 1 {
        let (request, inline) = make(n);
        let packet = wire::encode_request(u64::from(n) + 1, &request).unwrap();
        let start = Instant::now();
        let fresh;
        let reply: BorrowedFd<'_> = if way == Way::Fresh {
            fresh = job::memory_file("reply").unwrap();
            fresh.as_fd()
        } else {
            pool[n as usize % REPLY_POOL_FILES].as_fd()
        };
        let (channel, pid) = match &stream {
            Some((c, p)) => (c.try_clone().unwrap(), Some(*p)),
            None => {
                let (c, p) = spawn(way, &filter);
                (c, Some(p))
            }
        };
        let mut fds = vec![reply];
        if let Some(fd) = inline {
            fds.push(fd.as_fd());
        }
        channel::send(&channel, &packet, &fds).unwrap();
        let got = channel::receive(&channel, channel::OUTCOME_MAX).unwrap();
        let outcome = channel::decode_outcome(&got.bytes).unwrap();
        // the helper replies once the outcome is in and reaps the worker
        // after
        let took = start.elapsed();
        if stream.is_none() {
            rustix::process::waitpid(pid, rustix::process::WaitOptions::empty()).unwrap();
        }
        assert_eq!(
            outcome.status,
            Status::Ok,
            "{way:?} frame {n}: {}",
            outcome.reason
        );
        // the first frame warms the caches and the pool
        if n > 0 {
            times.push(took);
        }
    }
    if let Some((channel, pid)) = stream.take() {
        drop(channel);
        rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty()).unwrap();
    }
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let median = ms(times[times.len() / 2]);
    let mean = ms(times.iter().sum::<Duration>()) / times.len() as f64;
    (median, mean)
}

fn inline_file(bytes: &[u8]) -> OwnedFd {
    let fd = job::memory_file("inline").unwrap();
    rustix::io::pwrite(&fd, bytes, 0).unwrap();
    fd
}

fn main() {
    assert!(
        one_thread(),
        "the measurement forks, so it runs on one thread"
    );
    let args: Vec<String> = std::env::args().collect();
    let frames: u32 = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(20);
    println!("frames timed per case: {frames}, after one to warm up; median and mean ms a frame");
    println!("| Size | Frames | Way | Median ms | Mean ms |");
    println!("|---|---|---|---|---|");
    for (w, h) in [(1366u32, 768u32), (7680, 2160)] {
        let frames = if w > 4000 {
            frames.div_ceil(2).max(5)
        } else {
            frames
        };
        // four frames, each held in shared memory and in memory files of
        // its zlib and PNG forms, so the helper's side forks from a
        // process as small as the real helper rather than one that holds
        // every frame
        let names: Vec<String> = (0..4)
            .map(|n| format!("vtium-image-streams-{}-{n}", std::process::id()))
            .collect();
        let mut zlibs: Vec<&'static OwnedFd> = Vec::new();
        let mut pngs: Vec<&'static OwnedFd> = Vec::new();
        for (n, name) in names.iter().enumerate() {
            let picture = frame(w, h, n as u32);
            std::fs::write(std::path::Path::new("/dev/shm").join(name), &picture).unwrap();
            let z = miniz_oxide::deflate::compress_to_vec_zlib(&picture, 1);
            zlibs.push(Box::leak(Box::new(inline_file(&z))));
            pngs.push(Box::leak(Box::new(inline_file(&png(w, h, &picture)))));
        }
        let shm = |n: u32| {
            let name = names[n as usize % 4].clone().into_bytes();
            (raw(Medium::SharedMemory(name), w, h, false), None)
        };
        for way in [Way::Fresh, Way::Pool] {
            let (median, mean) = time(way, w, h, frames, &shm);
            println!("| {w}x{h} | t=s,f=32 | {way:?} | {median:.2} | {mean:.2} |");
        }
        for name in &names {
            let _ = std::fs::remove_file(std::path::Path::new("/dev/shm").join(name));
        }
        let zlib = |n: u32| {
            (
                raw(Medium::Direct(Vec::new()), w, h, true),
                Some(zlibs[n as usize % 4]),
            )
        };
        for way in [Way::Fresh, Way::Pool, Way::Stream] {
            let (median, mean) = time(way, w, h, frames, &zlib);
            println!("| {w}x{h} | f=32,o=z | {way:?} | {median:.2} | {mean:.2} |");
        }
        let png_frame = |n: u32| {
            (
                Request {
                    png: true,
                    ..base(Medium::Direct(Vec::new()), w, h)
                },
                Some(pngs[n as usize % 4]),
            )
        };
        for way in [Way::Fresh, Way::Pool, Way::Stream] {
            let (median, mean) = time(way, w, h, frames, &png_frame);
            println!("| {w}x{h} | f=100 | {way:?} | {median:.2} | {mean:.2} |");
        }
    }
}
