//! The memory a head's color table costs: the head's displays share one
//! table, and one copy of its nodes widened to 32-bit floats for the CPU
//! path's lookup, however many displays the head has. The GPU path reads
//! the same table, so a build with `gpu` holds no copy of its own.
//!
//! The test counts the bytes the process holds through a global allocator,
//! so this file holds one test, and no other test runs beside it to move
//! the count.
//!
//! References:
//! - ADR-1304 D2: the table is built once for each head, and the CPU path keeps its values as 32-bit floats, 421 KiB for each head.
#![cfg(feature = "filters")]
#![expect(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::unwrap_used,
    reason = "a test's sizes are small, and a test fails by panicking"
)]

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

use common::unifont;
use vtium_render::filter::{Fixed, Given, Map, Table, color};
use vtium_render::{Config, Renderer};

/// The global allocator, counting the bytes the process holds.
struct Counting;

static HELD: AtomicIsize = AtomicIsize::new(0);

// SAFETY: each call passes its arguments to the system allocator unchanged
// and only counts the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HELD.fetch_add(layout.size() as isize, Ordering::SeqCst);
        // SAFETY: the caller's layout, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HELD.fetch_sub(layout.size() as isize, Ordering::SeqCst);
        // SAFETY: a pointer this allocator gave with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        HELD.fetch_add(size as isize - layout.size() as isize, Ordering::SeqCst);
        // SAFETY: a pointer this allocator gave with this layout.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn held() -> isize {
    HELD.load(Ordering::SeqCst)
}

/// The nodes of a table 33 a side.
const NODES: isize = 33 * 33 * 33;

// A head of two displays holds its table, three half floats a node, and
// one copy of the nodes as 32-bit floats, which the first display's lookup
// makes: 646,906 bytes. The second display adds a few bytes, not another
// copy, and the GPU path's table is the head's.
#[test]
fn a_head_s_displays_share_one_color_table() {
    let tint = Map::new(
        color("tint").unwrap(),
        &[Given::Color([255, 176, 0]), Given::Number(Fixed::of(1, 0))],
    )
    .unwrap();
    let mut displays = [
        Renderer::new(unifont(), 640, 480, Config::default()),
        Renderer::new(unifont(), 640, 480, Config::default()),
    ];
    let before = held();
    let head = Arc::new(Table::build(&[tint]).unwrap());
    let table = held() - before;
    assert!(
        (NODES * 6..NODES * 6 + 256).contains(&table),
        "the table: {table} bytes"
    );
    let mut added = Vec::new();
    for display in &mut displays {
        let from = held();
        display.set_color_table(Some(&head)).unwrap();
        added.push(held() - from);
    }
    assert!(
        (NODES * 12..NODES * 12 + 256).contains(&added[0]),
        "the first display: {} bytes",
        added[0]
    );
    assert!(added[1] < 256, "the second display: {} bytes", added[1]);
    // the table goes when its last holder lets go
    drop(head);
    for display in &mut displays {
        display.set_color_table(None).unwrap();
    }
    assert!(held() - before < 256, "left: {} bytes", held() - before);
}
