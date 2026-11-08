//! The gate of ADR-1101 D9: the real helper program, built as installed,
//! started by the real host under its filter, sent every kind of request,
//! each reply checked. A call the filter wrongly refuses fails a request,
//! and this test with it, so a stale list cannot pass. The same requests
//! and a stream run again with landlock hidden, as on a kernel without
//! it, where each worker adds its third filter.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::event::{PollFd, PollFlags, Timespec};
use vtium_image::decode::{
    Alpha, Description, FrameLimits, Layout, Medium, Payload, Reply, Request,
};
use vtium_platform::image_helper::{ImageHost, Program, Session};

fn program() -> Program {
    Program::open(Path::new(env!("CARGO_BIN_EXE_vtium-image"))).unwrap()
}

fn session() -> Session {
    Session {
        child: None,
        pty_owner: Ok(rustix::process::geteuid().as_raw()),
    }
}

/// The files and directories a test makes outside its own memory, in
/// `/dev/shm`, the temporary directory and the build's, removed when the
/// test ends, whether it passes or panics.
#[derive(Default)]
struct Leftovers(Vec<PathBuf>);

impl Leftovers {
    /// `path`, to be removed when the test ends.
    fn add(&mut self, path: PathBuf) -> PathBuf {
        self.0.push(path.clone());
        path
    }

    /// A new directory of this test's own under the build's temporary
    /// directory.
    fn dir(&mut self, name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "vtium-image-helper-gate-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        self.add(dir)
    }
}

impl Drop for Leftovers {
    fn drop(&mut self) {
        for path in &self.0 {
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// A request the kitty handler could make, its bounds wide.
fn base(medium: Medium) -> Request {
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
        frames: None,
        pixels: None,
        largest: (1000, 1000),
        largest_bytes: 16 << 20,
    }
}

fn raw(medium: Medium, layout: Layout, width: u32, height: u32, compressed: bool) -> Request {
    Request {
        min: u64::from(width * height) * layout.bytes() as u64,
        inflate: compressed,
        pixels: Some(Description {
            layout,
            alpha: Alpha::Straight,
            width,
            height,
            compressed,
        }),
        ..base(medium)
    }
}

/// The reply to `request`, each wait for the socket bounded at ten
/// seconds so the test fails instead of hanging; the hello and the reply
/// take at most two packets.
fn ask(host: &mut ImageHost, request: &Request) -> Reply {
    if let Err(answer) = host.send(request, &session()) {
        return answer;
    }
    for _ in 0..3 {
        let socket = host.wake().expect("a request is out");
        let mut fds = [PollFd::new(&socket, PollFlags::IN)];
        let ten = Timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        let ready = rustix::event::poll(&mut fds, Some(&ten)).unwrap();
        assert!(ready > 0, "no reply in ten seconds");
        if let Some(reply) = host.readable() {
            return reply.checked(request);
        }
    }
    panic!("no reply after the hello");
}

/// A reply's picture and its bytes, read out of the helper's reply file.
fn picture(reply: Reply) -> (u32, u32, Vec<u8>) {
    match reply {
        Reply::Ok(Payload::File {
            width,
            height,
            file,
        }) => {
            let mut data = Vec::new();
            assert!(
                file.read(&mut |run| data.extend_from_slice(run)),
                "a short file"
            );
            (width, height, data)
        }
        other => panic!("not a picture: {other:?}"),
    }
}

fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(rgba).unwrap();
    w.finish().unwrap();
    out
}

const STRAIGHT: [u8; 8] = [0xff, 0x80, 0, 0xff, 0xff, 0xff, 0xff, 0x80];
const PREMULTIPLIED: [u8; 8] = [0xff, 0x80, 0, 0xff, 0x80, 0x80, 0x80, 0x80];

// covers: image.helper
#[test]
fn the_real_helper_answers_every_kind_of_request_under_its_filter() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    every_kind_of_request(&mut ImageHost::new(Some(program()), None));
}

/// Every kind of request sent to the helper `host` starts, each reply
/// checked, and none of them a crash.
fn every_kind_of_request(host: &mut ImageHost) {
    let mut leftovers = Leftovers::default();
    let dir = leftovers.dir("kinds");

    // inline raw pixels with zlib, converted and premultiplied
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&STRAIGHT, 6);
    let r = raw(Medium::Direct(z), Layout::Rgba, 2, 1, true);
    assert_eq!(picture(ask(host, &r)), (2, 1, PREMULTIPLIED.to_vec()));
    let hello = host.hello().unwrap();
    assert_eq!(
        hello.seccomp,
        cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
    );

    // inline PNG, and PNG inside zlib held to an exact length
    let file = png(2, 1, &STRAIGHT);
    let r = Request {
        png: true,
        ..base(Medium::Direct(file.clone()))
    };
    assert_eq!(picture(ask(host, &r)), (2, 1, PREMULTIPLIED.to_vec()));
    let r = Request {
        png: true,
        inflate: true,
        exact: true,
        min: file.len() as u64,
        ..base(Medium::Direct(miniz_oxide::deflate::compress_to_vec_zlib(
            &file, 6,
        )))
    };
    assert_eq!(picture(ask(host, &r)), (2, 1, PREMULTIPLIED.to_vec()));
    // iTerm2's any image
    let r = Request {
        any_image: true,
        frames: None,
        ..base(Medium::Direct(file.clone()))
    };
    assert_eq!(picture(ask(host, &r)), (2, 1, PREMULTIPLIED.to_vec()));
    // a PNG that does not decode
    let r = Request {
        png: true,
        ..base(Medium::Direct(b"\x89PNG\r\n\x1a\njunk".to_vec()))
    };
    assert_eq!(ask(host, &r), Reply::Corrupt);

    // a file of raw RGB pixels at an offset
    let path = dir.join("rgb");
    std::fs::write(&path, [9, 9, 1, 2, 3, 4, 5, 6, 7]).unwrap();
    let mut r = raw(
        Medium::File(path.as_os_str().as_bytes().to_vec()),
        Layout::Rgb,
        2,
        1,
        false,
    );
    r.offset = 2;
    assert_eq!(
        picture(ask(host, &r)),
        (2, 1, vec![1, 2, 3, 0xff, 4, 5, 6, 0xff])
    );
    // a PNG file
    let pngfile = dir.join("p.png");
    std::fs::write(&pngfile, &file).unwrap();
    let r = Request {
        png: true,
        ..base(Medium::File(pngfile.as_os_str().as_bytes().to_vec()))
    };
    assert_eq!(picture(ask(host, &r)), (2, 1, PREMULTIPLIED.to_vec()));

    // a temporary file is read and removed
    let temp = leftovers.add(
        std::env::temp_dir().join(format!("tty-graphics-protocol-gate-{}", std::process::id())),
    );
    std::fs::write(&temp, &file).unwrap();
    let r = Request {
        png: true,
        ..base(Medium::TempFile(temp.as_os_str().as_bytes().to_vec()))
    };
    assert_eq!(picture(ask(host, &r)).0, 2);
    assert!(!temp.exists(), "the temporary file is removed once read");

    // shared memory: a valid picture removes the object, a short one stays
    let name = format!("vtium-image-gate-{}", std::process::id());
    let shm = leftovers.add(Path::new("/dev/shm").join(&name));
    std::fs::write(&shm, [1, 2, 3, 4, 5, 6]).unwrap();
    let short = raw(
        Medium::SharedMemory(name.clone().into_bytes()),
        Layout::Rgb,
        3,
        1,
        false,
    );
    assert_eq!(ask(host, &short), Reply::TooShort);
    assert!(
        shm.exists(),
        "an object a picture cannot be read from stays"
    );
    let slash = format!("/{name}").into_bytes();
    let r = raw(Medium::SharedMemory(slash), Layout::Rgb, 2, 1, false);
    assert_eq!(picture(ask(host, &r)).2, [1, 2, 3, 0xff, 4, 5, 6, 0xff]);
    assert!(!shm.exists(), "a valid picture's object is removed");

    // a discard removes a temporary file and reads nothing; of shared
    // memory it removes nothing
    std::fs::write(&temp, b"x").unwrap();
    let discard = Request {
        discard: true,
        ..base(Medium::TempFile(temp.as_os_str().as_bytes().to_vec()))
    };
    assert_eq!(ask(host, &discard), Reply::Ok(Payload::Discarded));
    assert!(!temp.exists());
    std::fs::write(&shm, b"x").unwrap();
    let discard = Request {
        discard: true,
        ..base(Medium::SharedMemory(name.clone().into_bytes()))
    };
    assert_eq!(ask(host, &discard), Reply::Ok(Payload::Discarded));
    assert!(shm.exists());

    // refused paths: a symlink into /proc, a FIFO, a name escaping /dev/shm
    let link = dir.join("status");
    std::os::unix::fs::symlink("/proc/self/status", &link).unwrap();
    let fifo = dir.join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    for refused in [&link, &fifo] {
        let r = raw(
            Medium::File(refused.as_os_str().as_bytes().to_vec()),
            Layout::Rgb,
            1,
            1,
            false,
        );
        assert_eq!(ask(host, &r), Reply::Unreadable, "{}", refused.display());
    }
    let escape = raw(
        Medium::SharedMemory(b"../shm/x".to_vec()),
        Layout::Rgb,
        1,
        1,
        false,
    );
    assert_eq!(ask(host, &escape), Reply::Unreadable);

    // every request above went to one helper, which never crashed: each
    // worker found its sandbox as it should be, vtium's socket closed, one
    // more seccomp filter than the helper and, where the kernel offers
    // landlock, a domain it cannot open a directory through
    assert_eq!(host.crashes(), 0);
    assert!(host.pid().is_some());
}

/// The pooled reply files the running helper holds open: the distinct
/// memory files of that name, since a reply's copy of one may still be
/// open beside it.
fn pools(host: &ImageHost) -> usize {
    use std::os::unix::fs::MetadataExt;
    let pid = host.pid().unwrap();
    let inodes: std::collections::BTreeSet<u64> = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .filter_map(|e| {
            let path = e.ok()?.path();
            let link = std::fs::read_link(&path).ok()?;
            link.to_str()?.contains("vtium-image-pool").then_some(())?;
            Some(std::fs::metadata(&path).ok()?.ino())
        })
        .collect();
    inodes.len()
}

/// The running helper's children: its workers not yet reaped.
fn workers(host: &ImageHost) -> Vec<u32> {
    let helper = host.pid().unwrap();
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| {
            let pid: u32 = e.ok()?.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            // the parent's pid is the second field after the name
            let ppid: u32 = stat
                .rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()?;
            (ppid == helper).then_some(pid)
        })
        .collect()
}

// covers: image.helper
#[test]
fn a_stream_of_inline_frames_has_one_worker_and_a_new_size_ends_it() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    a_stream(&mut ImageHost::new(Some(program()), None));
}

/// Frames from shared memory, each served by a one-off worker, then
/// streams of inline frames served by one stream worker, sent to the
/// helper `host` starts, and none of them a crash.
fn a_stream(host: &mut ImageHost) {
    let mut leftovers = Leftovers::default();
    // frames from shared memory never make a stream: each has a fresh
    // worker and a fresh reply file
    let name = format!("vtium-image-stream-{}", std::process::id());
    let shm = leftovers.add(Path::new("/dev/shm").join(&name));
    for n in 0..5u8 {
        std::fs::write(&shm, [n, 2, 3, 0xff, 4, 5, 6, 0xff]).unwrap();
        let r = raw(
            Medium::SharedMemory(name.clone().into_bytes()),
            Layout::Rgba,
            2,
            1,
            false,
        );
        assert_eq!(
            picture(ask(host, &r)).2,
            [n, 2, 3, 0xff, 4, 5, 6, 0xff],
            "frame {n}"
        );
        assert!(!shm.exists(), "frame {n}'s object is removed");
        assert_eq!(pools(host), 0, "after frame {n}");
    }
    // inline PNG frames: from the fourth, one stream worker serves them
    let frame = Request {
        png: true,
        ..base(Medium::Direct(png(2, 1, &STRAIGHT)))
    };
    let mut stream_worker = None;
    for n in 1..=6 {
        assert_eq!(
            picture(ask(host, &frame)),
            (2, 1, PREMULTIPLIED.to_vec()),
            "frame {n}"
        );
        // the stream starts once the third frame's reply is out, and the
        // fourth request is read only once it has
        if n < 3 {
            assert_eq!(pools(host), 0, "after frame {n}");
        }
        if n > 3 {
            assert_eq!(pools(host), 2, "after frame {n}");
            let now = workers(host);
            assert_eq!(now.len(), 1, "the stream worker alone, after frame {n}");
            assert_eq!(
                *stream_worker.get_or_insert(now[0]),
                now[0],
                "one worker for the whole stream"
            );
        }
    }
    let stream_worker = stream_worker.unwrap();
    // a frame that decodes to more bytes than the stream's does not fit
    // its pooled file, so the stream ends and the frame runs again as a
    // one-off with a fresh file
    let four = [STRAIGHT, STRAIGHT].concat();
    let r = Request {
        png: true,
        ..base(Medium::Direct(png(2, 2, &four)))
    };
    let both = [PREMULTIPLIED, PREMULTIPLIED].concat();
    assert_eq!(
        picture(ask(host, &r)),
        (2, 2, both),
        "run again as a one-off"
    );
    assert_eq!(pools(host), 0, "the stream ended");
    assert!(
        !workers(host).contains(&stream_worker),
        "the stream worker is reaped"
    );
    // inline zlib frames make a stream too, and a request of another kind
    // ends it before it is served
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&STRAIGHT, 6);
    let zlib = raw(Medium::Direct(z), Layout::Rgba, 2, 1, true);
    for n in 1..=4 {
        assert_eq!(
            picture(ask(host, &zlib)),
            (2, 1, PREMULTIPLIED.to_vec()),
            "zlib frame {n}"
        );
    }
    assert_eq!(pools(host), 2);
    assert_eq!(picture(ask(host, &frame)).0, 2);
    assert_eq!(pools(host), 0, "a PNG frame ends a zlib stream");
    assert_eq!(host.crashes(), 0);
}

#[test]
fn a_helper_started_for_a_larger_budget_replaces_the_smaller_one() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let mut host = ImageHost::new(Some(program()), None);
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&STRAIGHT, 6);
    let small = raw(Medium::Direct(z.clone()), Layout::Rgba, 2, 1, true);
    picture(ask(&mut host, &small));
    let first = host.pid().unwrap();
    let larger = Request {
        largest_bytes: 32 << 20,
        ..small
    };
    picture(ask(&mut host, &larger));
    assert_ne!(
        host.pid().unwrap(),
        first,
        "its memory cap was set from fewer bytes"
    );
    until_gone(first);
}

/// Waits, at most ten seconds, until the helper `pid` is killed and
/// reaped. The host reaps a killed helper on a thread of its own when it
/// has not died by the time the host looks, so the test waits for that
/// thread, bounded only so a helper that is never reaped fails the test.
fn until_gone(pid: u32) {
    for _ in 0..1000 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        let _ = rustix::event::poll(
            &mut [],
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        );
    }
    panic!("the old helper {pid} is not killed and reaped in ten seconds");
}

#[test]
fn a_picture_past_the_budget_is_refused_before_it_is_decoded() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let mut host = ImageHost::new(Some(program()), None);
    let big = png(1000, 1000, &vec![0; 4_000_000]);
    let r = Request {
        png: true,
        largest: (999, 1000),
        ..base(Medium::Direct(big.clone()))
    };
    assert_eq!(ask(&mut host, &r), Reply::TooLarge);
    let r = Request {
        png: true,
        largest_bytes: 3_999_999.max(big.len() as u64),
        ..base(Medium::Direct(big))
    };
    assert_eq!(ask(&mut host, &r), Reply::OverBudget);
}

/// The variable that tells this test binary, run again, to hide landlock
/// from the helpers it starts.
const NO_LANDLOCK: &str = "VTIUM_TEST_IMAGE_HELPER_NO_LANDLOCK";

/// The helper on a kernel without landlock, before 5.13 or with landlock
/// left out of `lsm=`, which no gate machine is: this test binary runs
/// again, and there a seccomp filter on the test's thread answers
/// `landlock_create_ruleset` with ENOSYS, as such a kernel does. Every
/// helper the test then starts inherits the filter, so it offers its
/// workers no landlock, and each worker adds its third filter, which the
/// helper must count. Every kind of request and a stream must still be
/// served with no crash.
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
// covers: image.helper
#[test]
fn the_real_helper_serves_every_request_on_a_kernel_without_landlock() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    const NAME: &str = "the_real_helper_serves_every_request_on_a_kernel_without_landlock";
    if std::env::var_os(NO_LANDLOCK).is_some() {
        hide_landlock();
        let mut host = ImageHost::new(Some(program()), None);
        every_kind_of_request(&mut host);
        assert_eq!(host.hello().unwrap().landlock, 0, "landlock is hidden");
        a_stream(&mut ImageHost::new(Some(program()), None));
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(NO_LANDLOCK, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains(&format!("test {NAME} ... ok")),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Makes `landlock_create_ruleset` answer ENOSYS on the calling thread and
/// every process it forks from now on, as on a kernel without landlock.
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
fn hide_landlock() {
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};
    #[cfg(target_arch = "x86_64")]
    let arch = TargetArch::x86_64;
    #[cfg(target_arch = "aarch64")]
    let arch = TargetArch::aarch64;
    #[cfg(target_arch = "riscv64")]
    let arch = TargetArch::riscv64;
    let rules = [(libc::SYS_landlock_create_ruleset, Vec::new())].into();
    let enosys = SeccompAction::Errno(libc::ENOSYS as u32);
    let filter = SeccompFilter::new(rules, SeccompAction::Allow, enosys, arch).unwrap();
    seccompiler::apply_filter(&BpfProgram::try_from(filter).unwrap()).unwrap();
}

/// A GIF of `frames` frames, 16 by 16 pixels each in one palette color, its
/// index from 1, delays 5, 6, 7 and on, and a loop count of 2.
fn gif_file(frames: u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let palette: Vec<u8> = (0..=frames).flat_map(|i| [i * 10, 0, 0]).collect();
        let mut enc = gif::Encoder::new(&mut out, 16, 16, &palette).unwrap();
        enc.set_repeat(gif::Repeat::Finite(2)).unwrap();
        for i in 1..=frames {
            let frame = gif::Frame {
                width: 16,
                height: 16,
                delay: 4 + u16::from(i),
                buffer: std::borrow::Cow::Owned(vec![i; 256]),
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).unwrap();
        }
    }
    out
}

// covers: image.iterm2
#[test]
fn the_real_helper_returns_an_animated_gifs_frames_past_its_budget_of_one_picture() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let mut host = ImageHost::new(Some(program()), None);
    let file = gif_file(3);
    // one 16 by 16 frame is 1,024 bytes, the most a picture may take, so
    // the reply file of three frames passes it and needs the frames' room
    let r = Request {
        any_image: true,
        frames: Some(FrameLimits {
            max_frames: 1024,
            max_frame_bytes: 2048,
        }),
        largest_bytes: 1024,
        ..base(Medium::Direct(file.clone()))
    };
    let reply = ask(&mut host, &r);
    let Reply::Ok(payload @ Payload::Frames { .. }) = reply else {
        panic!("not frames: {reply:?}");
    };
    let Payload::Frames { delays, loops, .. } = &payload else {
        unreachable!();
    };
    assert_eq!((delays.as_slice(), *loops), (&[5u16, 6, 7][..], Some(2)));
    let frames = payload
        .copy_frames(usize::MAX)
        .expect("the frames read whole");
    let red = |i: u32| 0xff00_0000 | (i * 10) << 16;
    for (n, (pixels, opaque)) in frames.iter().enumerate() {
        assert_eq!(pixels, &vec![red(n as u32 + 1); 256], "frame {n}");
        assert!(opaque);
    }
    // one byte short of the frames past the first: its first frame, still
    let still = Request {
        frames: Some(FrameLimits {
            max_frames: 1024,
            max_frame_bytes: 2047,
        }),
        ..r.clone()
    };
    assert_eq!(picture(ask(&mut host, &still)).2.len(), 1024);
    let two = Request {
        frames: Some(FrameLimits {
            max_frames: 2,
            max_frame_bytes: 2048,
        }),
        ..r
    };
    assert_eq!(picture(ask(&mut host, &two)).2.len(), 1024);
    // without the frames flag, the first frame
    let first = Request {
        any_image: true,
        frames: None,
        ..base(Medium::Direct(file))
    };
    assert_eq!(picture(ask(&mut host, &first)).0, 16);
    assert_eq!(host.crashes(), 0);
}

#[test]
fn requests_that_alternate_a_larger_budget_and_more_frame_room_restart_the_helper_once() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let mut host = ImageHost::new(Some(program()), None);
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&STRAIGHT, 6);
    let budget = Request {
        largest_bytes: 32 << 20,
        ..raw(Medium::Direct(z), Layout::Rgba, 2, 1, true)
    };
    let frames = Request {
        any_image: true,
        frames: Some(FrameLimits {
            max_frames: 1024,
            max_frame_bytes: 64 << 20,
        }),
        largest_bytes: 1 << 20,
        ..base(Medium::Direct(gif_file(2)))
    };
    picture(ask(&mut host, &budget));
    let first = host.pid().unwrap();
    assert!(matches!(
        ask(&mut host, &frames),
        Reply::Ok(Payload::Frames { .. })
    ));
    let second = host.pid().unwrap();
    assert_ne!(first, second, "its file cap was set from fewer bytes");
    picture(ask(&mut host, &budget));
    assert!(matches!(
        ask(&mut host, &frames),
        Reply::Ok(Payload::Frames { .. })
    ));
    assert_eq!(
        host.pid().unwrap(),
        second,
        "the helper keeps the larger of each cap"
    );
}
