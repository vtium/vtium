//! Uploads in chunks, and the commands that arrive while one is open,
//! each test written from ADR-1100 D16's rules.

use super::*;

/// Opens an upload of image 1, a 2×1 RGB picture, its first chunk one
/// red pixel.
const OPEN1: &str = "a=t,f=24,s=2,v=1,i=1,m=1;/wAA";
/// A last chunk holding one green pixel.
const LAST1: &str = "m=0;AP8A";
/// Image 2, one blue pixel, sent whole.
const IMG2: &str = "a=t,f=24,s=1,v=1,i=2;AAD/";

fn put(n: u32) -> String {
    format!("a=p,i={n},C=1")
}

fn ok(n: u32) -> String {
    format!("\x1b_Gi={n};OK\x1b\\")
}

fn noent(n: u32) -> String {
    format!(
        "\x1b_Gi={n};ENOENT:Put command refers to non-existent image with id: {n} and number: 0\x1b\\"
    )
}

/// The size of the image of id `id` in the buffer in use.
fn size(h: &mut Host, id: u32) -> Option<(u32, u32)> {
    let key = h.store.find_image(id, 0)?;
    h.store.image_size(key)
}

// covers: image.kitty
#[test]
fn an_upload_in_chunks_answers_once_at_its_last_chunk() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    assert_eq!(
        h.take(),
        "",
        "a chunk that keeps the upload open is not answered"
    );
    assert_eq!(
        size(&mut h, 1),
        None,
        "nothing is stored while the upload is open"
    );
    send(&mut k, &mut h, LAST1);
    assert_eq!(h.take(), ok(1));
    assert_eq!(size(&mut h, 1), Some((2, 1)));
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), ok(1));
    // red, then green, from the two chunks
    let red = opaque([0xff, 0, 0]);
    let green = opaque([0, 0xff, 0]);
    assert_eq!(h.top_pixels(), vec![red, green]);
}

// covers: image.kitty
#[test]
fn each_chunk_decodes_its_own_base64_unpadded() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    // /wA gives two bytes and AAP8A three, its last character none
    send(&mut k, &mut h, "a=t,f=24,s=2,v=1,i=1,m=1;/wA");
    send(&mut k, &mut h, "m=0;AAP8A");
    assert_eq!(
        h.take(),
        "\x1b_Gi=1;ENODATA:Insufficient image data: 5 < 6\x1b\\"
    );
}

// covers: image.kitty
#[test]
fn a_direct_transmission_while_an_upload_is_open_is_its_next_chunk() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    // a whole picture of another image ends image 1's upload with its data
    send(&mut k, &mut h, IMG2);
    assert_eq!(h.take(), ok(1));
    // the last chunk is now a command of its own, with no id
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    send(&mut k, &mut h, &put(2));
    assert_eq!(h.take(), format!("{}{}", ok(1), noent(2)));
    // a chunk's other keys are ignored: i, f, s
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, "m=0,i=7,f=32,s=9;AP8A");
    send(&mut k, &mut h, &put(7));
    assert_eq!(h.take(), format!("{}{}", ok(1), noent(7)));
    assert_eq!(size(&mut h, 1), Some((2, 1)));
}

// covers: image.kitty
#[test]
fn an_uploads_q_is_its_first_chunks_until_a_later_one_other_than_0() {
    let first = "a=t,f=24,s=3,v=1,i=1,q=2,m=1;/wAA";
    let short = "\x1b_Gi=1;ENODATA:Insufficient image data: 6 < 9\x1b\\";
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, first);
    send(&mut k, &mut h, "m=0,q=1;AP8A");
    assert_eq!(h.take(), short, "the last q other than 0 wins");
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, first);
    send(&mut k, &mut h, "m=0,q=0;AP8A");
    assert_eq!(h.take(), "", "q=0 leaves the first chunk's q=2");
    // q on a middle chunk silences the OK
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=24,s=3,v=1,i=1,m=1;/wAA");
    send(&mut k, &mut h, "m=1,q=1;AP8A");
    send(&mut k, &mut h, "m=0;AAD/");
    assert_eq!(h.take(), "");
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), ok(1));
}

// covers: image.kitty
#[test]
fn a_first_chunk_refused_by_its_keys_is_answered_at_once_and_opens_nothing() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=7,s=2,v=1,i=1,m=1;/wAA");
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:Unknown image format: 7\x1b\\");
    // the next commands are their own: f=32 with no size, refused by keys,
    // unanswered without an id, then answered for image 4
    send(&mut k, &mut h, "m=1;AP8A");
    send(&mut k, &mut h, "m=0,i=4;AAD/");
    assert_eq!(
        h.take(),
        "\x1b_Gi=4;EINVAL:Zero width/height not allowed\x1b\\"
    );
    send(&mut k, &mut h, IMG2);
    send(&mut k, &mut h, &put(1));
    send(&mut k, &mut h, &put(2));
    assert_eq!(h.take(), format!("{}{}{}", ok(2), noent(1), ok(2)));
    assert_eq!(h.store.bytes(), 4 + 512, "only image 2 holds the budget");
}

// covers: image.kitty
#[test]
fn an_a_capital_t_upload_places_at_the_cursor_of_its_last_chunk_with_its_first_keys() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=T,f=24,s=2,v=1,i=1,c=4,r=2,C=1,m=1;/wAA");
    assert_eq!(h.boxes(), vec![], "nothing is placed before the last chunk");
    h.cursor = (5, 2);
    send(&mut k, &mut h, "C=0,c=8,m=0;AP8A");
    assert_eq!(h.take(), ok(1));
    assert_eq!(h.boxes(), vec![(5, 2, 4, 2)]);
    assert_eq!(h.cursor, (5, 2), "the first chunk's C=1 holds");
}

// covers: image.kitty
#[test]
fn a_transmission_from_a_named_medium_ends_the_upload_and_runs_on_its_own() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.helper = true;
    send(&mut k, &mut h, OPEN1);
    let held = h.store.bytes();
    assert_eq!(held, 8 + 512, "the upload reserves its whole cost");
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=24,s=1,v=1,i=3;{}", b64(b"/tmp/white")),
    );
    assert_eq!(h.requests.len(), 1, "the file goes to the helper");
    assert_eq!(h.store.bytes(), 0, "the upload gave its reservation back");
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 1,
            height: 1,
            data: vec![0xff; 4],
        }),
    );
    assert_eq!(h.take(), ok(3));
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), noent(1));
    // m=1 on a named medium opens nothing
    send(
        &mut k,
        &mut h,
        &format!("a=t,t=f,f=24,s=1,v=1,i=3,m=1;{}", b64(b"/tmp/white")),
    );
    k.decoded(
        &mut h,
        Reply::Ok(Payload::Rgba {
            width: 1,
            height: 1,
            data: vec![0xff; 4],
        }),
    );
    send(&mut k, &mut h, "m=0;");
    assert_eq!(h.take(), ok(3));
}

// covers: image.kitty
#[test]
fn a_delete_ends_the_upload_with_no_answer_then_deletes() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, "a=d,d=i,i=99");
    assert_eq!(h.store.bytes(), 0, "the upload gave its reservation back");
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), noent(1));
    // the chunk after a delete is a whole command of its own
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, "a=d");
    send(&mut k, &mut h, "f=24,s=2,v=1,i=1,m=0;AP8A");
    assert_eq!(
        h.take(),
        "\x1b_Gi=1;ENODATA:Insufficient image data: 3 < 6\x1b\\"
    );
}

// covers: image.kitty, image.kitty-animation
#[test]
fn a_put_a_composition_and_a_control_leave_the_upload_open() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, IMG2);
    send(&mut k, &mut h, "a=f,i=2,f=24,s=1,v=1;////");
    assert_eq!(h.take(), format!("{}\x1b_Gi=2,r=2;OK\x1b\\", ok(2)));
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, &put(2));
    send(&mut k, &mut h, "a=c,i=2,r=2,c=1");
    send(&mut k, &mut h, "a=a,i=2,c=2");
    send(&mut k, &mut h, "a=a,i=42,c=2");
    send(&mut k, &mut h, LAST1);
    assert_eq!(
        h.take(),
        format!(
            "{}{}\x1b_Gi=42;ENOENT:Animation command refers to non-existent image with id: 42 and number: 0\x1b\\{}",
            ok(2),
            ok(2),
            ok(1)
        )
    );
}

// covers: image.kitty
#[test]
fn a_query_while_an_upload_is_open_is_answered_from_its_own_keys() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, "a=q,i=5,f=24,s=1,v=1;AAAA");
    // its m is ignored, so it is answered whole
    send(&mut k, &mut h, "a=q,i=6,f=24,s=2,v=1,m=1;AAAA");
    assert_eq!(
        h.take(),
        format!(
            "{}\x1b_Gi=6;ENODATA:Insufficient image data: 3 < 6\x1b\\",
            ok(5)
        )
    );
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    send(&mut k, &mut h, &put(5));
    assert_eq!(h.take(), format!("{}{}{}", ok(1), ok(1), noent(5)));
}

// covers: image.kitty
#[test]
fn a_query_in_chunks_is_answered_at_its_last_chunk_and_stores_nothing() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=q,i=5,f=24,s=2,v=1,m=1;/wAA");
    assert_eq!(h.take(), "");
    send(&mut k, &mut h, "m=0;AP8A");
    send(&mut k, &mut h, &put(5));
    assert_eq!(h.take(), format!("{}{}", ok(5), noent(5)));
    assert_eq!(h.store.bytes(), 0);
}

// covers: image.kitty, image.kitty-animation
#[test]
fn a_frame_while_a_pictures_upload_is_open_is_its_next_chunk() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, IMG2);
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, "a=f,i=2,f=24,s=1,v=1;////");
    assert_eq!(h.take(), format!("{}{}", ok(2), ok(1)));
    let key = h.store.find_image(2, 0).unwrap();
    assert_eq!(
        h.store.animation(key).map_or(1, |a| a.frames),
        1,
        "image 2 gained no frame"
    );
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), ok(1));
}

// covers: image.kitty-animation
#[test]
fn a_frame_in_chunks_is_answered_once_with_its_frame_number() {
    let picture = "a=t,f=24,s=2,v=1,i=2;AAD/AAD/";
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, picture);
    send(&mut k, &mut h, "a=f,i=2,f=24,s=2,v=1,m=1;/wAA");
    assert_eq!(h.take(), ok(2));
    send(&mut k, &mut h, "a=f,m=0;AP8A");
    assert_eq!(h.take(), "\x1b_Gi=2,r=2;OK\x1b\\");
    // short data is answered at the last chunk, with the frame it would be
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, picture);
    send(&mut k, &mut h, "a=f,i=2,f=24,s=3,v=1,m=1;/wAA");
    send(&mut k, &mut h, "a=f,m=0;AP8A");
    assert_eq!(
        h.take(),
        format!(
            "{}\x1b_Gi=2,r=2;ENODATA:Insufficient image data: 6 < 9\x1b\\",
            ok(2)
        )
    );
    assert_eq!(
        h.store.frame_bytes(),
        0,
        "the frame's reservation went back"
    );
}

// covers: image.kitty
#[test]
fn each_buffer_holds_its_own_upload() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, OPEN1);
    h.store.set_alternate(true);
    // on the alternate screen the last chunk is a command of its own
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, "a=t,f=24,s=1,v=1,i=2,m=1;AAD/");
    send(&mut k, &mut h, "m=0;");
    assert_eq!(h.take(), ok(2));
    assert_eq!(size(&mut h, 2), Some((1, 1)));
    h.store.set_alternate(false);
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    send(&mut k, &mut h, &put(2));
    assert_eq!(h.take(), format!("{}{}{}", ok(1), ok(1), noent(2)));
}

// covers: image.kitty
#[test]
fn the_old_image_stays_until_the_upload_is_accepted() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=24,s=1,v=1,i=1;AAD/");
    send(&mut k, &mut h, OPEN1);
    // a put during the upload finds the old 1×1 picture
    send(&mut k, &mut h, &put(1));
    assert_eq!(size(&mut h, 1), Some((1, 1)));
    assert_eq!(h.boxes().len(), 1);
    send(&mut k, &mut h, LAST1);
    assert_eq!(h.take(), format!("{}{}{}", ok(1), ok(1), ok(1)));
    // the new picture replaces it and its placement
    assert_eq!(size(&mut h, 1), Some((2, 1)));
    assert_eq!(h.boxes(), vec![]);
    // a refused upload leaves the old picture
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=24,s=1,v=1,i=1;AAD/");
    send(&mut k, &mut h, "a=t,f=24,s=3,v=1,i=1,m=1;/wAA");
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(
        h.take(),
        format!(
            "{}\x1b_Gi=1;ENODATA:Insufficient image data: 6 < 9\x1b\\{}",
            ok(1),
            ok(1)
        )
    );
    assert_eq!(size(&mut h, 1), Some((1, 1)));
    assert_eq!(h.store.bytes(), 4 + 512);
}

// covers: image.kitty
#[test]
fn an_uploads_reservation_counts_beside_the_old_image() {
    // a 1×1 picture costs 516 bytes and a 2×1 one 520: 1,036 together
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_limit(1035);
    send(&mut k, &mut h, "a=t,f=24,s=1,v=1,i=1;AAD/");
    send(&mut k, &mut h, OPEN1);
    assert_eq!(
        h.take(),
        format!("{}\x1b_Gi=1;ENOSPC:image storage is full\x1b\\", ok(1))
    );
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), ok(1), "the old picture stays");
    assert_eq!(size(&mut h, 1), Some((1, 1)));
    // the same upload fits alone
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_limit(1035);
    send(&mut k, &mut h, OPEN1);
    send(&mut k, &mut h, LAST1);
    assert_eq!(h.take(), ok(1));
}

// covers: image.kitty
#[test]
fn an_uploads_data_is_capped_at_its_reservation() {
    // a 1×1 RGB upload reserves 4 + 512 = 516 bytes; it has taken 3
    let first = "a=t,f=24,s=1,v=1,i=1,m=1;/wAA";
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, first);
    // a frame is the next chunk, and its 516 bytes pass the cap
    send(
        &mut k,
        &mut h,
        &format!("a=f,i=2,f=24,s=1,v=1,m=1;{}", "A".repeat(688)),
    );
    assert_eq!(h.take(), "\x1b_Gi=1;EINVAL:invalid argument\x1b\\");
    assert_eq!(h.store.bytes(), 0, "the reservation went back");
    send(&mut k, &mut h, LAST1);
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), noent(1));
    // 3 + 513 bytes are exactly the cap, and the bytes past 3 are ignored
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, first);
    send(&mut k, &mut h, &format!("m=0;{}", "A".repeat(684)));
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), format!("{}{}", ok(1), ok(1)));
}

// covers: image.kitty
#[test]
fn reset_withdraw_and_a_cancelled_chunk_end_the_upload_with_no_answer() {
    let ends: [fn(&mut Kitty, &mut Host); 3] = [
        |k, h| k.reset(h),
        |k, h| k.withdraw(h),
        |k, h| {
            k.start(h, Intro::String);
            k.feed(h, Chunk::Bytes(b"m=1;AP8"));
            k.cancel(h);
        },
    ];
    for (n, end) in ends.iter().enumerate() {
        let (mut k, mut h) = (Kitty::default(), Host::new());
        send(&mut k, &mut h, OPEN1);
        end(&mut k, &mut h);
        assert_eq!(h.store.bytes(), 0, "end {n} gives the reservation back");
        send(&mut k, &mut h, IMG2);
        send(&mut k, &mut h, LAST1);
        send(&mut k, &mut h, &put(1));
        assert_eq!(h.take(), format!("{}{}", ok(2), noent(1)), "end {n}");
    }
}

// covers: image.kitty
#[test]
fn a_palette_change_between_chunks_leaves_the_picture_as_its_first_chunk_found_it() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=8,s=2,v=1,i=1,m=1;Bw==");
    let first = h.colors.clone().unwrap();
    h.colors.as_mut().unwrap().table[7] = [1, 2, 3];
    h.colors.as_mut().unwrap().table[8] = [4, 5, 6];
    send(&mut k, &mut h, "m=0;CA==");
    send(&mut k, &mut h, &put(1));
    assert_eq!(h.take(), format!("{}{}", ok(1), ok(1)));
    assert_eq!(
        h.top_pixels(),
        vec![opaque(first.table[7]), opaque(first.table[8])]
    );
}
