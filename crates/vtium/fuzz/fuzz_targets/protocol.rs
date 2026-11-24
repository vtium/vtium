//! Any input, as a request the broker's reader takes from the drawer, and
//! as an order the drawer's reader takes from the broker: no panic, and a
//! message either reader accepts encodes back to the same bytes, so the
//! reader checked every byte of it.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/broker/protocol.rs"]
mod protocol;

use protocol::{Order, Request};

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = Request::decode(data) {
        assert_eq!(request.encode().as_slice(), data);
    }
    if let Ok(order) = Order::decode(data) {
        assert_eq!(order.encode().as_slice(), data);
    }
});
