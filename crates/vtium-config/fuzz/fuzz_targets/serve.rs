//! Any bytes as the requests the console writes to the glyph helper's
//! stdin, read one after another by `read_request` as the helper reads
//! them (ADR-1301 D2): no panic, whatever the bytes, and each request it
//! takes encodes back to bytes it reads as the same request. That holds
//! for a glyph request, its style and its script among its fields
//! (ADR-1202 D11), and for a pointer ask, its file kind, shape, size,
//! heights and file among them (ADR-1309 D5).
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config::serve::{Request, read_request};

fuzz_target!(|bytes: &[u8]| {
    let mut input = bytes;
    while let Ok(Some(request)) = read_request(&mut input) {
        let encoded = match &request {
            Request::Serve(r) => r.encode(),
            Request::Pointer(a) => a.encode(),
        };
        assert_eq!(
            read_request(&mut &encoded[..]),
            Ok(Some(request)),
            "a request read back differs"
        );
    }
});
