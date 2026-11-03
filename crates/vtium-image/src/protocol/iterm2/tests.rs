//! The iTerm2 handler's tests, through a stand-in for vtium-term's context.
//!
//! The stand-in holds a real store over a screen of fixed lines and the
//! scrollback above it, and does what vtium-term's context does with each
//! call: it runs a store edit over both buffers' rows and settles the store
//! after it, clamps the cursor to the screen, scrolls the region as the
//! screen does, and keeps the requests and the notes in order.

#![expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use vtium_parse::Chunk;

use super::Iterm2;
use crate::bounds::{IMAGE_OVERHEAD, ITERM2_FILE_MAX, ITERM2_PARTS_MAX};
use crate::decode::{Medium, Payload, Reply, Request};
use crate::handler::{Context, Handler, Intro, Limits, Margins, ReplyKind, SixelState, Switches};
use crate::image::LineKey;
use crate::store::{Edit, Rows, Screens, Store};

struct Host {
    store: Store,
    screen: Vec<LineKey>,
    /// The scrollback's lines, newest first.
    above: Vec<LineKey>,
    next_line: u64,
    cols: usize,
    cursor: (usize, usize),
    margins: Margins,
    helper: bool,
    requests: Vec<Request>,
    notes: Vec<String>,
    limits: Limits,
    /// What `sixel` reports: the handler reads none of it.
    sixel: SixelState,
}

impl Host {
    /// A 12 by 4 screen, its cell 8 by 16 pixels, a budget of 1 MiB, and a
    /// helper.
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
            helper: true,
            requests: Vec::new(),
            notes: Vec::new(),
            limits: Limits::default(),
            sixel: SixelState {
                display: false,
                private_registers: true,
                cursor_right: false,
                registers: 1024,
            },
        }
    }

    fn rows(&self) -> Rows<'_> {
        Rows {
            screen: &self.screen,
            above: &self.above,
        }
    }

    /// Each placement as (column, row, columns, rows, drawn width, drawn
    /// height, kind).
    fn boxes(&self) -> Vec<(u32, i64, u32, u32, u64, u64, &'static str)> {
        self.store
            .frame(self.rows(), self.cols)
            .iter()
            .map(|p| {
                (
                    p.col,
                    p.row,
                    p.cols,
                    p.rows,
                    p.drawn_width,
                    p.drawn_height,
                    p.kind,
                )
            })
            .collect()
    }

    /// The region scrolls up one row, as vtium-term scrolls it: the whole
    /// screen sends its top line into the scrollback, and a new line
    /// enters at the bottom.
    fn scroll_one(&mut self) {
        let (top, bottom) = (self.margins.top, self.margins.bottom);
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
        self.cursor = (col.min(self.cols - 1), row.min(self.screen.len() - 1));
    }

    fn margins(&self) -> Margins {
        self.margins
    }

    fn index(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor.1 == self.margins.bottom {
                self.scroll_one();
            } else {
                self.cursor.1 = (self.cursor.1 + 1).min(self.screen.len() - 1);
            }
        }
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            self.scroll_one();
        }
    }

    fn sixel(&self) -> SixelState {
        self.sixel
    }

    fn reply(&mut self, _: ReplyKind, _: &[u8]) {
        panic!("the iTerm2 handler never replies");
    }

    fn switches(&self) -> Switches {
        Switches {
            sixel: true,
            kitty: false,
            iterm2: true,
        }
    }

    fn limits(&self) -> Limits {
        self.limits
    }

    fn add_work(&mut self, _: u64) {}

    fn note(&mut self, why: &str) {
        self.notes.push(why.to_owned());
    }
}

/// One OSC 1337 string, `text` being what follows `1337;`, as the router
/// hands it over.
fn send(i: &mut Iterm2, h: &mut Host, text: &str) {
    i.start(h, Intro::String);
    i.feed(h, Chunk::Bytes(text.as_bytes()));
    i.finish(h);
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

/// A file the tests send: its bytes are never read as a picture, since the
/// stand-in answers for the helper.
const FILE: &[u8] = b"\x89PNG\r\n\x1a\n not a real picture, 37 bytes";

/// The helper's answer: a `w` by `h` picture of opaque white.
fn picture(w: u32, h: u32) -> Reply {
    Reply::Ok(Payload::Rgba {
        width: w,
        height: h,
        data: vec![0xff; (w * h * 4) as usize],
    })
}

/// The request out, taken.
fn taken(h: &mut Host) -> Request {
    assert_eq!(h.requests.len(), 1, "one request out");
    h.requests.pop().unwrap()
}

// covers: image.iterm2
#[test]
fn a_picture_in_one_string_goes_to_the_helper_and_is_placed_under_the_sixel_tag() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    h.cursor = (2, 1);
    send(
        &mut i,
        &mut h,
        &format!("File=name=YS5wbmc=;size=37;inline=1:{}", b64(FILE)),
    );
    let request = taken(&mut h);
    assert_eq!(request.medium, Medium::Direct(FILE.to_vec()));
    assert!(request.any_image);
    assert!(!(request.png || request.inflate || request.exact || request.discard));
    assert_eq!(request.pixels, None);
    assert_eq!(request.largest, (10_000, 10_000));
    assert_eq!(request.largest_bytes, (1 << 20) - IMAGE_OVERHEAD);
    // the file's buffer is held under the budget until the answer comes
    assert!(h.store.bytes() >= 37);
    i.decoded(&mut h, picture(16, 24));
    // 16 by 24 pixels at its own size: 2 columns and 2 rows of 8 by 16
    assert_eq!(h.boxes(), [(2, 1, 2, 2, 16, 24, "sixel")]);
    assert_eq!(h.store.bytes(), 16 * 24 * 4 + IMAGE_OVERHEAD);
    // the cursor ends on the picture's last row, at its left column
    assert_eq!(h.cursor, (2, 2));
    assert!(h.notes.is_empty(), "{:?}", h.notes);
}

#[test]
fn a_string_without_inline_or_without_a_colon_is_dropped() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    let data = b64(FILE);
    for text in [
        format!("File=name=YS5wbmc=:{data}"),
        format!("File=inline=0:{data}"),
        format!("File=inline=1;{data}"),
        "File=".to_owned(),
        "MultipartFile=inline=0".to_owned(),
        format!("FilePart={data}"),
        "FileEnd".to_owned(),
        format!("Fil={data}"),
        "SetUserVar=a=Yg==".to_owned(),
    ] {
        send(&mut i, &mut h, &text);
    }
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
    assert_eq!(i, Iterm2::default());
}

#[test]
fn base64_that_does_not_decode_drops_the_picture() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, "File=inline=1:AAAA*AAA");
    send(&mut i, &mut h, "File=inline=1:A");
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
    assert_eq!(h.notes.len(), 2);
}

#[test]
fn without_a_helper_nothing_is_asked_and_nothing_held() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    h.helper = false;
    send(&mut i, &mut h, &format!("File=inline=1:{}", b64(FILE)));
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
}

#[test]
fn an_answer_other_than_a_picture_stores_nothing_and_gives_the_buffer_back() {
    for reply in [
        Reply::Corrupt,
        Reply::TooLarge,
        Reply::OverBudget,
        Reply::Failed,
        Reply::Ok(Payload::Discarded),
    ] {
        let (mut i, mut h) = (Iterm2::default(), Host::new());
        send(&mut i, &mut h, &format!("File=inline=1:{}", b64(FILE)));
        let _ = taken(&mut h);
        i.decoded(&mut h, reply.clone());
        assert_eq!(h.store.bytes(), 0, "{reply:?}");
        assert!(h.boxes().is_empty(), "{reply:?}");
        assert_eq!(i, Iterm2::default());
    }
}

#[test]
fn a_picture_the_budget_or_max_graphic_size_refuses_is_dropped() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, &format!("File=inline=1:{}", b64(FILE)));
    let _ = taken(&mut h);
    // lowered while the request was out
    h.limits.graphic = (8, 8);
    i.decoded(&mut h, picture(16, 8));
    assert!(h.boxes().is_empty());
    assert_eq!(h.store.bytes(), 0);
    h.limits = Limits::default();
    send(&mut i, &mut h, &format!("File=inline=1:{}", b64(FILE)));
    let _ = taken(&mut h);
    i.decoded(&mut h, picture(512, 512 + 1));
    assert!(h.boxes().is_empty(), "past the 1 MiB budget");
    assert_eq!(h.store.bytes(), 0);
}

// covers: image.iterm2
#[test]
fn width_and_height_size_the_picture_and_its_box() {
    // (keys, drawn size, box columns and rows) for a 64 by 32 picture at 8
    // by 16 pixels on a 12 by 4 screen
    let cases = [
        ("", (64, 32), (8, 2)),
        ("width=4", (32, 16), (4, 1)),
        ("height=1", (32, 16), (4, 1)),
        ("width=20px", (20, 10), (3, 1)),
        ("width=50%", (48, 24), (6, 2)),
        ("width=50%;height=1;preserveAspectRatio=0", (48, 16), (6, 1)),
        ("width=2;height=2", (16, 8), (2, 1)),
        (
            "width=auto;height=40px;preserveAspectRatio=0",
            (64, 40),
            (8, 3),
        ),
    ];
    for (keys, drawn, cells) in cases {
        let (mut i, mut h) = (Iterm2::default(), Host::new());
        send(
            &mut i,
            &mut h,
            &format!("File={keys};inline=1:{}", b64(FILE)),
        );
        let _ = taken(&mut h);
        i.decoded(&mut h, picture(64, 32));
        let b = h.boxes();
        assert_eq!(b.len(), 1, "{keys}");
        assert_eq!((b[0].4, b[0].5), drawn, "{keys}");
        assert_eq!((b[0].2, b[0].3), cells, "{keys}");
    }
}

#[test]
fn a_box_past_the_largest_is_dropped() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(
        &mut i,
        &mut h,
        &format!("File=width=10001;inline=1:{}", b64(FILE)),
    );
    let _ = taken(&mut h);
    i.decoded(&mut h, picture(8, 8));
    assert!(h.boxes().is_empty());
    // the freed pixels stay as the store's spare buffer, under the budget
    assert_eq!(h.store.image_count(), 0);
}

// covers: image.iterm2
#[test]
fn a_picture_taller_than_the_rows_below_scrolls_as_a_sixel_and_ignores_the_sixel_modes() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    // mode 80 and mode 8452 are sixel's: neither applies
    h.sixel.display = true;
    h.sixel.cursor_right = true;
    h.cursor = (3, 2);
    send(
        &mut i,
        &mut h,
        &format!("File=height=3;inline=1:{}", b64(FILE)),
    );
    let _ = taken(&mut h);
    i.decoded(&mut h, picture(8, 16));
    // three rows from row 2 of four: the region scrolls up one first
    assert_eq!(h.boxes(), [(3, 1, 3, 3, 24, 48, "sixel")]);
    assert_eq!(h.above.len(), 1);
    assert_eq!(h.cursor, (3, 3), "the last row, the left column");
}

// covers: image.iterm2
#[test]
fn a_transfer_in_parts_gathers_its_file_across_parts_of_any_length() {
    let file: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    let data = b64(&file);
    // parts cut at every length from 1 to 9, none a multiple of 4 needed
    for cut in 1..10 {
        let (mut i, mut h) = (Iterm2::default(), Host::new());
        send(
            &mut i,
            &mut h,
            "MultipartFile=name=eA==;size=1000;width=2;inline=1",
        );
        for part in data.as_bytes().chunks(cut) {
            send(
                &mut i,
                &mut h,
                &format!("FilePart={}", core::str::from_utf8(part).unwrap()),
            );
            assert!(h.requests.is_empty(), "nothing asked before FileEnd");
        }
        assert!(
            h.store.bytes() >= 1000,
            "the buffer is held under the budget"
        );
        send(&mut i, &mut h, "FileEnd");
        assert_eq!(
            taken(&mut h).medium,
            Medium::Direct(file.clone()),
            "cut {cut}"
        );
        i.decoded(&mut h, picture(32, 32));
        // width=2 from the MultipartFile= keys
        assert_eq!(h.boxes(), [(0, 0, 2, 1, 16, 16, "sixel")], "cut {cut}");
        assert_eq!(h.store.bytes(), 32 * 32 * 4 + IMAGE_OVERHEAD);
    }
}

/// A handler with a transfer open and one part of `FILE` received.
fn open_transfer() -> (Iterm2, Host) {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, "MultipartFile=inline=1");
    send(&mut i, &mut h, &format!("FilePart={}", b64(FILE)));
    assert!(h.store.bytes() > 0);
    (i, h)
}

#[test]
fn a_new_file_or_multipart_string_abandons_the_transfer_and_goes_on() {
    let (mut i, mut h) = open_transfer();
    send(&mut i, &mut h, &format!("File=inline=1:{}", b64(b"other")));
    assert_eq!(taken(&mut h).medium, Medium::Direct(b"other".to_vec()));
    i.decoded(&mut h, Reply::Corrupt);
    assert_eq!(h.store.bytes(), 0, "the transfer's buffer went back");
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty(), "no transfer to end");

    let (mut i, mut h) = open_transfer();
    send(&mut i, &mut h, "MultipartFile=inline=1");
    assert_eq!(h.store.bytes(), 0);
    send(&mut i, &mut h, &format!("FilePart={}", b64(b"new")));
    send(&mut i, &mut h, "FileEnd");
    assert_eq!(taken(&mut h).medium, Medium::Direct(b"new".to_vec()));

    // even a string that is itself dropped ends the transfer
    for text in [
        "File=inline=0:AAAA",
        "File=inline=1",
        "MultipartFile=inline=0",
    ] {
        let (mut i, mut h) = open_transfer();
        send(&mut i, &mut h, text);
        assert_eq!(h.store.bytes(), 0, "{text}");
        send(&mut i, &mut h, "FileEnd");
        assert!(h.requests.is_empty(), "{text}");
    }
}

#[test]
fn reset_and_cancel_abandon_the_transfer() {
    let (mut i, mut h) = open_transfer();
    i.reset(&mut h);
    assert_eq!(h.store.bytes(), 0);
    assert_eq!(i, Iterm2::default());
    let (mut i, mut h) = open_transfer();
    i.cancel(&mut h);
    assert_eq!(h.store.bytes(), 0);
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty());
}

#[test]
fn a_part_that_does_not_decode_abandons_the_transfer() {
    let (mut i, mut h) = open_transfer();
    send(&mut i, &mut h, "FilePart=AA!A");
    assert_eq!(h.store.bytes(), 0);
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty());
    // a lone character left over at the end is no whole byte
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, "MultipartFile=inline=1");
    send(&mut i, &mut h, "FilePart=AAAAA");
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
}

#[test]
fn a_transfer_past_its_parts_is_abandoned() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, "MultipartFile=inline=1");
    for _ in 0..ITERM2_PARTS_MAX {
        send(&mut i, &mut h, "FilePart=");
    }
    assert!(i.transfer.is_some(), "the bound itself is taken");
    send(&mut i, &mut h, "FilePart=");
    assert_eq!(i.transfer, None);
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty());
}

#[test]
fn a_transfer_past_its_file_bound_is_abandoned() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    h.store.set_limit(64 << 20);
    send(&mut i, &mut h, "MultipartFile=inline=1");
    // three bytes a group: ITERM2_FILE_MAX is not a multiple of 3, so the
    // last whole group that fits leaves one byte of room
    let groups = (ITERM2_FILE_MAX / 3) as usize;
    send(
        &mut i,
        &mut h,
        &format!("FilePart={}", "AAAA".repeat(groups)),
    );
    assert!(i.transfer.is_some());
    send(&mut i, &mut h, "FilePart=AA==");
    assert!(i.transfer.is_some(), "one byte more reaches the bound");
    send(&mut i, &mut h, "FilePart=AA==");
    assert_eq!(i.transfer, None, "and one past it is refused");
    assert_eq!(h.store.bytes(), 0);
}

#[test]
fn a_transfer_the_budget_refuses_is_abandoned() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    h.store.set_limit(10_000);
    send(&mut i, &mut h, "MultipartFile=inline=1");
    send(&mut i, &mut h, &format!("FilePart={}", "AAAA".repeat(1000)));
    assert!(i.transfer.is_some(), "3,000 bytes fit");
    send(&mut i, &mut h, &format!("FilePart={}", "AAAA".repeat(2000)));
    assert_eq!(i.transfer, None, "9,000 do not");
    assert_eq!(h.store.bytes(), 0);
}

#[test]
fn text_between_the_parts_leaves_the_transfer_and_a_stray_part_is_ignored() {
    let (mut i, mut h) = (Iterm2::default(), Host::new());
    send(&mut i, &mut h, &format!("FilePart={}", b64(FILE)));
    send(&mut i, &mut h, "FileEnd");
    assert!(h.requests.is_empty());
    assert_eq!(h.store.bytes(), 0);
    // another OSC 1337 string the handler does not take
    let (mut i, mut h) = open_transfer();
    send(&mut i, &mut h, "SetUserVar=a=Yg==");
    send(&mut i, &mut h, "FileEnd");
    assert_eq!(taken(&mut h).medium, Medium::Direct(FILE.to_vec()));
}
