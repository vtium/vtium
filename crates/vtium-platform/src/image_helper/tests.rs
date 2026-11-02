//! The host's own rules, in process; the host against stand-in helpers and
//! the real one is in `tests/image_helper.rs` and the helper crate's tests.

#![expect(clippy::unwrap_used, reason = "a test fails by panicking")]

use rustix::fs::{FileType, MemfdFlags, SealFlags};
use vtium_image::decode::wire::{ReplyHeader, Status};
use vtium_image::decode::{Medium, Reply, Request};

use super::start::refusal;
use super::{inline_file, memory_cap, pixels, without_helper};

fn request(medium: Medium) -> Request {
    Request {
        medium,
        offset: 0,
        max: 0,
        min: 0,
        inflate: false,
        png: true,
        exact: false,
        discard: false,
        any_image: false,
        pixels: None,
        largest: (100, 100),
        largest_bytes: 1 << 20,
    }
}

#[test]
fn a_program_root_or_vtium_owns_and_alone_writes_is_taken() {
    let me = 1000;
    assert_eq!(refusal(FileType::RegularFile, 0, 0o100_755, me), None);
    assert_eq!(refusal(FileType::RegularFile, me, 0o100_700, me), None);
    for mode in [0o104_755, 0o102_755] {
        assert!(
            refusal(FileType::RegularFile, 0, mode, 0)
                .unwrap()
                .contains("setuid or setgid")
        );
    }
    assert!(refusal(FileType::Directory, 0, 0o40_755, me).is_some());
    assert!(
        refusal(FileType::RegularFile, 1001, 0o100_755, me)
            .unwrap()
            .contains("uid 1001")
    );
    for mode in [0o100_775, 0o100_757] {
        assert!(
            refusal(FileType::RegularFile, me, mode, me)
                .unwrap()
                .contains("may be written")
        );
    }
}

#[test]
fn the_memory_cap_is_three_budgets_and_the_base() {
    assert_eq!(memory_cap(0), 64 << 20);
    assert_eq!(memory_cap(10 << 20), (30 << 20) + (64 << 20));
    assert_eq!(memory_cap(u64::MAX), u64::MAX);
}

#[test]
fn without_a_helper_files_are_unreadable_and_inline_bytes_corrupt() {
    assert_eq!(
        without_helper(&request(Medium::File(b"/a".to_vec()))),
        Reply::Unreadable
    );
    assert_eq!(
        without_helper(&request(Medium::SharedMemory(b"a".to_vec()))),
        Reply::Unreadable
    );
    assert_eq!(
        without_helper(&request(Medium::Direct(vec![1]))),
        Reply::Corrupt
    );
}

#[test]
fn inline_bytes_travel_in_a_file_sealed_against_every_change() {
    let fd = inline_file(b"abc").unwrap();
    let seals = rustix::fs::fcntl_get_seals(&fd).unwrap();
    assert!(
        seals.contains(SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL)
    );
    let mut out = [0u8; 3];
    rustix::io::pread(&fd, &mut out, 0).unwrap();
    assert_eq!(&out, b"abc");
    assert!(rustix::io::pwrite(&fd, b"x", 0).is_err());
}

/// A stand-in reply's memory file of `len` bytes with `seals`.
fn reply_file(len: u64, seals: SealFlags) -> std::os::fd::OwnedFd {
    let fd =
        rustix::fs::memfd_create("reply", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING).unwrap();
    rustix::fs::ftruncate(&fd, len).unwrap();
    if !seals.is_empty() {
        rustix::fs::fcntl_add_seals(&fd, seals).unwrap();
    }
    fd
}

fn header(pooled: bool) -> ReplyHeader {
    ReplyHeader {
        status: Status::Ok,
        id: 1,
        width: 1,
        height: 1,
        ignored: 0,
        pooled,
        reason: Vec::new(),
    }
}

#[test]
fn a_replys_seals_are_checked_before_its_size() {
    let r = request(Medium::Direct(Vec::new()));
    let sealed = SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE;
    // unsealed and of the wrong size: the seals fail it first, since until
    // the shrink seal holds the size could change under a mapping
    let both = reply_file(5, SealFlags::empty());
    assert_eq!(
        pixels(&both, &header(false), &r),
        Err("a reply's memory file is not sealed")
    );
    let pooled_both = reply_file(5, SealFlags::SHRINK);
    assert_eq!(
        pixels(&pooled_both, &header(true), &r),
        Err("a reply's memory file is not sealed")
    );
    // sealed, the size is read next
    let wrong = reply_file(5, sealed);
    assert_eq!(
        pixels(&wrong, &header(false), &r),
        Err("a reply's memory file is not exactly the size asked for")
    );
    assert_eq!(
        pixels(&reply_file(4, sealed), &header(false), &r),
        Ok(vec![0; 4])
    );
    let pooled = reply_file(4, SealFlags::SHRINK | SealFlags::GROW);
    assert_eq!(pixels(&pooled, &header(true), &r), Ok(vec![0; 4]));
}
