//! The kitty handler's tests, through a stand-in for vtium-term's context.
//!
//! The stand-in holds a real store over a screen of fixed lines, and does
//! what vtium-term's context does with each call: it runs a store edit
//! over both buffers' rows and settles the store after it, clamps the
//! cursor to the screen, and keeps replies and requests in order. It does
//! not scroll: an index past the bottom margin fails the test, so the
//! cases that scroll are tier 1's, through the real screen.

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
use crate::decode::{Medium, Payload, Reply, Request};
use crate::handler::{Context, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Switches};
use crate::image::LineKey;
use crate::store::{Edit, Rows, Screens, Store};

struct Host {
    store: Store,
    screen: Vec<LineKey>,
    cols: usize,
    cursor: (usize, usize),
    margins: Margins,
    helper: bool,
    replies: Vec<u8>,
    requests: Vec<Request>,
    limits: Limits,
    work: u64,
}

impl Host {
    /// A 12 by 4 screen, its cell 8 by 16 pixels, a budget of 1 MiB.
    fn new() -> Self {
        let mut store = Store::new();
        store.set_limit(1 << 20);
        Self {
            store,
            screen: (0..4).map(LineKey::new).collect(),
            cols: 12,
            cursor: (0, 0),
            margins: Margins { top: 0, bottom: 3 },
            helper: false,
            replies: Vec::new(),
            requests: Vec::new(),
            limits: Limits::default(),
            work: 0,
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
            above: &[],
        };
        self.store
            .frame(view, self.cols)
            .iter()
            .map(|p| (p.col, p.row, p.cols, p.rows))
            .collect()
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
                above: &[],
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
        let (col, row) = self.cursor;
        let to = row + n;
        assert!(
            to <= self.margins.bottom,
            "the stand-in screen does not scroll"
        );
        self.cursor = (col, to);
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

#[test]
fn the_later_parts_of_the_protocol_are_refused_for_now() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, &format!("a=t,s=1,v=1,i=1;{}", red(1)));
    let _ = h.take();
    for keys in [
        "a=p,i=1,U=1",
        "a=p,i=1,P=1",
        "a=p,i=1,Q=0",
        "a=p,i=1,H=1",
        "a=a,i=1",
        "a=c,i=1",
    ] {
        send(&mut k, &mut h, keys);
        assert_eq!(
            h.take(),
            "\x1b_Gi=1;EINVAL:invalid argument\x1b\\",
            "{keys}"
        );
    }
    send(&mut k, &mut h, &format!("a=f,i=1,s=1,v=1;{}", red(1)));
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    // a delete of animation frames deletes nothing
    send(&mut k, &mut h, "a=p,i=1,C=1");
    send(&mut k, &mut h, "a=d,d=F,i=1");
    assert_eq!(h.boxes().len(), 1);
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
    k.decoded(&mut h, Reply::Ok(Payload::Bytes(vec![9, 8, 7])));
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
    k.decoded(&mut h, Reply::Ok(Payload::Bytes(Vec::new())));
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
    assert_eq!(h.store.bytes(), 400 + IMAGE_OVERHEAD);
}
