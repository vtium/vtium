//! Any input, as a packet the helper's request reader takes, and as a
//! reply the console's reader takes: no panic, and a request it accepts
//! encodes back to the same packet.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image::decode::wire;

fuzz_target!(|data: &[u8]| {
    if let Ok((id, request)) = wire::decode_request(data) {
        assert_eq!(wire::encode_request(id, &request).as_deref(), Ok(data));
    }
    if let Ok(reply) = wire::decode_reply(data) {
        assert_eq!(wire::encode_reply(&reply), data);
    }
    let _ = wire::decode_hello(data);
});
