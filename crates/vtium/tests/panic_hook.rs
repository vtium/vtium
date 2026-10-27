//! The panic hook, in a process of its own: it is global.

use std::sync::{Arc, Mutex};

/// The VT is put back before the panic is printed.
#[test]
fn the_vt_is_restored_before_the_panic_prints() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let printed = Arc::clone(&order);
    // The hook that prints, as the default hook does.
    std::panic::set_hook(Box::new(move |_| {
        printed.lock().map(|mut o| o.push("printed")).ok();
    }));
    let restored = Arc::clone(&order);
    vtium::hook::restore_first(move || {
        restored.lock().map(|mut o| o.push("restored")).ok();
    });
    let _ = std::panic::catch_unwind(|| panic!("a test's panic"));
    let _ = std::panic::take_hook();
    assert_eq!(*order.lock().expect("the order"), ["restored", "printed"]);
}
