//! Any bytes as the requests the console writes to the glyph helper's
//! stdin, read one after another by `read_request` as the helper reads
//! them: no panic, whatever the bytes, and each request it takes encodes
//! back to bytes it reads as the same request. That holds for a glyph
//! request, its style and its script among its fields, for a pointer ask,
//! its file kind, shape, size, heights and file among them, for the
//! active fonts, their account and paths, and for a font file's
//! description, its path.
//!
//! References:
//! - ADR-1301 D2: the console's requests to the glyph helper and how the helper reads them.
//! - ADR-1202 D11: a glyph request carries its style and its script.
//! - ADR-1309 D5: the pointer ask and its fields.
//! - ADR-1205 D4: the request that names the active fonts, their account and paths.
//! - ADR-1205 D3: the request that asks the helper to describe a font file.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config::serve::{Request, read_request};

fuzz_target!(|bytes: &[u8]| {
    let mut input = bytes;
    while let Ok(Some(request)) = read_request(&mut input) {
        let encoded = match &request {
            Request::Serve(r) => r.encode(),
            Request::Pointer(a) => a.encode(),
            Request::Active(a) => a.encode(),
            Request::Describe(a) => a.encode(),
        };
        assert_eq!(
            read_request(&mut &encoded[..]),
            Ok(Some(request)),
            "a request read back differs"
        );
    }
});
