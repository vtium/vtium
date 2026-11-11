//! Any bytes as the requests the console writes to the glyph helper's
//! stdin, read one after another by `ServeRequest::read` as the helper
//! reads them (ADR-1301 D2): no panic, whatever the bytes, and each request
//! it takes, its style and its script among its fields (ADR-1202 D11),
//! encodes back to bytes it reads as the same request.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config::serve::ServeRequest;

fuzz_target!(|bytes: &[u8]| {
    let mut input = bytes;
    while let Ok(Some(request)) = ServeRequest::read(&mut input) {
        let encoded = request.encode();
        assert_eq!(
            ServeRequest::read(&mut &encoded[..]),
            Ok(Some(request)),
            "a request read back differs"
        );
    }
});
