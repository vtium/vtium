//! The kitty handler's tests, through a stand-in for vtium-term's context.
//!
//! The stand-in holds a real store over a screen of fixed lines and the
//! scrollback above it, and does what vtium-term's context does with each
//! call: it runs a store edit over both buffers' rows and settles the store
//! after it, clamps the cursor to the screen, and keeps replies and
//! requests in order. A handler's index past the bottom margin fails the
//! test, so the cases where a handler scrolls are tier 1's, through the
//! real screen; a test scrolls the screen itself as vtium-term does, the
//! store's scroll before the lines move and its `scroll_done` after.

#![expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use vtium_parse::Chunk;

use super::Kitty;
use crate::IMAGE_OVERHEAD;
use crate::decode::{
    Alpha, Description, Layout, Medium, Payload, Reply, ReplyFile, ReplySource, Request,
};
use crate::handler::{Context, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Switches};
use crate::image::LineKey;
use crate::store::{Edit, Rows, Screens, Store};

struct Host {
    store: Store,
    screen: Vec<LineKey>,
    /// The scrollback's lines, newest first.
    above: Vec<LineKey>,
    /// The key the next new line gets.
    next_line: u64,
    cols: usize,
    cursor: (usize, usize),
    margins: Margins,
    helper: bool,
    replies: Vec<u8>,
    requests: Vec<Request>,
    limits: Limits,
    work: u64,
    /// The rows of each index the handler asked for, which the real screen
    /// would scroll by past the bottom margin.
    indexed: Vec<usize>,
}

impl Host {
    /// A 12 by 4 screen, its cell 8 by 16 pixels, a budget of 1 MiB.
    fn new() -> Self {
        let mut store = Store::new();
        store.set_limit(1 << 20);
        Self {
            store,
            screen: (0..4).map(LineKey::new).collect(),
            above: Vec::new(),
            next_line: 100,
            cols: 12,
            cursor: (0, 0),
            margins: Margins { top: 0, bottom: 3 },
            helper: false,
            replies: Vec::new(),
            requests: Vec::new(),
            limits: Limits::default(),
            work: 0,
            indexed: Vec::new(),
        }
    }

    /// The replies so far, as text, taken.
    fn take(&mut self) -> String {
        String::from_utf8(core::mem::take(&mut self.replies)).unwrap()
    }

    /// Each placement as (column, row, columns, rows).
    fn boxes(&self) -> Vec<(u32, i64, u32, u32)> {
        let view = Rows {
            screen: &self.screen,
            above: &self.above,
        };
        self.store
            .frame(view, self.cols)
            .iter()
            .map(|p| (p.col, p.row, p.cols, p.rows))
            .collect()
    }

    /// Rows `top` to `bottom` scroll up one, as vtium-term scrolls them: the
    /// whole screen sends its top line into the scrollback, a region drops
    /// it, and a new line enters at the bottom.
    fn scroll_up(&mut self, top: usize, bottom: usize) {
        let whole = top == 0 && bottom + 1 == self.screen.len();
        let rows = Rows {
            screen: &self.screen,
            above: &self.above,
        };
        self.store.scroll((top, bottom), 1, true, whole, rows);
        let gone = self.screen.remove(top);
        if whole {
            self.above.insert(0, gone);
        }
        self.screen.insert(bottom, LineKey::new(self.next_line));
        self.next_line += 1;
        let rows = Rows {
            screen: &self.screen,
            above: &self.above,
        };
        self.store.scroll_done(rows);
    }
}

impl Context for Host {
    fn store(&self) -> &Store {
        &self.store
    }

    fn edit_store(&mut self, f: &mut dyn FnMut(&mut Edit<'_>)) {
        let screens = Screens {
            main: Rows {
                screen: &self.screen,
                above: &self.above,
            },
            alt: Rows::default(),
        };
        f(&mut Edit::new(&mut self.store, screens, self.cols));
        self.store.settle_event(screens);
    }

    fn helper(&self) -> bool {
        self.helper
    }

    fn request(&mut self, request: Request) {
        self.requests.push(request);
    }

    fn cell_size(&self) -> (u32, u32) {
        self.store.cell()
    }

    fn grid(&self) -> (usize, usize) {
        (self.cols, self.screen.len())
    }

    fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    fn move_cursor(&mut self, col: usize, row: usize) {
        let col = col.min(self.cols - 1);
        let row = row.min(self.screen.len() - 1);
        self.cursor = (col, row);
    }

    fn margins(&self) -> Margins {
        self.margins
    }

    fn index(&mut self, n: usize) {
        // as the screen's move_down: inside the region the cursor stops at
        // the bottom margin and the region scrolls, which the stand-in
        // cannot do; below it the cursor stops at the screen's last row
        self.indexed.push(n);
        let (col, row) = self.cursor;
        let bottom = self.margins.bottom;
        let end = if row <= bottom {
            bottom + 1
        } else {
            self.screen.len()
        };
        let room = end - row - 1;
        assert!(
            n <= room || row > bottom,
            "the stand-in screen does not scroll"
        );
        self.cursor = (col, row + n.min(room));
    }

    fn scroll_up(&mut self, _: usize) {
        panic!("the kitty handler never scrolls the region itself");
    }

    fn sixel(&self) -> SixelState {
        SixelState {
            display: false,
            private_registers: true,
            cursor_right: false,
            registers: 1024,
        }
    }

    fn reply(&mut self, kind: ReplyKind, bytes: &[u8]) {
        assert_eq!(kind, ReplyKind::KittyGraphics);
        self.replies.extend_from_slice(bytes);
    }

    fn switches(&self) -> Switches {
        Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        }
    }

    fn limits(&self) -> Limits {
        self.limits
    }

    fn add_work(&mut self, work: u64) {
        self.work = self.work.saturating_add(work);
    }
}

/// Feeds one APC `G` string's data, the bytes after its `G`, in `pieces`,
/// and ends it with its terminator.
fn string(k: &mut Kitty, h: &mut Host, pieces: &[&[u8]]) {
    k.start(h, Intro::String);
    for p in pieces {
        k.feed(h, Chunk::Bytes(p));
    }
    k.finish(h);
}

fn send(k: &mut Kitty, h: &mut Host, data: &str) {
    string(k, h, &[data.as_bytes()]);
}

/// Base64 of `bytes`, for the tests' payloads.
fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let v = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(A[((v >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `n` opaque red RGBA pixels, as base64.
fn red(n: usize) -> String {
    b64(&[0xff, 0, 0, 0xff].repeat(n))
}

// covers: image.kitty
#[test]
fn a_transmission_stores_premultiplied_pixels_and_places_at_the_cursor() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    // one opaque red pixel and one half-transparent white one
    let data = b64(&[0xff, 0, 0, 0xff, 0xff, 0xff, 0xff, 0x80]);
    send(&mut k, &mut h, &format!("a=T,f=32,s=2,v=1,i=7;{data}"));
    assert_eq!(h.take(), "\x1b_Gi=7;OK\x1b\\");
    let frame = h.store.frame(
        Rows {
            screen: &h.screen,
            above: &[],
        },
        12,
    );
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].pixels.as_slice(), &[0xffff_0000, 0x8080_8080]);
    // rgb, one pixel, placed after the first
    send(
        &mut k,
        &mut h,
        &format!("a=T,f=24,s=1,v=1,i=8;{}", b64(&[1, 2, 3])),
    );
    assert_eq!(h.take(), "\x1b_Gi=8;OK\x1b\\");
    assert_eq!(h.boxes(), vec![(0, 0, 1, 1), (1, 0, 1, 1)]);
}

// covers: image.kitty
#[test]
fn a_payload_cut_anywhere_stores_the_same_image() {
    let data = format!(
        "a=T,f=24,s=5,v=3,i=1,c=2;{}",
        b64(&(0..45u8).collect::<Vec<_>>())
    );
    let whole = {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        send(&mut k, &mut h, &data);
        (h.take(), h.store.clone(), k)
    };
    for cut in 0..data.len() {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        let (a, b) = data.as_bytes().split_at(cut);
        string(&mut k, &mut h, &[a, b]);
        assert_eq!(h.take(), whole.0, "cut at {cut}");
        assert!(h.store == whole.1, "cut at {cut}");
        assert_eq!(k, whole.2);
    }
}

// covers: image.kitty
#[test]
fn replies_follow_q_and_need_an_id_or_a_number() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let px = red(1);
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=1,q=1;{px}"));
    send(&mut k, &mut h, "a=p,i=9,q=1");
    send(&mut k, &mut h, "a=p,i=9,q=2");
    send(&mut k, &mut h, &format!("a=t,s=1,v=1;{px}"));
    send(&mut k, &mut h, "a=p,i=1,p=4,C=1");
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,I=5;{px}"));
    assert_eq!(
        h.take(),
        "\x1b_Gi=9;ENOENT:image not found\x1b\\\
         \x1b_Gi=1,p=4;OK\x1b\\\
         \x1b_Gi=2,I=5;OK\x1b\\"
    );
}

#[test]
fn a_query_without_an_id_reads_nothing_and_is_never_answered() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=q,s=1,v=1,f=24,I=3;AAAA");
    send(&mut k, &mut h, "a=q,s=1,v=1,f=24,I=3,q=0;AAA*");
    assert_eq!(h.take(), "");
    send(&mut k, &mut h, "a=q,s=1,v=1,f=24,i=3;AAAA");
    send(&mut k, &mut h, "a=q,s=2,v=1,f=24,i=4;AAAA");
    assert_eq!(
        h.take(),
        "\x1b_Gi=3;OK\x1b\\\x1b_Gi=4;EINVAL:invalid argument\x1b\\"
    );
    assert!(h.store.is_empty(), "a query stores nothing");
}

#[test]
fn keys_that_cannot_be_read_and_keys_the_spec_lacks() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let px = red(1);
    // unknown keys, the feature query's e among them, are ignored
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=1,e=T,K=9,,;{px}"));
    // a format past 24, 32 and 100, such as 8 or 16, is refused
    send(&mut k, &mut h, &format!("a=t,f=8,s=1,v=1,i=2;{px}"));
    send(&mut k, &mut h, &format!("a=t,f=16,s=1,v=1,i=3;{px}"));
    // a value past 32 bits, a letter key of two letters, a key with no value
    send(&mut k, &mut h, &format!("a=t,s=4294967296,v=1,i=4;{px}"));
    send(&mut k, &mut h, &format!("a=tt,s=1,v=1,i=5;{px}"));
    send(&mut k, &mut h, &format!("a=t,s=,v=1,i=6;{px}"));
    // i with I
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=7,I=7;{px}"));
    let bad = "EINVAL:invalid argument";
    assert_eq!(
        h.take(),
        format!(
            "\x1b_Gi=1;OK\x1b\\\x1b_Gi=2;{bad}\x1b\\\x1b_Gi=3;{bad}\x1b\\\x1b_Gi=4;{bad}\x1b\\\
             \x1b_Gi=5;{bad}\x1b\\\x1b_Gi=6;{bad}\x1b\\\x1b_Gi=7,I=7;{bad}\x1b\\"
        )
    );
    // a signed z, and one past 32 bits
    send(&mut k, &mut h, "a=p,i=1,z=-2147483648,C=1");
    send(&mut k, &mut h, "a=p,i=1,z=-2147483649,C=1");
    assert_eq!(h.take(), format!("\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;{bad}\x1b\\"));
}

#[test]
fn a_key_section_past_1_kib_ignores_the_whole_string() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let pad = ",N=0".repeat(260);
    send(&mut k, &mut h, &format!("a=T,s=1,v=1,i=1{pad};{}", red(1)));
    assert_eq!(h.take(), "");
    assert!(h.store.is_empty());
    // a section of exactly 1024 bytes is read
    let keys = "a=T,s=1,v=1,i=2,C=1,N=0".to_owned();
    let fill = ",N=0".repeat((1024 - keys.len()) / 4);
    let keys = format!("{keys}{fill}{}", ",".repeat(1024 - keys.len() - fill.len()));
    assert_eq!(keys.len(), 1024);
    send(&mut k, &mut h, &format!("{keys};{}", red(1)));
    assert_eq!(h.take(), "\x1b_Gi=2;OK\x1b\\");
}

#[test]
fn raw_data_short_of_its_pixels_is_refused_and_data_past_them_ignored() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=2,v=1,i=1;{}", red(1)));
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=2;{}", red(3)));
    assert_eq!(
        h.take(),
        "\x1b_Gi=1;EINVAL:invalid argument\x1b\\\x1b_Gi=2;OK\x1b\\"
    );
    assert_eq!(h.store.image_count(), 1);
    assert_eq!(h.store.bytes(), 4 + IMAGE_OVERHEAD);
}

// covers: image.kitty
#[test]
fn chunks_hold_one_reservation_and_answer_once_at_the_last() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let data = red(4);
    let (a, b) = data.split_at(8);
    send(&mut k, &mut h, &format!("a=t,s=2,v=2,i=1,m=1;{a}"));
    // the whole picture is reserved at the first chunk
    assert_eq!(h.store.bytes(), 16 + IMAGE_OVERHEAD);
    assert_eq!(h.take(), "");
    send(&mut k, &mut h, &format!("m=0,q=1;{b}"));
    assert_eq!(h.take(), "", "a chunk's q holds from then on");
    assert_eq!(h.store.image_count(), 1);
    assert_eq!(h.store.bytes(), 16 + IMAGE_OVERHEAD);
}

#[test]
fn another_command_ends_an_upload_and_gives_back_its_reservation() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=2,v=2,i=1,m=1;{}", red(1)));
    assert_eq!(h.store.bytes(), 16 + IMAGE_OVERHEAD);
    send(&mut k, &mut h, "a=d,d=i,i=9");
    assert_eq!(h.store.bytes(), 0);
    // its remaining chunk is no continuation now: it transmits with the
    // defaults, without s and v, and is refused
    send(&mut k, &mut h, &format!("m=0;{}", red(3)));
    assert_eq!(h.take(), "");
    assert!(h.store.is_empty());
}

#[test]
fn a_failed_upload_swallows_its_chunks_and_answers_once() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=2,v=2,i=1,m=1;{}", red(1)));
    send(&mut k, &mut h, "m=1;AA*A");
    assert_eq!(h.store.bytes(), 0, "the failure gives the bytes back");
    send(&mut k, &mut h, &format!("m=1;{}", red(1)));
    send(&mut k, &mut h, &format!("m=0;{}", red(2)));
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    assert!(h.store.is_empty());
}

#[test]
fn cancel_and_reset_give_back_an_upload_in_chunks() {
    for reset in [false, true] {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        send(&mut k, &mut h, &format!("a=t,s=2,v=2,i=1,m=1;{}", red(1)));
        k.start(&mut h, Intro::String);
        k.feed(&mut h, Chunk::Bytes(b"m=1;AAAA"));
        if reset {
            k.cancel(&mut h);
            k.reset(&mut h);
            assert_eq!(k, Kitty::default());
        } else {
            k.cancel(&mut h);
        }
        assert_eq!(h.store.bytes(), 0);
        send(&mut k, &mut h, &format!("m=0;{}", red(3)));
        assert_eq!(h.take(), "");
        assert!(h.store.is_empty());
    }
}

#[test]
fn the_budget_refuses_a_picture_before_its_data() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_limit(1000);
    send(&mut k, &mut h, &format!("a=t,s=16,v=16,i=1;{}", red(256)));
    assert_eq!(h.take(), "\x1b_Gi=1;ENOSPC:image storage is full\x1b\\");
    assert_eq!(h.store.bytes(), 0);
    // past max-graphic-size, refused before the budget is asked
    h.limits.graphic = (4, 4);
    send(&mut k, &mut h, &format!("a=t,s=5,v=1,i=2;{}", red(5)));
    assert_eq!(h.take(), "\x1b_Gi=2;EINVAL:invalid argument\x1b\\");
}

// covers: image.kitty
#[test]
fn the_cursor_goes_after_the_box_and_wraps_at_the_right_edge() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    // 3 columns by 2 rows at 1,0: down one row, to column 4
    h.cursor = (1, 0);
    send(
        &mut k,
        &mut h,
        &format!("a=T,s=24,v=32,i=1;{}", red(24 * 32)),
    );
    assert_eq!(h.cursor, (4, 1));
    // reaching the right edge: down once more, to column 0
    h.cursor = (9, 1);
    send(&mut k, &mut h, "a=p,i=1");
    assert_eq!(h.cursor, (0, 3));
    // C=1 leaves it
    h.cursor = (2, 0);
    send(&mut k, &mut h, "a=p,i=1,C=1");
    assert_eq!(h.cursor, (2, 0));
    // a pending wrap places at the last column
    h.cursor = (12, 0);
    send(&mut k, &mut h, "a=p,i=1,c=1,r=1,C=1");
    assert!(h.boxes().contains(&(11, 0, 1, 1)));
}

#[test]
fn a_placement_id_replaces_its_placement_and_is_ignored_without_an_image_id() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=1;{}", red(1)));
    send(&mut k, &mut h, "a=p,i=1,p=3,C=1");
    h.cursor = (5, 2);
    send(&mut k, &mut h, "a=p,i=1,p=3,C=1");
    assert_eq!(h.boxes(), vec![(5, 2, 1, 1)]);
    // an image sent with neither i nor I has no id, so p means nothing
    send(&mut k, &mut h, &format!("a=T,s=1,v=1,p=3,C=1;{}", red(1)));
    send(&mut k, &mut h, &format!("a=T,s=1,v=1,p=3,C=1;{}", red(1)));
    assert_eq!(h.boxes().len(), 3);
}

#[test]
fn a_put_by_number_takes_the_newest_and_names_its_id() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=1;{}", red(1)));
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,I=4,q=1;{}", red(1)));
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,I=4,q=1;{}", red(1)));
    let _ = h.take();
    send(&mut k, &mut h, "a=p,I=4,C=1");
    send(&mut k, &mut h, "a=p,I=5,C=1");
    send(&mut k, &mut h, "a=p,C=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=3,I=4;OK\x1b\\\x1b_GI=5;ENOENT:image not found\x1b\\"
    );
}

#[test]
fn a_delete_answers_only_an_error() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=T,s=1,v=1,i=1,C=1;{}", red(1)));
    let _ = h.take();
    send(&mut k, &mut h, "a=d,d=i,i=1");
    send(&mut k, &mut h, "a=d,d=I,i=9");
    assert_eq!(h.take(), "");
    assert_eq!(
        h.store.image_count(),
        1,
        "a lowercase delete keeps the data"
    );
    send(&mut k, &mut h, "a=d,d=I,i=1,I=1");
    send(&mut k, &mut h, "a=d,d=w,i=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=1,I=1;EINVAL:invalid argument\x1b\\\x1b_Gi=1;EINVAL:invalid argument\x1b\\"
    );
    send(&mut k, &mut h, "a=d,d=I,i=1");
    assert!(h.store.is_empty());
}

/// `n` RGBA pixels of `rgba`, as base64.
fn rgba(rgba: [u8; 4], n: usize) -> String {
    b64(&rgba.repeat(n))
}

/// A host holding image 1, 256 by 256 pixels of opaque red, too large for
/// D1's first rule, so a small frame stays a change; replies taken.
fn animated() -> (Kitty, Host) {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_limit(4 << 20);
    send(
        &mut k,
        &mut h,
        &format!("a=t,s=256,v=256,i=1;{}", red(256 * 256)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    (k, h)
}

fn key(h: &mut Host) -> crate::ImageKey {
    h.store.find_image(1, 0).unwrap()
}

// covers: image.kitty-animation
#[test]
fn animation_commands_are_answered_as_a_put_is() {
    let (mut k, mut h) = animated();
    // a frame on the default color, then one on frame 2, each answered
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,s=2,v=1;{}", rgba([0, 0, 0xff, 0xff], 2)),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,x=4,y=4,s=1,v=1,c=2;{}", red(1)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\");
    let key = key(&mut h);
    assert_eq!(h.store.animation(key).unwrap().frames, 3);
    // a control and a composition
    send(&mut k, &mut h, "a=a,i=1,c=3,s=3,v=2,r=1,z=80");
    send(&mut k, &mut h, "a=c,i=1,r=2,c=3,w=2,h=1");
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\");
    let st = h.store.animation(key).unwrap();
    assert_eq!((st.current, st.run, st.loops), (3, crate::Run::Looping, 2));
    assert_eq!(h.store.frame_gap(key, 1), Some(80));
    // ENOENT for a missing image or frame, EINVAL for a rectangle out of
    // the image, a frame past FRAMES_MAX's room aside
    send(&mut k, &mut h, &format!("a=f,i=9,s=1,v=1;{}", red(1)));
    send(&mut k, &mut h, &format!("a=f,i=1,s=1,v=1,c=7;{}", red(1)));
    send(&mut k, &mut h, &format!("a=f,i=1,s=1,v=1,r=7;{}", red(1)));
    send(&mut k, &mut h, &format!("a=f,i=1,x=256,s=1,v=1;{}", red(1)));
    send(&mut k, &mut h, "a=a,i=1,c=4");
    send(&mut k, &mut h, "a=c,i=1,r=2,c=5");
    send(&mut k, &mut h, "a=c,i=1,r=2,c=2,x=1,w=4,h=4");
    assert_eq!(
        h.take(),
        "\x1b_Gi=9;ENOENT:image not found\x1b\\\
         \x1b_Gi=1;ENOENT:image not found\x1b\\\
         \x1b_Gi=1;ENOENT:image not found\x1b\\\
         \x1b_Gi=1;EINVAL:invalid argument\x1b\\\
         \x1b_Gi=1;ENOENT:image not found\x1b\\\
         \x1b_Gi=1;ENOENT:image not found\x1b\\\
         \x1b_Gi=1;EINVAL:invalid argument\x1b\\"
    );
    assert_eq!(h.store.animation(key).unwrap().frames, 3);
    // every animation command must name its image: with neither i nor I it
    // is refused, and with no id there is nothing to answer
    let before = h.store.clone();
    send(&mut k, &mut h, &format!("a=f,s=1,v=1;{}", red(1)));
    send(&mut k, &mut h, "a=a,c=1");
    send(&mut k, &mut h, "a=c,r=1,c=2");
    assert_eq!(h.take(), "");
    assert!(h.store == before);
    // an image number names the newest image of that number, answered
    // with its id
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,I=5;{}", red(1)));
    send(&mut k, &mut h, &format!("a=f,I=5,s=1,v=1;{}", red(1)));
    assert_eq!(h.take(), "\x1b_Gi=2,I=5;OK\x1b\\\x1b_Gi=2,I=5;OK\x1b\\");
}

// covers: image.kitty-animation
#[test]
fn a_frame_without_s_and_v_reaches_the_images_edge() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=3,v=2,i=1;{}", red(6)));
    // from x=1, the frame's data is 2 by 2
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,x=1;{}", rgba([0, 0, 0xff, 0xff], 4)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\");
    let key = key(&mut h);
    let p = h.store.frame_pixels(key, 2).unwrap();
    assert_eq!(
        p.pixels.as_slice(),
        &[0, 0xff00_00ff, 0xff00_00ff, 0, 0xff00_00ff, 0xff00_00ff]
    );
    // data short of the rectangle is refused
    send(&mut k, &mut h, &format!("a=f,i=1;{}", red(5)));
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    assert_eq!(h.store.frame_bytes(), 3 * 2 * 4 + crate::FRAME_OVERHEAD);
}

// covers: image.kitty-animation
#[test]
fn a_frame_in_chunks_holds_one_reservation_and_answers_once() {
    let (mut k, mut h) = animated();
    let data = rgba([0, 0, 0xff, 0xff], 16);
    let (a, b) = data.split_at(20);
    send(&mut k, &mut h, &format!("a=f,i=1,s=4,v=4,m=1;{a}"));
    assert_eq!(h.store.frame_bytes(), 4 * 4 * 4 + crate::FRAME_OVERHEAD);
    assert_eq!(h.take(), "");
    send(&mut k, &mut h, &format!("a=f,m=0;{b}"));
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    let key = key(&mut h);
    assert_eq!(h.store.animation(key).unwrap().frames, 2);
    // another command ends an upload in chunks and gives back what it holds
    let held = h.store.frame_bytes();
    send(&mut k, &mut h, &format!("a=f,i=1,s=4,v=4,m=1;{a}"));
    send(&mut k, &mut h, "a=a,i=1,s=1");
    assert_eq!(h.store.frame_bytes(), held);
    assert_eq!(h.store.animation(key).unwrap().frames, 2);
}

// covers: image.kitty-animation
#[test]
fn a_chunk_that_repeats_the_first_chunks_keys_still_continues_the_upload() {
    // kitty-doom's shape, at 2 by 2 pixels of RGB: the picture sent once
    // with a=T over the whole screen, then each frame as an edit of the
    // root frame whose later chunks repeat a=f,r=1,q=2, then a=a showing
    // frame 1; q=2 hides every refusal
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let black = b64(&[0, 0, 0].repeat(4));
    let blue = b64(&[0, 0, 0xff].repeat(4));
    let (a, b) = black.split_at(8);
    send(
        &mut k,
        &mut h,
        &format!("a=T,i=7,f=24,s=2,v=2,q=2,c=12,r=4,C=1,m=1;{a}"),
    );
    send(&mut k, &mut h, &format!("m=0;{b}"));
    let (a, b) = blue.split_at(8);
    send(
        &mut k,
        &mut h,
        &format!("a=f,r=1,i=7,f=24,q=2,x=0,y=0,s=2,v=2,m=1;{a}"),
    );
    send(&mut k, &mut h, &format!("a=f,r=1,q=2,m=0;{b}"));
    send(&mut k, &mut h, "a=a,q=2,c=1,i=7");
    assert_eq!(h.take(), "");
    let key = h.store.find_image(7, 0).unwrap();
    let root = h.store.frame_pixels(key, 1).unwrap();
    assert_eq!(
        root.pixels.as_slice(),
        &[0xff00_00ff; 4],
        "the edit reached the root frame"
    );
    assert_eq!(h.boxes(), vec![(0, 0, 12, 4)]);
    // the same holds for a picture's own upload: a later chunk that repeats
    // a=t and other keys continues it, and only its m and q are read
    let (a, b) = blue.split_at(8);
    send(&mut k, &mut h, &format!("a=t,i=8,f=24,s=2,v=2,m=1;{a}"));
    send(&mut k, &mut h, &format!("a=t,i=9,s=5,m=0,q=1;{b}"));
    assert_eq!(
        h.take(),
        "",
        "the chunk's q=1 holds, and its i=9 is not read"
    );
    let key = h.store.find_image(8, 0).unwrap();
    assert_eq!(
        h.store.frame_pixels(key, 1).unwrap().pixels.as_slice(),
        &[0xff00_00ff; 4]
    );
    assert_eq!(h.store.find_image(9, 0), None);
    // a chunk that names another action or medium is another command, and
    // ends the upload
    send(&mut k, &mut h, &format!("a=t,i=10,f=24,s=2,v=2,m=1;{a}"));
    send(&mut k, &mut h, &format!("a=T,i=10,f=24,s=2,v=2;{blue}"));
    send(&mut k, &mut h, &format!("a=t,i=11,f=24,s=2,v=2,m=1;{a}"));
    send(&mut k, &mut h, "a=t,t=f,i=11,f=24,s=2,v=2;AAAA");
    assert_eq!(
        h.take(),
        "\x1b_Gi=10;OK\x1b\\\x1b_Gi=11;EBADF:Failed to read image file\x1b\\"
    );
    assert_eq!(h.boxes().len(), 2, "a=T started anew and placed image 10");
}

// covers: image.kitty-animation
#[test]
fn d_f_deletes_every_frame_but_the_root_and_needs_an_image() {
    for d in ["f", "F"] {
        let (mut k, mut h) = animated();
        send(&mut k, &mut h, "a=p,i=1,C=1");
        for _ in 0..3 {
            send(&mut k, &mut h, &format!("a=f,i=1,s=1,v=1;{}", red(1)));
        }
        send(&mut k, &mut h, "a=a,i=1,c=3,s=3");
        let _ = h.take();
        let key = key(&mut h);
        let st = h.store.animation(key).unwrap();
        // with neither i nor I, nothing
        send(&mut k, &mut h, &format!("a=d,d={d}"));
        assert_eq!(h.store.animation(key).unwrap(), st);
        send(&mut k, &mut h, &format!("a=d,d={d},i=1"));
        assert_eq!(h.take(), "", "a delete answers only an error");
        let gone = h.store.animation(key).unwrap();
        assert_eq!(
            (gone.frames, gone.current, gone.run),
            (1, 1, crate::Run::Stopped)
        );
        assert_eq!(
            (gone.frame_stamp, gone.run_stamp),
            (st.frame_stamp + 1, st.run_stamp + 1)
        );
        assert_eq!(h.store.frame_bytes(), 0);
        // the image and its placement stay, whichever case
        assert_eq!(h.store.image_count(), 1);
        assert_eq!(h.boxes().len(), 1);
    }
}

// covers: image.kitty-animation
#[test]
fn a_frame_from_the_helper_is_reserved_of_the_allowance_and_stored() {
    let (mut k, mut h) = animated();
    h.helper = true;
    // as kitty's icat sends a GIF's frames: shared memory, on frame 1
    send(
        &mut k,
        &mut h,
        &format!("a=f,t=s,f=24,s=2,v=1,c=1,z=100,i=1;{}", b64(b"shm")),
    );
    let r = h.requests.pop().unwrap();
    assert_eq!(r.pixels.map(|p| (p.width, p.height)), Some((2, 1)));
    let budget = h.store.bytes();
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 2,
            height: 1,
            data: vec![0, 0, 0xff, 0xff, 0, 0, 0xff, 0xff],
        }),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    let key = key(&mut h);
    assert_eq!(h.store.frame_gap(key, 2), Some(100));
    assert_eq!(
        &h.store.frame_pixels(key, 2).unwrap().pixels.as_slice()[..3],
        &[0xff00_00ff, 0xff00_00ff, 0xffff_0000]
    );
    assert_eq!(h.store.bytes(), budget, "a frame is never in the budget");
    // a PNG frame that leaves the image is EINVAL once its size is known
    send(
        &mut k,
        &mut h,
        &format!("a=f,t=f,f=100,x=255,i=1;{}", b64(b"/p")),
    );
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 2,
            height: 1,
            data: vec![0; 8],
        }),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    assert_eq!(h.store.animation(key).unwrap().frames, 2);
}

// covers: image.kitty-animation
#[test]
fn frames_and_controls_fed_whole_or_cut_end_in_the_same_store_whatever_is_drawn() {
    let stream = [
        format!("a=t,s=256,v=256,i=1;{}", red(256 * 256)),
        format!("a=f,i=1,s=3,v=2,x=5,y=7;{}", rgba([0, 0x80, 0, 0x80], 6)),
        format!("a=f,i=1,s=2,v=2,c=2,X=1;{}", rgba([0, 0, 0xff, 0xff], 4)),
        format!("a=f,i=1,r=3,s=1,v=1,x=9;{}", rgba([0xff, 0xff, 0, 0x40], 1)),
        "a=a,i=1,r=1,z=50".to_string(),
        "a=a,i=1,s=3,v=1,c=2".to_string(),
        "a=c,i=1,r=3,c=2,w=4,h=4".to_string(),
        format!(
            "a=f,i=1,s=256,v=128,Y=4278190335;{}",
            rgba([0, 0, 0, 0], 256 * 128)
        ),
        "a=a,i=1,s=1".to_string(),
        // a change frame stays, so a drawn run's cache holds it at the end
        format!("a=f,i=1,s=1,v=1,c=4,x=3;{}", red(1)),
    ];
    let run = |cuts: u64, draw: bool| {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        h.store.set_limit(4 << 20);
        let mut seed = cuts;
        for (n, cmd) in stream.iter().enumerate() {
            let bytes = cmd.as_bytes();
            let mut pieces = Vec::new();
            let mut at = 0;
            while at < bytes.len() && cuts > 0 {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let step = 1 + (seed >> 33) as usize % 97;
                let end = (at + step).min(bytes.len());
                pieces.push(&bytes[at..end]);
                at = end;
            }
            if cuts == 0 {
                pieces.push(bytes);
            }
            string(&mut k, &mut h, &pieces);
            if draw && n > 0 {
                let key = h.store.find_image(1, 0).unwrap();
                let frames = h.store.animation(key).map_or(1, |a| a.frames);
                for f in 1..=frames {
                    let _ = h.store.frame_pixels(key, f);
                }
            }
        }
        (h.take(), h.store, k)
    };
    let whole = run(0, false);
    let key = whole.1.clone().find_image(1, 0).unwrap();
    assert_eq!(whole.1.animation(key).unwrap().frames, 5);
    for (cuts, draw) in [
        (0, true),
        (1, false),
        (7, true),
        (31, false),
        (1_000_003, true),
    ] {
        let cut = run(cuts, draw);
        assert_eq!(cut.0, whole.0, "cuts {cuts}");
        assert!(cut.1 == whole.1, "cuts {cuts}, drawn {draw}");
        assert_eq!(cut.2, whole.2);
        assert_eq!(cut.1.frame_bytes(), whole.1.frame_bytes());
    }
}

// covers: image.kitty
#[test]
fn without_a_helper_named_media_are_ebadf_and_png_and_zlib_einval() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=100,i=1;{}", b64(b"/tmp/a")),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=t,f=100,i=2;{}", b64(b"/tmp/a")),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=t,f=100,i=3;{}", b64(b"\x89PNG")),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=t,o=z,s=1,v=1,i=4;{}", b64(b"xx")),
    );
    let bad = "EBADF:Failed to read image file";
    let inv = "EINVAL:invalid argument";
    assert_eq!(
        h.take(),
        format!(
            "\x1b_Gi=1;{bad}\x1b\\\x1b_Gi=2;{bad}\x1b\\\x1b_Gi=3;{inv}\x1b\\\x1b_Gi=4;{inv}\x1b\\"
        )
    );
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
}

// covers: image.kitty
#[test]
fn a_named_medium_goes_to_the_helper_and_its_answer_is_stored() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    send(
        &mut k,
        &mut h,
        &format!("a=T,t=f,f=24,s=1,v=1,O=2,S=9,i=1,C=1;{}", b64(b"/x/p")),
    );
    assert_eq!(h.requests.len(), 1);
    let r = h.requests.pop().unwrap();
    assert_eq!(r.medium, Medium::File(b"/x/p".to_vec()));
    assert_eq!(
        (r.offset, r.max, r.min, r.png, r.inflate),
        (2, 9, 3, false, false)
    );
    // the helper converts the pixels the request describes
    assert_eq!(
        r.pixels,
        Some(Description {
            layout: Layout::Rgb,
            alpha: Alpha::Straight,
            width: 1,
            height: 1,
            compressed: false,
        })
    );
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 1,
            height: 1,
            data: vec![9, 8, 7, 0xff],
        }),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(h.boxes(), vec![(0, 0, 1, 1)]);
    // a read too short is EBADF; an inflate too short is EINVAL
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=s,s=1,v=1,i=2;{}", b64(b"shm")),
    );
    k.decoded(&mut h, Reply::TooShort);
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,o=z,s=1,v=1,i=3;{}", b64(b"/z")),
    );
    k.decoded(&mut h, Reply::TooShort);
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=100,i=4;{}", b64(b"/big")),
    );
    k.decoded(&mut h, Reply::OverBudget);
    assert_eq!(
        h.take(),
        "\x1b_Gi=2;EBADF:Failed to read image file\x1b\\\
         \x1b_Gi=3;EINVAL:invalid argument\x1b\\\
         \x1b_Gi=4;ENOSPC:image storage is full\x1b\\"
    );
}

#[test]
fn a_direct_png_is_kept_under_the_budget_and_given_back_at_its_answer() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    let png = b64(&[7u8; 5000]);
    let (a, b) = png.split_at(4000);
    send(
        &mut k,
        &mut h,
        &format!("a=T,f=100,o=z,S=40,i=5,m=1,C=1;{a}"),
    );
    // the buffer doubled from 4 KiB to hold 3000 bytes, then 5000
    assert_eq!(h.store.bytes(), 4096);
    send(&mut k, &mut h, &format!("m=0;{b}"));
    assert_eq!(h.store.bytes(), 8192);
    let r = h.requests.pop().unwrap();
    assert_eq!(r.medium, Medium::Direct(vec![7u8; 5000]));
    assert!(r.png && r.inflate && r.exact);
    assert_eq!(r.min, 40);
    let rgba = Payload::Rgba {
        width: 1,
        height: 2,
        data: vec![0, 0, 0xff, 0xff, 0, 0xff, 0, 0],
    };
    k.decoded(&mut h, Reply::Ok(rgba));
    assert_eq!(h.take(), "\x1b_Gi=5;OK\x1b\\");
    assert_eq!(h.store.bytes(), 8 + IMAGE_OVERHEAD);
}

#[test]
fn a_refused_temporary_file_still_goes_to_the_helper_to_be_removed() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=t,f=24,i=1;{}", b64(b"/tmp/tty-graphics-protocol-1")),
    );
    let r = h.requests.pop().unwrap();
    assert!(r.discard);
    assert_eq!((r.min, r.png, r.inflate), (0, false, false));
    k.decoded(&mut h, Reply::Ok(Payload::Discarded));
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    // a refused file is not removable and makes no request
    send(&mut k, &mut h, &format!("a=t,t=f,f=24,i=2;{}", b64(b"/a")));
    assert!(h.requests.is_empty());
    assert_eq!(h.take(), "\x1b_Gi=2;EINVAL:invalid argument\x1b\\");
}

#[test]
fn a_name_that_holds_nul_or_runs_long_is_ebadf_with_no_request() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=100,i=1;{}", b64(b"/a\0b")),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=100,i=2;{}", b64(&[b'a'; 4096])),
    );
    send(&mut k, &mut h, "a=t,t=f,f=100,i=3;");
    send(&mut k, &mut h, "a=t,t=f,f=100,i=4;L*E=");
    assert!(h.requests.is_empty());
    let bad = "EBADF:Failed to read image file";
    assert_eq!(
        h.take(),
        format!(
            "\x1b_Gi=1;{bad}\x1b\\\x1b_Gi=2;{bad}\x1b\\\x1b_Gi=3;{bad}\x1b\\\
             \x1b_Gi=4;EINVAL:invalid argument\x1b\\"
        )
    );
    // 4095 bytes is the longest name
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=100,i=5;{}", b64(&[b'a'; 4095])),
    );
    assert_eq!(h.requests.len(), 1);
}

// covers: image.kitty
#[test]
fn a_reservation_for_an_id_keeps_the_old_image_when_the_new_one_is_refused() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_limit(16 + IMAGE_OVERHEAD + 600);
    send(&mut k, &mut h, &format!("a=T,s=2,v=2,i=1,C=1;{}", red(4)));
    // a picture of id 1 too large even with the old one's bytes
    send(&mut k, &mut h, &format!("a=t,s=16,v=16,i=1;{}", red(256)));
    assert_eq!(
        h.take(),
        "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;ENOSPC:image storage is full\x1b\\"
    );
    assert_eq!(h.boxes(), vec![(0, 0, 1, 1)]);
    assert_eq!(h.store.bytes(), 16 + IMAGE_OVERHEAD);
    // one that fits with them replaces it, and its placement goes
    send(&mut k, &mut h, &format!("a=t,s=10,v=10,i=1;{}", red(100)));
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    assert!(h.boxes().is_empty());
    // the replaced image's 16 bytes of pixels stay as the store's spare
    // buffer, charged to the budget, which had room for them
    assert_eq!(h.store.bytes(), 400 + IMAGE_OVERHEAD + 16);
}

// covers: image.kitty
#[test]
fn the_cursor_after_a_tall_image_is_bounded_by_the_margin_and_a_screen() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    // the cursor below the region: the rows indexed stop at the screen's
    // last row, and the column goes past the box
    h.margins = Margins { top: 0, bottom: 1 };
    h.cursor = (3, 2);
    send(&mut k, &mut h, &format!("a=T,s=8,v=96,i=1;{}", red(8 * 96)));
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(h.boxes(), vec![(3, 2, 1, 6)]);
    // 5 rows asked of a box 6 rows tall, bounded to no margin room and one
    // screen of 4 rows
    assert_eq!(h.indexed, vec![4]);
    assert_eq!(h.cursor, (4, 3));
    // a box as wide as the screen from column 0 wraps: down one row more
    // than its rows less one, to column 0
    h.margins = Margins { top: 0, bottom: 3 };
    h.cursor = (0, 0);
    send(
        &mut k,
        &mut h,
        &format!("a=T,s=96,v=16,i=2;{}", red(96 * 16)),
    );
    assert_eq!(h.cursor, (0, 1));
}

// covers: image.kitty
#[test]
fn point_deletes_take_kitty_placements_and_leave_a_sixel_beside_them() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    // a sixel over the first two cells of row 0, made through the store
    let screens = Screens {
        main: Rows {
            screen: &h.screen,
            above: &[],
        },
        alt: Rows::default(),
    };
    let sixel = crate::NewImage {
        protocol: crate::Protocol::SIXEL,
        width: 16,
        height: 16,
        pixels: vec![0xff00_00ff; 256],
        id: 0,
        number: 0,
        transient: false,
        opaque: None,
    };
    let key = h.store.add_image(sixel, None, screens).unwrap();
    let at = crate::Place::default();
    h.store.place(key, at, screens, 12).unwrap();
    h.cursor = (2, 0);
    send(
        &mut k,
        &mut h,
        &format!("a=T,s=16,v=16,i=1,z=1,C=1;{}", red(256)),
    );
    h.cursor = (2, 1);
    send(
        &mut k,
        &mut h,
        &format!("a=T,s=16,v=16,i=2,z=2,C=1;{}", red(256)),
    );
    let _ = h.take();
    let all = vec![(0, 0, 2, 1), (2, 0, 2, 1), (2, 1, 2, 1)];
    assert_eq!(h.boxes(), all);
    // d=p on the sixel's cell takes nothing, and on kitty 1's takes it
    send(&mut k, &mut h, "a=d,d=p,x=1,y=1");
    assert_eq!(h.boxes(), all);
    send(&mut k, &mut h, "a=d,d=p,x=3,y=1");
    assert_eq!(h.boxes(), vec![(0, 0, 2, 1), (2, 1, 2, 1)]);
    // d=q needs the z too
    send(&mut k, &mut h, "a=d,d=q,x=4,y=2,z=1");
    assert_eq!(h.boxes().len(), 2);
    send(&mut k, &mut h, "a=d,d=q,x=4,y=2,z=2");
    assert_eq!(h.boxes(), vec![(0, 0, 2, 1)]);
    // d=x and d=y through the sixel's column and row leave it
    h.cursor = (0, 1);
    send(&mut k, &mut h, "a=p,i=1,C=1");
    send(&mut k, &mut h, "a=d,d=x,x=1");
    assert_eq!(h.boxes(), vec![(0, 0, 2, 1)]);
    send(&mut k, &mut h, "a=p,i=1,C=1");
    send(&mut k, &mut h, "a=d,d=y,y=1");
    assert_eq!(h.boxes(), vec![(0, 0, 2, 1), (0, 1, 2, 1)]);
    send(&mut k, &mut h, "a=d,d=y,y=2");
    assert_eq!(h.boxes(), vec![(0, 0, 2, 1)]);
    // a coordinate not given chooses nothing
    send(&mut k, &mut h, "a=p,i=1,C=1");
    send(&mut k, &mut h, "a=d,d=p,x=1");
    send(&mut k, &mut h, "a=d,d=x");
    assert_eq!(h.boxes().len(), 2);
    // the puts are answered, and no delete is
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\".repeat(3));
}

#[test]
fn a_cancelled_string_whose_keys_passed_1_kib_still_ends_an_upload() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=2,v=2,i=1,m=1;{}", red(1)));
    assert_eq!(h.store.bytes(), 16 + IMAGE_OVERHEAD);
    // a string whose key section passes 1 KiB, cut off by CAN or SUB
    k.start(&mut h, Intro::String);
    k.feed(&mut h, Chunk::Bytes(",N=0".repeat(300).as_bytes()));
    k.cancel(&mut h);
    assert_eq!(h.store.bytes(), 0, "the upload gave its reservation back");
    // its last chunk is no continuation now, and is refused unanswered
    send(&mut k, &mut h, &format!("m=0;{}", red(3)));
    assert_eq!(h.take(), "");
    assert!(h.store.is_empty());
}

// covers: image.kitty
#[test]
fn a_run_fed_a_few_bytes_of_work_at_a_time_stores_what_it_stores_whole() {
    let data = format!(
        "a=T,f=24,s=5,v=3,i=1,c=2;{}",
        b64(&(0..45u8).collect::<Vec<_>>())
    );
    let whole = {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        send(&mut k, &mut h, &data);
        (h.take(), h.store.clone())
    };
    for left in [0u64, 1, 3, 7, 1000] {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        k.start(&mut h, Intro::String);
        let mut rest = data.as_bytes();
        let mut calls = 0;
        while !rest.is_empty() {
            let n = k.feed_some(&mut h, rest, left);
            assert!(n >= 1 && n <= rest.len(), "took {n} of {}", rest.len());
            // the keys whole, then at most the work left, or one byte
            let keys = rest.iter().position(|&b| b == b';').map(|e| e + 1);
            if keys.is_none() || calls > 0 {
                assert!(n as u64 <= left.max(1), "took {n} with {left} left");
            }
            rest = &rest[n..];
            calls += 1;
        }
        k.finish(&mut h);
        assert_eq!(h.take(), whole.0, "{left} left");
        assert!(h.store == whole.1, "{left} left");
    }
}

impl Host {
    /// The address of the pixels of the topmost picture placed at column
    /// `col`.
    fn pixels_at(&self, col: u32) -> *const u32 {
        let view = Rows {
            screen: &self.screen,
            above: &[],
        };
        let frame = self.store.frame(view, self.cols);
        frame
            .iter()
            .rfind(|p| p.col == col)
            .map(|p| p.pixels.as_slice().as_ptr())
            .unwrap()
    }
}

// fork-perf's buffer reuse: a frame that replaces one of its size fills the
// buffer the store freed last instead of fresh memory, in timg's pattern of
// two alternating ids and in ncplayer's of a new id and a delete each frame
// covers: image.kitty
#[test]
fn replaced_frames_of_one_size_reuse_the_buffer_freed_last() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    let frame = |k: &mut Kitty, h: &mut Host, id: u32, col: usize| {
        h.cursor = (col, 0);
        send(k, h, &format!("a=T,s=4,v=4,i={id},q=2,C=1;{}", red(16)));
        h.pixels_at(u32::try_from(col).unwrap())
    };
    let a1 = frame(&mut k, &mut h, 1, 0);
    let b1 = frame(&mut k, &mut h, 2, 6);
    // frame 3 replaces id 1: nothing is free yet, and it frees a1
    let _a2 = frame(&mut k, &mut h, 1, 0);
    assert_eq!(frame(&mut k, &mut h, 2, 6), a1, "b2 fills a1's buffer");
    assert_eq!(frame(&mut k, &mut h, 1, 0), b1, "a3 fills b1's buffer");
    // ncplayer: transmit a new id, place it, delete the previous id, under
    // a roomy budget and under one of two frames, where the reservation
    // could only fit by freeing the spare had it not taken it first
    let frame_bytes = 16 * 4 + IMAGE_OVERHEAD;
    for limit in [1 << 20, 2 * frame_bytes] {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        h.store.set_limit(limit);
        let mut ptrs: Vec<*const u32> = Vec::new();
        for id in 10..16u32 {
            send(
                &mut k,
                &mut h,
                &format!("a=t,s=4,v=4,i={id},p=1;{}", red(16)),
            );
            send(&mut k, &mut h, &format!("a=p,i={id},p=1,q=1,C=1"));
            ptrs.push(h.pixels_at(0));
            send(&mut k, &mut h, &format!("a=d,d=I,i={},q=2", id - 1));
            assert!(h.store.bytes() <= limit, "frame {id} at {limit}");
        }
        assert_eq!(
            h.take(),
            (10..16)
                .map(|i| format!("\x1b_Gi={i},p=1;OK\x1b\\"))
                .collect::<String>()
        );
        // each frame fills the buffer of the frame deleted while it was
        // placed, the one before the last
        for i in 2..ptrs.len() {
            assert_eq!(ptrs[i], ptrs[i - 2], "frame {} at {limit}", 10 + i);
        }
        // the next frame's buffer is the spare, still holding frame 14's
        // pixels, with the whole charge reserved: under the tight budget a
        // reservation made before taking it would have freed it for room
        let ctx: &mut dyn Context = &mut h;
        let taken = ctx.edit(|e| e.kitty_buffer(16, 16, frame_bytes));
        let Some(Ok((buffer, reservation))) = taken else {
            panic!("no room for a frame at {limit}");
        };
        assert_eq!(buffer.len(), 16, "the spare, at {limit}");
        assert_eq!(reservation.bytes(), frame_bytes);
        ctx.unreserve(reservation);
    }
}

// timg's alternating ids under a budget that fits a static picture, two
// frames and the spare buffer, and no more. Whether the next frame takes
// the spare depends on whether a frame the host drew still holds it; the
// images the store keeps must not: the replacement counts the image it
// replaces whichever way it reserves.
// covers: image.kitty
#[test]
fn a_replacement_keeps_the_same_images_whether_or_not_it_takes_the_spare() {
    let frame_bytes = 16 * 4 + IMAGE_OVERHEAD;
    let limit = 3 * frame_bytes + 16 * 4;
    let run = |hold: bool| {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        h.store.set_limit(limit);
        h.cursor = (10, 0);
        send(
            &mut k,
            &mut h,
            &format!("a=T,s=4,v=4,i=99,q=2,C=1;{}", red(16)),
        );
        let frame = |k: &mut Kitty, h: &mut Host, id: u32, col: usize| {
            h.cursor = (col, 0);
            send(k, h, &format!("a=T,s=4,v=4,i={id},q=2,C=1;{}", red(16)));
        };
        frame(&mut k, &mut h, 1, 0);
        frame(&mut k, &mut h, 2, 6);
        let view = Rows {
            screen: &h.screen,
            above: &[],
        };
        // a frame the host drew holds a1's pixels, or none does
        let held = hold.then(|| h.store.frame(view, h.cols));
        frame(&mut k, &mut h, 1, 0); // a2 frees a1 into the spare
        frame(&mut k, &mut h, 2, 6); // b2 takes the spare unless a1 is held
        drop(held);
        (h.store.image_count(), h.boxes(), h.take())
    };
    let (held, free) = (run(true), run(false));
    assert_eq!(held.0, 3, "the static picture and both ids stay");
    assert_eq!(held, free);
}

/// Transmits image `id`, 16 by 16 opaque pixels, two cells wide and one
/// high, answered under no `q`.
fn wide(k: &mut Kitty, h: &mut Host, id: u32) {
    send(k, h, &format!("a=t,f=32,s=16,v=16,i={id},q=2;{}", red(256)));
}

/// Transmits image `id`, 8 by 32 opaque pixels, one cell wide and two
/// high, answered under no `q`.
fn tall(k: &mut Kitty, h: &mut Host, id: u32) {
    send(k, h, &format!("a=t,f=32,s=8,v=32,i={id},q=2;{}", red(256)));
}

/// The frame's boxes, sorted, as (column, row, columns, rows).
fn sorted(h: &Host) -> Vec<(u32, i64, u32, u32)> {
    let mut boxes = h.boxes();
    boxes.sort_unstable();
    boxes
}

// covers: image.kitty-relative
#[test]
fn a_relative_placement_draws_at_its_parents_origin_plus_its_offset() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    h.cursor = (2, 1);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=3,V=1");
    assert_eq!(h.take(), "\x1b_Gi=1,p=1;OK\x1b\\\x1b_Gi=2,p=1;OK\x1b\\");
    assert_eq!(sorted(&h), vec![(2, 1, 2, 1), (5, 2, 2, 1)]);
    assert_eq!(
        h.cursor,
        (2, 1),
        "a relative placement never moves the cursor"
    );
    // negative offsets go left and up, and the part left of the screen is
    // cut off
    send(&mut k, &mut h, "a=p,i=2,p=2,P=1,Q=1,H=-3,V=-1");
    assert_eq!(h.take(), "\x1b_Gi=2,p=2;OK\x1b\\");
    assert_eq!(sorted(&h), vec![(0, 0, 1, 1), (2, 1, 2, 1), (5, 2, 2, 1)]);
    // without Q, the parent is the image's placement made last that has no
    // placement id
    h.cursor = (0, 3);
    send(&mut k, &mut h, "a=p,i=1,C=1");
    h.cursor = (6, 3);
    send(&mut k, &mut h, "a=p,i=1,C=1");
    send(&mut k, &mut h, "a=p,i=2,p=3,P=1,H=2");
    assert_eq!(
        h.take(),
        "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\\x1b_Gi=2,p=3;OK\x1b\\"
    );
    assert!(sorted(&h).contains(&(8, 3, 2, 1)), "{:?}", sorted(&h));
    // P=0 names no image, so the put is an ordinary one at the cursor
    h.cursor = (0, 2);
    send(&mut k, &mut h, "a=p,i=2,p=4,P=0,H=5,C=1");
    assert!(sorted(&h).contains(&(0, 2, 2, 1)), "{:?}", sorted(&h));
}

// covers: image.kitty-relative
#[test]
fn a_relative_placement_follows_its_parent_through_a_scroll_and_the_scrollback() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    h.cursor = (1, 1);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,V=2,q=1");
    assert_eq!(sorted(&h), vec![(1, 1, 2, 1), (1, 3, 2, 1)]);
    h.scroll_up(0, 3);
    assert_eq!(sorted(&h), vec![(1, 0, 2, 1), (1, 2, 2, 1)]);
    // the parent leaves the screen for the scrollback, and the child still
    // draws: the frame is given the rows above the screen it reaches
    h.scroll_up(0, 3);
    assert_eq!(h.store.reach(), 2);
    assert_eq!(sorted(&h), vec![(1, 1, 2, 1)]);
    // the scrollback drops its oldest line, then the parent's: the parent
    // goes, and the child with it, taking its image, which has no
    // placement left
    let oldest = h.above.pop().unwrap();
    h.store.discard_line(oldest, h.above.last().copied());
    assert_eq!(sorted(&h), vec![(1, 1, 2, 1)]);
    let parents = h.above.pop().unwrap();
    h.store.discard_line(parents, h.screen.first().copied());
    assert_eq!(sorted(&h), vec![]);
    assert_eq!(h.store.placement_count(), 0);
    assert_eq!(h.store.image_count(), 1, "image 1 stays; image 2 went");
}

// covers: image.kitty-relative
#[test]
fn a_relative_placement_moves_with_its_parent_in_a_region_scroll() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    // a parent inside rows 1 to 2 and its child below the region: the
    // child moves because its parent does, wherever the margins are
    h.cursor = (0, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=3,V=1,q=1");
    assert_eq!(sorted(&h), vec![(0, 2, 2, 1), (3, 3, 2, 1)]);
    h.scroll_up(1, 2);
    assert_eq!(sorted(&h), vec![(0, 1, 2, 1), (3, 2, 2, 1)]);
    // a region scroll that cuts the parent's top keeps the child where it
    // was against the parent's picture
    let (mut k, mut h) = (Kitty::default(), Host::new());
    tall(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    h.cursor = (0, 1);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=1,V=1,q=1");
    assert_eq!(sorted(&h), vec![(0, 1, 1, 2), (1, 2, 2, 1)]);
    h.scroll_up(1, 3);
    assert_eq!(sorted(&h), vec![(0, 1, 1, 1), (1, 1, 2, 1)]);
    // a parent across the region's edge keeps its place, and so does its
    // child
    let (mut k, mut h) = (Kitty::default(), Host::new());
    tall(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    h.cursor = (0, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=1,q=1");
    h.scroll_up(1, 2);
    assert_eq!(sorted(&h), vec![(0, 2, 1, 2), (1, 2, 2, 1)]);
}

// covers: image.kitty-relative
#[test]
fn a_relative_placement_follows_its_parent_when_the_parent_is_replaced() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=2,V=1,q=1");
    assert_eq!(sorted(&h), vec![(0, 0, 2, 1), (2, 1, 2, 1)]);
    // a put with the same image and placement id replaces the parent, and
    // keeps its child
    h.cursor = (5, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    assert_eq!(sorted(&h), vec![(5, 2, 2, 1), (7, 3, 2, 1)]);
    assert_eq!(h.store.placement_count(), 2);
    // a parent replaced as a child of another placement takes its child
    // along
    h.cursor = (0, 0);
    wide(&mut k, &mut h, 3);
    send(&mut k, &mut h, "a=p,i=3,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=1,p=1,P=3,Q=1,V=1,q=1");
    assert_eq!(sorted(&h), vec![(0, 0, 2, 1), (0, 1, 2, 1), (2, 2, 2, 1)]);
}

// covers: image.kitty-relative
#[test]
fn deleting_a_parent_takes_its_children_and_images_left_without_placements() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    for id in 1..=4 {
        wide(&mut k, &mut h, id);
    }
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,H=2,q=1");
    send(&mut k, &mut h, "a=p,i=3,p=1,P=2,Q=1,H=2,q=1");
    // image 4 has a placement of its own beside its child of image 1
    h.cursor = (0, 2);
    send(&mut k, &mut h, "a=p,i=4,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=4,p=2,P=1,Q=1,V=1,q=1");
    assert_eq!(h.store.placement_count(), 5);
    // a lowercase delete keeps image 1's data; its child and grandchild go
    // with it and take their images, left with no placement, while image 4
    // keeps its own placement and its data
    send(&mut k, &mut h, "a=d,d=i,i=1");
    assert_eq!(sorted(&h), vec![(0, 2, 2, 1)]);
    assert_eq!(h.store.image_count(), 2);
    send(&mut k, &mut h, "a=p,i=2,C=1");
    send(&mut k, &mut h, "a=p,i=3,C=1");
    send(&mut k, &mut h, "a=p,i=1,C=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=2;ENOENT:image not found\x1b\\\x1b_Gi=3;ENOENT:image not found\x1b\\\x1b_Gi=1;OK\x1b\\"
    );
    // ED 2 takes a child by the rows it draws on, and with its parent
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,V=2,q=1");
    h.store.erase_screen(Rows {
        screen: &h.screen,
        above: &h.above,
    });
    assert_eq!(h.store.placement_count(), 0);
    assert_eq!(h.store.image_count(), 1, "the child's image went with it");
}

// covers: image.kitty-relative
#[test]
fn a_parent_that_does_not_exist_is_enoparent() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    wide(&mut k, &mut h, 1);
    wide(&mut k, &mut h, 2);
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    let enoparent = "\x1b_Gi=2;ENOPARENT:parent placement not found\x1b\\";
    for keys in ["a=p,i=2,P=9", "a=p,i=2,P=1,Q=5", "a=p,i=2,P=1"] {
        send(&mut k, &mut h, keys);
        assert_eq!(h.take(), enoparent, "{keys}");
    }
    assert_eq!(h.store.placement_count(), 1, "a refused put places nothing");
}

// covers: image.kitty-relative
#[test]
fn a_placement_that_would_follow_itself_is_ecycle() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    for id in 1..=3 {
        wide(&mut k, &mut h, id);
    }
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=2,p=1,P=1,Q=1,V=1,q=1");
    send(&mut k, &mut h, "a=p,i=3,p=1,P=2,Q=1,V=1,q=1");
    let before = sorted(&h);
    send(&mut k, &mut h, "a=p,i=1,p=1,P=3,Q=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=1,p=1;ECYCLE:relative placement would make a cycle\x1b\\"
    );
    send(&mut k, &mut h, "a=p,i=2,p=1,P=2,Q=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=2,p=1;ECYCLE:relative placement would make a cycle\x1b\\"
    );
    assert_eq!(
        sorted(&h),
        before,
        "a refused replacement keeps the old one"
    );
}

// covers: image.kitty-relative
#[test]
fn a_chain_deeper_than_eight_relative_placements_is_etoodeep() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    for id in 1..=11 {
        wide(&mut k, &mut h, id);
    }
    send(&mut k, &mut h, "a=p,i=1,p=1,C=1,q=1");
    // eight relative placements below image 1's
    for id in 2..=9 {
        send(&mut k, &mut h, &format!("a=p,i={id},p=1,P={},Q=1", id - 1));
        assert_eq!(h.take(), format!("\x1b_Gi={id},p=1;OK\x1b\\"));
    }
    let toodeep = "ETOODEEP:chain of relative placements is too deep";
    send(&mut k, &mut h, "a=p,i=10,p=1,P=9,Q=1");
    assert_eq!(h.take(), format!("\x1b_Gi=10,p=1;{toodeep}\x1b\\"));
    // the chain's first placement replaced as a child would put the chain
    // nine deep
    h.cursor = (0, 3);
    send(&mut k, &mut h, "a=p,i=11,p=1,C=1,q=1");
    send(&mut k, &mut h, "a=p,i=1,p=1,P=11,Q=1");
    assert_eq!(h.take(), format!("\x1b_Gi=1,p=1;{toodeep}\x1b\\"));
    // its second, with seven below it, fits
    send(&mut k, &mut h, "a=p,i=2,p=1,P=11,Q=1");
    assert_eq!(h.take(), "\x1b_Gi=2,p=1;OK\x1b\\");
    assert_eq!(h.store.placement_count(), 10);
}

// covers: image.kitty-placeholders
#[test]
fn a_virtual_placement_is_stored_on_no_line_and_only_a_delete_by_image_takes_it() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.cursor = (3, 1);
    send(
        &mut k,
        &mut h,
        &format!("a=T,f=32,s=16,v=16,i=1,p=7,U=1,c=4,r=2;{}", red(256)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1,p=7;OK\x1b\\");
    assert_eq!(
        h.cursor,
        (3, 1),
        "a virtual placement never moves the cursor"
    );
    assert_eq!(h.store.placement_count(), 1);
    assert_eq!(
        sorted(&h),
        vec![],
        "it draws only through placeholder cells"
    );
    // no screen event reaches it, and neither do the deletes by cell, row,
    // column or z
    for _ in 0..6 {
        h.scroll_up(0, 3);
    }
    h.scroll_up(1, 2);
    h.store.erase_screen(Rows {
        screen: &h.screen,
        above: &h.above,
    });
    for keys in [
        "a=d,d=A",
        "a=d,d=C",
        "a=d,d=P,x=4,y=2",
        "a=d,d=Q,x=4,y=2,z=0",
        "a=d,d=X,x=4",
        "a=d,d=Y,y=2",
        "a=d,d=Z,z=0",
    ] {
        send(&mut k, &mut h, keys);
        assert_eq!(h.store.placement_count(), 1, "{keys}");
    }
    // a virtual placement is never a child, though it may be a parent; its
    // child draws nothing while the store reads no placeholder cells
    wide(&mut k, &mut h, 2);
    send(&mut k, &mut h, "a=p,i=2,U=1,P=1,Q=7");
    assert_eq!(h.take(), "\x1b_Gi=2;EINVAL:invalid argument\x1b\\");
    send(&mut k, &mut h, "a=p,i=2,P=1,Q=7,H=1");
    assert_eq!(h.take(), "\x1b_Gi=2;OK\x1b\\");
    assert_eq!(h.store.placement_count(), 2);
    assert_eq!(sorted(&h), vec![]);
    // a delete by image takes it, and its child with it
    send(&mut k, &mut h, "a=d,d=i,i=1,p=7");
    assert_eq!(h.store.placement_count(), 0);
    assert_eq!(h.store.image_count(), 1, "image 1 stays; image 2 went");
}

/// A helper's reply file that holds `held` bytes of opaque red but says it
/// holds what the picture needs, as a lying or truncated file would.
#[derive(Debug)]
struct Lying {
    held: usize,
    says: u64,
}

impl ReplySource for Lying {
    fn bytes(&self) -> u64 {
        self.says
    }

    fn read(&self, sink: &mut dyn FnMut(&[u8])) -> bool {
        sink(&[0xff, 0, 0, 0xff].repeat(self.held / 4));
        true
    }
}

// covers: image.kitty
#[test]
fn a_reply_file_too_short_or_too_long_is_ebadf_and_gives_its_bytes_back() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    for (id, held) in [(1, 12), (2, 20)] {
        send(
            &mut k,
            &mut h,
            &format!("a=t,t=f,f=32,s=2,v=2,i={id};{}", b64(b"/x")),
        );
        assert_eq!(h.requests.len(), 1);
        h.requests.clear();
        let file = ReplyFile::new(Lying { held, says: 16 });
        k.decoded(
            &mut h,
            Reply::Ok(Payload::File {
                width: 2,
                height: 2,
                file,
            }),
        );
        assert_eq!(
            h.take(),
            format!("\x1b_Gi={id};EBADF:Failed to read image file\x1b\\"),
            "{held} bytes"
        );
        assert_eq!(h.store.image_count(), 0);
        assert_eq!(h.store.bytes(), 0, "the budget is whole again");
    }
}

// covers: image.kitty-animation
#[test]
fn a_frames_buffered_payload_is_charged_to_the_frame_allowance_and_evicts_nothing() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    // a budget of image 1, 256 by 256, image 2, 64 by 64, and 1 KiB more
    let limit = (256 * 256 * 4 + IMAGE_OVERHEAD) + (64 * 64 * 4 + IMAGE_OVERHEAD) + 1024;
    h.store.set_limit(limit);
    send(
        &mut k,
        &mut h,
        &format!("a=t,s=256,v=256,i=1;{}", red(256 * 256)),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=t,s=64,v=64,i=2;{}", red(64 * 64)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\\x1b_Gi=2;OK\x1b\\");
    let budget = h.store.bytes();
    // a zlib frame sent directly: its 64 KiB payload is kept for the helper
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,o=z,f=32,s=256,v=256;{}", b64(&[0x5a; 64 << 10])),
    );
    assert_eq!(h.store.image_count(), 2, "the payload evicted no image");
    assert_eq!(h.store.bytes(), budget, "nor took any of the budget");
    assert!(h.store.frame_bytes() >= 64 << 10, "the allowance holds it");
    assert_eq!(h.requests.len(), 1);
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 256,
            height: 256,
            data: [0, 0, 0xff, 0xff].repeat(256 * 256),
        }),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    let key = h.store.find_image(1, 0).unwrap();
    assert_eq!(h.store.animation(key).unwrap().frames, 2);
    assert_eq!(h.store.image_count(), 2);
    assert_eq!(
        h.store.frame_bytes(),
        256 * 256 * 4 + crate::FRAME_OVERHEAD,
        "the payload's bytes went back when the helper answered"
    );
}

/// Frame `n` of image 1's pixels.
fn frame_of(h: &mut Host, n: u32) -> Vec<u32> {
    let key = key(h);
    h.store
        .frame_pixels(key, n)
        .unwrap()
        .pixels
        .as_slice()
        .to_vec()
}

// covers: image.kitty-animation
#[test]
fn a_composition_lays_frame_r_onto_frame_c_and_gives_c_a_new_pixel_revision() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=4,v=4,i=1;{}", red(16)));
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1;{}", rgba([0, 0, 0xff, 0xff], 16)),
    );
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1;{}", rgba([0, 0xff, 0, 0xff], 16)),
    );
    let _ = h.take();
    let (two, three) = (frame_of(&mut h, 2), frame_of(&mut h, 3));
    let key = key(&mut h);
    let revision = |h: &Host, n| h.store.frame_pixels(key, n).unwrap().revision;
    let (r2, r3) = (revision(&h, 2), revision(&h, 3));
    // r is the source and c the destination
    send(&mut k, &mut h, "a=c,i=1,r=2,c=3,w=2,h=1,C=1");
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(frame_of(&mut h, 2), two, "the source keeps its pixels");
    let mut want = three.clone();
    want[..2].copy_from_slice(&two[..2]);
    assert_eq!(frame_of(&mut h, 3), want);
    assert_eq!(revision(&h, 2), r2);
    assert!(
        revision(&h, 3) > r3,
        "the destination's pixel revision moves"
    );
}

// covers: image.kitty-animation
#[test]
fn an_edit_of_the_root_frame_gives_it_a_new_pixel_revision() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=4,v=4,i=1;{}", red(16)));
    let key = key(&mut h);
    let before = h.store.frame_pixels(key, 1).unwrap();
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,r=1,s=1,v=1;{}", rgba([0, 0, 0xff, 0xff], 1)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\");
    let after = h.store.frame_pixels(key, 1).unwrap();
    assert_eq!(after.pixels.as_slice()[0], 0xff00_00ff);
    assert!(after.revision > before.revision);
    // a composition onto the root moves it again
    send(&mut k, &mut h, &format!("a=f,i=1;{}", red(16)));
    send(&mut k, &mut h, "a=c,i=1,r=2,c=1,w=1,h=1,C=1");
    let again = h.store.frame_pixels(key, 1).unwrap();
    assert_eq!(again.pixels.as_slice()[0], 0xffff_0000);
    assert!(again.revision > after.revision);
}

// covers: image.kitty-animation
#[test]
fn an_animation_command_moves_its_images_use_clock() {
    // three pictures of 256 by 256, under a budget whose allowance, 640 KiB,
    // holds two whole frames of them
    let (mut k, mut h) = (Kitty::default(), Host::new());
    for id in 1..=3 {
        send(
            &mut k,
            &mut h,
            &format!("a=t,s=256,v=256,i={id};{}", red(256 * 256)),
        );
    }
    h.store.set_limit(128 << 10);
    let whole = rgba([0, 0, 0xff, 0xff], 256 * 256);
    send(&mut k, &mut h, &format!("a=f,i=1;{whole}"));
    send(&mut k, &mut h, &format!("a=f,i=2;{whole}"));
    // a control names image 1, so image 2 is now used least recently, and
    // its frames go for image 3's
    send(&mut k, &mut h, "a=a,i=1,s=1");
    send(&mut k, &mut h, &format!("a=f,i=3;{whole}"));
    let frames = |h: &mut Host, id| {
        let key = h.store.find_image(id, 0).unwrap();
        h.store.animation(key).unwrap().frames
    };
    assert_eq!(
        (frames(&mut h, 1), frames(&mut h, 2), frames(&mut h, 3)),
        (2, 1, 2)
    );
}

// covers: image.kitty-animation
#[test]
fn z_on_an_edit_sets_the_gap_of_the_frame_it_edits() {
    let (mut k, mut h) = animated();
    send(&mut k, &mut h, &format!("a=f,i=1,s=1,v=1;{}", red(1)));
    let key = key(&mut h);
    assert_eq!(h.store.frame_gap(key, 2), Some(40));
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,r=2,z=77,s=1,v=1;{}", red(1)),
    );
    assert_eq!(h.store.frame_gap(key, 2), Some(77));
    // z=0 keeps the gap
    send(&mut k, &mut h, &format!("a=f,i=1,r=2,s=1,v=1;{}", red(1)));
    assert_eq!(h.store.frame_gap(key, 2), Some(77));
}

// covers: image.kitty-animation
#[test]
fn z_on_an_edit_of_the_root_frame_sets_the_roots_gap() {
    let (mut k, mut h) = animated();
    let key = key(&mut h);
    assert_eq!(h.store.frame_gap(key, 1), Some(0));
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=1,r=1,z=55,s=1,v=1;{}", red(1)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(h.store.frame_gap(key, 1), Some(55));
    // z=0 keeps it
    send(&mut k, &mut h, &format!("a=f,i=1,r=1,s=1,v=1;{}", red(1)));
    assert_eq!(h.store.frame_gap(key, 1), Some(55));
}

// covers: image.kitty-animation
#[test]
fn a_frames_refusal_names_the_image_as_its_success_does() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,I=5;{}", red(1)));
    send(&mut k, &mut h, &format!("a=f,I=5,s=1,v=1;{}", red(1)));
    assert_eq!(h.take(), "\x1b_Gi=1,I=5;OK\x1b\\\x1b_Gi=1,I=5;OK\x1b\\");
    let einval = "\x1b_Gi=1,I=5;EINVAL:invalid argument\x1b\\";
    let enoent = "\x1b_Gi=1,I=5;ENOENT:image not found\x1b\\";
    // refused before the data: a rectangle out of the image, a missing frame
    send(&mut k, &mut h, &format!("a=f,I=5,x=999,s=1,v=1;{}", red(1)));
    assert_eq!(h.take(), einval);
    send(&mut k, &mut h, &format!("a=f,I=5,c=9,s=1,v=1;{}", red(1)));
    assert_eq!(h.take(), enoent);
    // refused at the end: data short of its rectangle
    send(&mut k, &mut h, &format!("a=f,I=5,s=1,v=2;{}", red(1)));
    assert_eq!(h.take(), einval);
    // refused with a named medium, before the request and at the answer
    send(
        &mut k,
        &mut h,
        &format!("a=f,I=5,t=f,f=100,x=9;{}", b64(b"/p")),
    );
    assert_eq!(h.take(), einval);
    send(&mut k, &mut h, &format!("a=f,I=5,t=f,f=100;{}", b64(b"/p")));
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 2,
            height: 1,
            data: vec![0; 8],
        }),
    );
    assert_eq!(h.take(), einval);
    send(&mut k, &mut h, &format!("a=f,I=5,t=f,f=100;{}", b64(b"/p")));
    k.decoded(&mut h, Reply::Corrupt);
    assert_eq!(h.take(), einval);
}
