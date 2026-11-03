//! The frame code's tests: the allowance, D1's rules, composition, the
//! cache, the controls and the pixel identity a snapshot carries
//! (ADR-1102).

#![expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]

use super::*;
use crate::image::NewImage;
use crate::rules::Protocol;
use crate::store::Screens;

/// A store with a budget of `limit` bytes, so a frame allowance of five
/// times that.
fn store(limit: u64) -> Store {
    let mut s = Store::new();
    s.set_limit(limit);
    s
}

/// A kitty image of kitty id `id`, `w` by `h` pixels of `px`.
fn image(s: &mut Store, id: u32, w: u32, h: u32, px: u32) -> ImageKey {
    let new = NewImage {
        protocol: Protocol::KITTY,
        width: w,
        height: h,
        pixels: vec![px; (w * h) as usize],
        opaque: None,
        id,
        number: 0,
        transient: false,
    };
    s.add_image(new, None, Screens::default()).expect("stored")
}

/// A frame's data: `w` by `h` pixels of `px` at `x`, `y`.
fn data(x: u32, y: u32, w: u32, h: u32, px: u32) -> NewFrame {
    NewFrame {
        x,
        y,
        width: w,
        height: h,
        pixels: vec![px; (w * h) as usize],
        opaque: px >> 24 == 0xff,
        replace: false,
        base: 0,
        background: 0,
        edit: 0,
        gap: 0,
    }
}

/// Sends `new` to image `key` as the handler does: its data reserved
/// first, then stored.
fn send(s: &mut Store, key: ImageKey, new: NewFrame) -> Result<u32, Refusal> {
    let r = s.frame_reserve(key, layer_cost(new.width, new.height))?;
    s.add_frame(new, r)
}

fn anim(s: &Store, key: ImageKey) -> &Anim {
    s.any_image(key)
        .and_then(|i| i.anim.as_deref())
        .expect("animated")
}

/// How frame `n` is kept: "whole" or "change".
fn kind(s: &Store, key: ImageKey, n: u32) -> &'static str {
    match anim(s, key).frame(n).map(|f| &f.kind) {
        Some(Kind::Whole(_)) => "whole",
        Some(Kind::Change { .. }) => "change",
        None => "none",
    }
}

fn pixels(s: &Store, key: ImageKey, n: u32) -> Vec<u32> {
    s.frame_pixels(key, n)
        .expect("frame")
        .pixels
        .as_slice()
        .to_vec()
}

/// The bytes the allowance must hold: every animation's charge recomputed
/// from its frames.
fn charges(s: &Store) -> u64 {
    s.buffers
        .iter()
        .flat_map(|b| b.images.values())
        .filter_map(|i| {
            let a = i.anim.as_deref()?;
            Some(Anim::total(&a.shapes(Size::of(i)), Size::of(i)))
        })
        .sum()
}

const RED: u32 = 0xffff_0000;
const BLUE: u32 = 0xff00_00ff;
const HALF_GREEN: u32 = 0x8000_8000;

// covers: image.kitty-animation
#[test]
fn a_frame_is_reserved_of_the_allowance_before_it_is_allocated_and_never_of_the_budget() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 16, 16, RED);
    let budget = s.bytes();
    assert_eq!(s.frame_limit(), 5 << 20);
    let r = s.frame_reserve(key, layer_cost(16, 16)).expect("reserved");
    assert_eq!(
        s.frame_bytes(),
        16 * 16 * 4 + FRAME_OVERHEAD,
        "reserved first"
    );
    assert!(s.any_image(key).unwrap().hold > 0, "the image is held");
    assert_eq!(s.add_frame(data(0, 0, 16, 16, BLUE), r), Ok(2));
    assert_eq!(s.frame_bytes(), 16 * 16 * 4 + FRAME_OVERHEAD);
    assert_eq!(s.bytes(), budget, "frames never count toward the budget");
    assert_eq!(s.any_image(key).unwrap().hold, 0);
    // a budget of 0 gives an allowance of 0
    let mut s = store(0);
    assert_eq!(s.frame_limit(), 0);
    s.set_limit(1 << 20);
    let key = image(&mut s, 1, 1, 1, RED);
    s.set_limit(0);
    assert_eq!(s.frame_reserve(key, 1), Err(Refusal::NoSpace));
}

// covers: image.kitty-animation
#[test]
fn the_allowance_frees_other_animations_least_recently_used_first_and_never_its_own_or_a_root() {
    // 256 by 256: a whole frame is 256 KiB, and the allowance of a 128 KiB
    // budget, 640 KiB, holds two of them
    let mut s = store(128 << 10);
    s.set_limit(64 << 20);
    let a = image(&mut s, 1, 256, 256, RED);
    let b = image(&mut s, 2, 256, 256, RED);
    let c = image(&mut s, 3, 256, 256, RED);
    s.set_limit(128 << 10);
    assert_eq!(send(&mut s, a, data(0, 0, 256, 256, BLUE)), Ok(2));
    assert_eq!(send(&mut s, b, data(0, 0, 256, 256, BLUE)), Ok(2));
    // b was used after a, so a's frames go for c's
    s.touch(b);
    let before = s.animation(a).unwrap();
    assert_eq!(send(&mut s, c, data(0, 0, 256, 256, BLUE)), Ok(2));
    let after = s.animation(a).unwrap();
    assert_eq!(after.frames, 1, "a's frames went");
    assert_eq!((after.current, after.run), (1, Run::Stopped));
    assert_eq!(after.frame_stamp, before.frame_stamp + 1);
    assert_eq!(after.run_stamp, before.run_stamp + 1);
    assert_eq!(s.animation(b).unwrap().frames, 2, "b's stay");
    assert_eq!(pixels(&s, a, 1), vec![RED; 256 * 256], "a root frame stays");
    assert_eq!(s.frame_bytes(), charges(&s));
    // c's own frames are never freed for c: a second whole frame for c
    // frees b, and a third, which needs b's and c's room, is refused
    assert_eq!(send(&mut s, c, data(0, 0, 256, 256, BLUE)), Ok(3));
    assert_eq!(s.animation(b).unwrap().frames, 1);
    assert_eq!(
        send(&mut s, c, data(0, 0, 256, 256, BLUE)),
        Err(Refusal::NoSpace)
    );
    assert_eq!(s.animation(c).unwrap().frames, 3);
    assert_eq!(s.frame_bytes(), charges(&s));
}

// covers: image.kitty-animation
#[test]
fn a_frame_that_freeing_cannot_fit_is_enospc_and_frees_nothing() {
    let mut s = store(64 << 20);
    let a = image(&mut s, 1, 256, 256, RED);
    let b = image(&mut s, 2, 256, 256, RED);
    s.set_limit(128 << 10);
    assert_eq!(send(&mut s, a, data(0, 0, 256, 256, BLUE)), Ok(2));
    // the allowance is 640 KiB, a and b hold 256 KiB each, and 6 MiB does
    // not fit even with a's frames freed
    assert_eq!(send(&mut s, b, data(0, 0, 256, 256, BLUE)), Ok(2));
    let held = s.frame_bytes();
    let r = s.frame_reserve(b, 6 << 20);
    assert_eq!(r, Err(Refusal::NoSpace));
    assert_eq!(s.animation(a).unwrap().frames, 2, "nothing was freed");
    assert_eq!(s.frame_bytes(), held);
}

// covers: image.kitty-animation
#[test]
fn an_animation_holds_at_most_frames_max_frames() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 1, 1, RED);
    let max = u32::try_from(FRAMES_MAX).unwrap();
    for n in 2..=max {
        assert_eq!(send(&mut s, key, data(0, 0, 1, 1, BLUE)), Ok(n));
    }
    assert_eq!(s.animation(key).unwrap().frames, max);
    let held = s.frame_bytes();
    assert_eq!(
        send(&mut s, key, data(0, 0, 1, 1, BLUE)),
        Err(Refusal::NoSpace)
    );
    assert_eq!(
        s.frame_bytes(),
        held,
        "the refused frame gave its data back"
    );
    // an edit adds no frame
    let mut edit = data(0, 0, 1, 1, RED);
    edit.edit = 7;
    assert_eq!(send(&mut s, key, edit), Ok(7));
}

// covers: image.kitty-animation
#[test]
fn a_small_picture_is_composed_when_its_frame_arrives() {
    // 128 by 128 is 64 KiB, the bound itself; 129 by 128 is past it
    let mut s = store(1 << 20);
    let small = image(&mut s, 1, 128, 128, RED);
    let large = image(&mut s, 2, 129, 128, RED);
    send(&mut s, small, data(0, 0, 4, 4, BLUE)).unwrap();
    send(&mut s, large, data(0, 0, 4, 4, BLUE)).unwrap();
    assert_eq!(kind(&s, small, 2), "whole");
    assert_eq!(kind(&s, large, 2), "change");
}

// covers: image.kitty-animation
#[test]
fn a_change_covering_half_the_picture_is_composed_when_it_arrives() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    // exactly half the area, then one row short of it
    send(&mut s, key, data(0, 0, 256, 128, BLUE)).unwrap();
    send(&mut s, key, data(0, 0, 256, 127, BLUE)).unwrap();
    assert_eq!(kind(&s, key, 2), "whole");
    assert_eq!(kind(&s, key, 3), "change");
    // an edit that brings frame 3's layers to half the area composes it
    let mut edit = data(0, 200, 256, 1, BLUE);
    edit.edit = 3;
    send(&mut s, key, edit).unwrap();
    assert_eq!(kind(&s, key, 3), "whole");
}

// covers: image.kitty-animation
#[test]
fn a_frame_past_frame_chain_max_layers_is_composed_when_it_arrives() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    // frames 2 to 9 each lay one layer on the one before: frame 9 applies
    // eight layers, frame 10 would apply nine
    for n in 2u32..=10 {
        let mut d = data(n, 0, 1, 1, BLUE);
        d.base = n - 1;
        assert_eq!(send(&mut s, key, d), Ok(n));
    }
    for n in 2..=9 {
        assert_eq!(kind(&s, key, n), "change", "frame {n}");
    }
    assert_eq!(anim(&s, key).depth(9), FRAME_CHAIN_MAX);
    assert_eq!(kind(&s, key, 10), "whole");
    // an edit that would make a frame apply nine layers composes it
    for _ in 0..7 {
        let mut d = data(0, 1, 1, 1, BLUE);
        d.edit = 2;
        send(&mut s, key, d).unwrap();
    }
    assert_eq!(anim(&s, key).depth(2), 8);
    assert_eq!(kind(&s, key, 2), "change");
    let mut d = data(0, 2, 1, 1, BLUE);
    d.edit = 2;
    send(&mut s, key, d).unwrap();
    assert_eq!(kind(&s, key, 2), "whole");
    // no frame of the image applies more than FRAME_CHAIN_MAX layers
    for n in 2..=10 {
        assert!(anim(&s, key).depth(n) <= FRAME_CHAIN_MAX);
    }
}

#[test]
fn source_over_is_premultiplied_and_exact() {
    // half-alpha green over opaque blue: green 0x80, blue 0xff * 127 / 255
    assert_eq!(over(HALF_GREEN, BLUE), 0xff00_807f);
    assert_eq!(over(RED, BLUE), RED);
    assert_eq!(over(0, BLUE), BLUE);
    assert_eq!(over(HALF_GREEN, 0), HALF_GREEN);
    // the rounding term: 1 × 128 + 127 is 255, which divides to 1
    assert_eq!(over(0x7f00_0000, 0x0101_0101), 0x8001_0101);
}

// covers: image.kitty-animation
#[test]
fn a_frame_composed_when_shown_holds_the_bytes_of_the_same_frame_composed_on_arrival() {
    // each blend, each on an earlier frame and on a color: one image keeps
    // the frame as a change, its twin is made the same way with the frame
    // forced whole, and both compose to the same bytes
    let size = Size {
        width: 300,
        height: 300,
    };
    let root = Pixels::new(
        (0..300u32 * 300)
            .map(|i| 0xff00_0000 | i.wrapping_mul(2_654_435_761) >> 8)
            .collect(),
    );
    for replace in [false, true] {
        for on_frame in [false, true] {
            let mut new = data(17, 31, 40, 23, HALF_GREEN);
            new.replace = replace;
            if on_frame {
                new.base = 1;
            } else {
                new.background = 0x3366_99aa;
            }
            let layer = || Layer {
                x: new.x,
                y: new.y,
                width: new.width,
                height: new.height,
                replace,
                pixels: Pixels::new(new.pixels.clone()),
            };
            let mut kept = Anim::default();
            let mut whole = Anim::default();
            kept.push(&root, size, &new, layer(), false, true, 1);
            whole.push(&root, size, &new, layer(), true, true, 1);
            assert!(matches!(kept.frames[0].kind, Kind::Change { .. }));
            assert!(matches!(whole.frames[0].kind, Kind::Whole(_)));
            let a = kept.compose(root.as_slice(), 2, size);
            let b = whole.compose(root.as_slice(), 2, size);
            assert_eq!(a, b, "replace {replace}, on a frame {on_frame}");
            // and the pixels are the blend's: inside the rectangle, one of
            // the data over the canvas; outside, the canvas
            let canvas = if on_frame {
                root.as_slice()[0]
            } else {
                premultiply(0x33, 0x66, 0x99, 0xaa)
            };
            let inside = if on_frame {
                root.as_slice()[31 * 300 + 17]
            } else {
                canvas
            };
            let want = if replace {
                HALF_GREEN
            } else {
                over(HALF_GREEN, inside)
            };
            assert_eq!(a[31 * 300 + 17], want);
            assert_eq!(a[0], canvas);
        }
    }
}

// covers: image.kitty-animation
#[test]
fn a_composed_frame_shows_its_base_and_its_layers_in_order() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    let mut d = data(0, 0, 2, 1, BLUE);
    d.base = 1;
    send(&mut s, key, d).unwrap();
    let mut d = data(1, 0, 2, 1, HALF_GREEN);
    d.edit = 2;
    send(&mut s, key, d).unwrap();
    assert_eq!(kind(&s, key, 2), "change");
    let p = pixels(&s, key, 2);
    assert_eq!(
        &p[..4],
        &[BLUE, over(HALF_GREEN, BLUE), over(HALF_GREEN, RED), RED]
    );
    // on the default color, transparent black
    send(&mut s, key, data(0, 0, 1, 1, HALF_GREEN)).unwrap();
    let p = pixels(&s, key, 3);
    assert_eq!(&p[..2], &[HALF_GREEN, 0]);
}

// covers: image.kitty-animation
#[test]
fn an_edit_of_a_base_composes_the_frames_laid_on_it_first() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    // frame 2 lies on the root, frame 3 on frame 2
    let mut d = data(0, 0, 1, 1, BLUE);
    d.base = 1;
    send(&mut s, key, d).unwrap();
    let mut d = data(1, 0, 1, 1, BLUE);
    d.base = 2;
    send(&mut s, key, d).unwrap();
    let (two, three) = (pixels(&s, key, 2), pixels(&s, key, 3));
    let revisions = (
        s.frame_pixels(key, 2).unwrap().revision,
        s.frame_pixels(key, 3).unwrap().revision,
    );
    // an edit of the root composes frame 2 whole first
    let mut d = data(0, 5, 4, 4, HALF_GREEN);
    d.edit = 1;
    send(&mut s, key, d).unwrap();
    assert_eq!(kind(&s, key, 2), "whole");
    assert_eq!(kind(&s, key, 3), "change", "frame 3 lies on frame 2");
    assert_eq!(pixels(&s, key, 2), two, "frame 2 keeps its pixels");
    assert_eq!(pixels(&s, key, 3), three);
    assert_eq!(
        s.frame_pixels(key, 2).unwrap().revision,
        revisions.0,
        "composing whole changes no pixel revision"
    );
    // an edit of frame 2 composes frame 3 whole first
    let mut d = data(9, 9, 1, 1, HALF_GREEN);
    d.edit = 2;
    send(&mut s, key, d).unwrap();
    assert_eq!(kind(&s, key, 3), "whole");
    assert_eq!(pixels(&s, key, 3), three);
    assert_eq!(s.frame_pixels(key, 3).unwrap().revision, revisions.1);
    assert_eq!(s.frame_bytes(), charges(&s));
}

// covers: image.kitty-animation
#[test]
fn a_composition_onto_a_base_composes_the_frames_laid_on_it_first() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    let mut d = data(1, 0, 1, 1, BLUE);
    d.base = 2;
    send(&mut s, key, d).unwrap();
    let three = pixels(&s, key, 3);
    // a=c from the root onto frame 2: frame 2 is the destination and is
    // composed whole, and frame 3, laid on it, keeps its pixels
    let c = Compose {
        src: 1,
        dst: 2,
        width: 4,
        height: 4,
        replace: true,
        ..Compose::default()
    };
    assert_eq!(s.compose_frames(key, c), Ok(()));
    assert_eq!(kind(&s, key, 2), "whole");
    assert_eq!(kind(&s, key, 3), "whole");
    assert_eq!(pixels(&s, key, 3), three);
    assert_eq!(pixels(&s, key, 2)[0], RED);
    assert_eq!(s.frame_bytes(), charges(&s));
}

// covers: image.kitty-animation
#[test]
fn a_composition_answers_each_refusal_and_changes_nothing() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 8, 8, RED);
    let c = |src, dst, x, y| Compose {
        src,
        dst,
        dst_x: x,
        dst_y: y,
        width: 4,
        height: 4,
        ..Compose::default()
    };
    assert_eq!(s.compose_frames(key, c(1, 2, 0, 0)), Err(Refusal::NotFound));
    assert!(s.animation(key).is_none(), "a refusal makes no animation");
    send(&mut s, key, data(0, 0, 8, 8, BLUE)).unwrap();
    let before = s.clone();
    assert_eq!(s.compose_frames(key, c(1, 2, 5, 0)), Err(Refusal::Invalid));
    assert_eq!(s.compose_frames(key, c(2, 2, 2, 2)), Err(Refusal::Invalid));
    assert_eq!(s.compose_frames(key, c(3, 2, 0, 0)), Err(Refusal::NotFound));
    assert_eq!(s, before);
    // the same frame, rectangles apart, composes
    assert_eq!(s.compose_frames(key, c(2, 2, 4, 4)), Ok(()));
}

// covers: image.kitty-animation
#[test]
fn the_cache_is_charged_its_full_bound_whatever_it_holds() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    let bound = FRAME_CACHE_FRAMES * 256 * 256 * 4;
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    assert_eq!(s.frame_bytes(), bound + layer_cost(1, 1));
    for n in 0..5u32 {
        send(&mut s, key, data(n, 0, 1, 1, BLUE)).unwrap();
    }
    let charged = s.frame_bytes();
    let unshown = s.clone();
    for n in 2..=7 {
        let _ = s.frame_pixels(key, n);
    }
    assert_eq!(s.frame_bytes(), charged, "showing frames charges nothing");
    assert!(anim(&s, key).cache.len() <= usize::try_from(FRAME_CACHE_FRAMES).unwrap());
    assert_eq!(anim(&s, key).cache.len(), 2);
    assert_eq!(s, unshown, "the cache is not the store's state");
    // a cached frame is the one composed
    assert!(
        s.frame_pixels(key, 7)
            .unwrap()
            .pixels
            .same(&s.frame_pixels(key, 7).unwrap().pixels)
    );
    // with no change frame left, the bound is given back
    s.delete_frames(key);
    assert_eq!(s.frame_bytes(), 0);
    assert_eq!(anim(&s, key).cache.len(), 0);
}

// covers: image.kitty-animation
#[test]
fn controls_set_the_state_and_move_the_stamps_by_what_they_set() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 4, 4, RED);
    for _ in 0..2 {
        send(&mut s, key, data(0, 0, 4, 4, BLUE)).unwrap();
    }
    let st = s.animation(key).unwrap();
    assert_eq!(
        (st.frames, st.current, st.run, st.loops),
        (3, 1, Run::Stopped, 1)
    );
    assert_eq!(s.frame_gap(key, 1), Some(0), "the root frame's gap is 0");
    assert_eq!(s.frame_gap(key, 2), Some(40), "a new frame's gap is 40");
    let control = |s: &mut Store, c: Control| s.control_animation(key, c);
    // c moves the frame stamp alone
    control(
        &mut s,
        Control {
            current: 3,
            ..Control::default()
        },
    )
    .unwrap();
    let st2 = s.animation(key).unwrap();
    assert_eq!(st2.current, 3);
    assert_eq!(
        (st2.frame_stamp, st2.run_stamp),
        (st.frame_stamp + 1, st.run_stamp)
    );
    // s and v move the run stamp alone, once a command
    control(
        &mut s,
        Control {
            run: 3,
            loops: 4,
            ..Control::default()
        },
    )
    .unwrap();
    let st3 = s.animation(key).unwrap();
    assert_eq!((st3.run, st3.loops), (Run::Looping, 4));
    assert_eq!(
        (st3.frame_stamp, st3.run_stamp),
        (st2.frame_stamp, st2.run_stamp + 1)
    );
    control(
        &mut s,
        Control {
            run: 2,
            ..Control::default()
        },
    )
    .unwrap();
    assert_eq!(s.animation(key).unwrap().run, Run::Loading);
    control(
        &mut s,
        Control {
            run: 1,
            ..Control::default()
        },
    )
    .unwrap();
    assert_eq!(s.animation(key).unwrap().run, Run::Stopped);
    // a gap moves no stamp; z=0 and v=0 are ignored
    let st4 = s.animation(key).unwrap();
    control(
        &mut s,
        Control {
            frame: 1,
            gap: 100,
            ..Control::default()
        },
    )
    .unwrap();
    control(
        &mut s,
        Control {
            frame: 2,
            gap: -1,
            ..Control::default()
        },
    )
    .unwrap();
    control(
        &mut s,
        Control {
            frame: 3,
            ..Control::default()
        },
    )
    .unwrap();
    assert_eq!(s.animation(key).unwrap(), st4);
    assert_eq!(s.frame_gap(key, 1), Some(100));
    assert_eq!(s.frame_gap(key, 2), Some(-1));
    assert_eq!(s.frame_gap(key, 3), Some(40));
    // refusals change nothing
    let before = s.clone();
    assert_eq!(
        control(
            &mut s,
            Control {
                current: 4,
                ..Control::default()
            }
        ),
        Err(Refusal::NotFound)
    );
    assert_eq!(
        control(
            &mut s,
            Control {
                frame: 9,
                gap: 5,
                ..Control::default()
            }
        ),
        Err(Refusal::NotFound)
    );
    assert_eq!(
        control(
            &mut s,
            Control {
                run: 4,
                current: 2,
                ..Control::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(s, before);
}

// covers: image.kitty-animation
#[test]
fn deleting_the_frames_leaves_the_root_stopped_and_moves_both_stamps() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    s.control_animation(
        key,
        Control {
            current: 2,
            run: 3,
            ..Control::default()
        },
    )
    .unwrap();
    let st = s.animation(key).unwrap();
    s.delete_frames(key);
    let gone = s.animation(key).unwrap();
    assert_eq!((gone.frames, gone.current, gone.run), (1, 1, Run::Stopped));
    assert_eq!(gone.frame_stamp, st.frame_stamp + 1);
    assert_eq!(gone.run_stamp, st.run_stamp + 1);
    assert_eq!(s.frame_bytes(), 0);
    assert_eq!(pixels(&s, key, 1), vec![RED; 256 * 256]);
    assert!(s.frame_pixels(key, 2).is_none());
}

// covers: image.kitty-animation
#[test]
fn an_image_freed_gives_its_frames_allowance_back() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 16, 16, RED);
    send(&mut s, key, data(0, 0, 16, 16, BLUE)).unwrap();
    assert!(s.frame_bytes() > 0);
    // a replacement by kitty id frees the old image and its frames
    image(&mut s, 1, 16, 16, RED);
    assert_eq!(s.frame_bytes(), 0);
    let key = image(&mut s, 2, 16, 16, RED);
    send(&mut s, key, data(0, 0, 16, 16, BLUE)).unwrap();
    s.reset();
    assert_eq!(s.frame_bytes(), 0);
}

// covers: image.kitty-animation
#[test]
fn a_change_frames_opacity_follows_its_base_and_its_layers() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    let opaque = |s: &Store, n| s.frame_pixels(key, n).unwrap().opaque;
    // on the opaque root: a blend of any data, or a replacement by opaque
    // data, stays opaque; a replacement by clear data does not
    let mut d = data(0, 0, 1, 1, HALF_GREEN);
    d.base = 1;
    send(&mut s, key, d).unwrap();
    let mut d = data(0, 0, 1, 1, BLUE);
    d.base = 1;
    d.replace = true;
    send(&mut s, key, d).unwrap();
    let mut d = data(0, 0, 1, 1, HALF_GREEN);
    d.base = 1;
    d.replace = true;
    send(&mut s, key, d).unwrap();
    assert_eq!(
        (opaque(&s, 2), opaque(&s, 3), opaque(&s, 4)),
        (true, true, false)
    );
    // on the transparent default color, never
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    assert!(!opaque(&s, 5));
    // the image counts as opaque only while every frame is
    assert!(!anim(&s, key).opaque());
    s.delete_frames(key);
    assert!(anim(&s, key).opaque());
}

// covers: image.kitty-animation
#[test]
fn the_pixel_identity_moves_with_the_frame_shown_and_with_a_change_to_it() {
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    let root = s.frame_pixels(key, 1).unwrap();
    let two = s.frame_pixels(key, 2).unwrap();
    assert_eq!((root.frame, two.frame), (1, 2));
    assert_ne!(root.pixels, two.pixels);
    // a change to frame 2 gives it a new pixel revision, and the root keeps
    // its own
    let mut d = data(3, 3, 1, 1, BLUE);
    d.edit = 2;
    send(&mut s, key, d).unwrap();
    let two_again = s.frame_pixels(key, 2).unwrap();
    assert!(two_again.revision > two.revision);
    assert_ne!(two_again.pixels, two.pixels);
    assert_eq!(s.frame_pixels(key, 1).unwrap().revision, root.revision);
    // a frame shown twice unchanged names the same pixels
    assert_eq!(s.frame_pixels(key, 2).unwrap().revision, two_again.revision);
}

// covers: image.kitty-animation
#[test]
fn a_frame_entry_names_its_pixels_by_image_frame_and_pixel_revision() {
    let lines: Vec<crate::LineKey> = (0..4).map(crate::LineKey::new).collect();
    let rows = crate::Rows {
        screen: &lines,
        above: &[],
    };
    let screens = Screens {
        main: rows,
        alt: crate::Rows::default(),
    };
    let mut s = store(1 << 20);
    let key = image(&mut s, 1, 256, 256, RED);
    s.place(key, crate::Place::default(), screens, 80)
        .expect("placed");
    let plain = s.frame(rows, 80).remove(0);
    let root = s.frame_pixels(key, 1).unwrap();
    assert_eq!((plain.frame, plain.pixel_revision), (1, root.revision));
    assert!(plain.animation.is_none(), "no animation command named it");
    send(&mut s, key, data(0, 0, 1, 1, BLUE)).unwrap();
    let mut entry = s.frame(rows, 80).remove(0);
    // the store fills in the image's current frame, still the root, and
    // the animation's state
    assert_eq!((entry.frame, entry.pixel_revision), (1, root.revision));
    assert!(entry.pixels.same(&root.pixels));
    assert_eq!(entry.animation, s.animation(key));
    assert!(!entry.opaque, "frame 2 lies on transparent black");
    // the snapshot shows the frame the clock chose: same image and store
    // revision, another frame and pixel revision, and that frame's pixels
    let revision = entry.revision;
    let two = s.frame_pixels(key, 2).unwrap();
    entry.show(two.clone());
    assert_eq!(entry.image, key);
    assert_eq!(entry.revision, revision);
    assert_eq!((entry.frame, entry.pixel_revision), (2, two.revision));
    assert_ne!(entry.pixel_revision, plain.pixel_revision);
    assert!(entry.pixels.same(&two.pixels));
}
