//! The host's own rules, in process; the host against stand-in helpers and
//! the real one is in `tests/image_helper.rs` and the helper crate's tests.

#![expect(clippy::unwrap_used, reason = "a test fails by panicking")]

use rustix::fs::{FileType, SealFlags};
use vtium_image::decode::{Medium, Reply, Request};

use super::start::refusal;
use super::{inline_file, memory_cap, without_helper};

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
