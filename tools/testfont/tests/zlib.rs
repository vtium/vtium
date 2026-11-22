//! The zlib stream that `ttf::zlib_run` writes inflates to the bytes it
//! names, with a checksum zlib accepts.

use vtium_testfont::ttf::zlib_run;

fn inflated(start: &[u8], len: usize) -> Vec<u8> {
    miniz_oxide::inflate::decompress_to_vec_zlib(&zlib_run(start, len)).expect("a valid stream")
}

#[test]
fn a_run_inflates_to_its_start_and_copies_of_its_last_byte() {
    for len in [1, 5, 6, 258, 263, 264, 1000, 70_000] {
        let start = [0, 255, 0, 0, 255];
        let mut want = start[..start.len().min(len)].to_vec();
        want.resize(len, 255);
        assert_eq!(inflated(&start, len), want, "{len} bytes");
    }
}

#[test]
fn a_long_run_is_small() {
    let stream = zlib_run(&[0], 600 << 20);
    assert!(stream.len() < 4 << 20, "{} bytes", stream.len());
}
