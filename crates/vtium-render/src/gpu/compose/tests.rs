use super::composition::arrow_quads;
use super::*;

// A quad that covers part of a cell receives ink as a job of that whole
// cell, since the shader reads the line bits of the cell each of its
// pixels lies in; a quad of no ink is no receiving job.
#[test]
fn a_quad_narrower_than_a_cell_receives_as_a_job_of_the_whole_cell() {
    let cell = Size::new(8, 16);
    let quad = |w: i32, flags: u32| Quad {
        dst: [12, 0, w, 16],
        src: [0, 0, 4, 2],
        params: [0, flags << 24, 0, 0],
    };
    let job = |cells, down| {
        Some(ReceivingJob {
            col: 4,
            row: 2,
            cells,
            down,
        })
    };
    assert_eq!(receiving(&quad(5, RECEIVES), cell), job(1, false));
    assert_eq!(receiving(&quad(16, RECEIVES), cell), job(2, false));
    assert_eq!(receiving(&quad(9, RECEIVES | DOWN), cell), job(2, true));
    assert_eq!(receiving(&quad(8, MARKED), cell), None);
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// How many quads cover each pixel of a `w` by `h` frame.
fn cover(quads: &[Quad], w: u32, h: u32) -> Vec<u32> {
    let mut n = vec![0; (w * h) as usize];
    for q in quads {
        for y in q.dst[1]..q.dst[1] + q.dst[3] {
            for x in q.dst[0]..q.dst[0] + q.dst[2] {
                n[(y as u32 * w + x as u32) as usize] += 1;
            }
        }
    }
    n
}

#[test]
fn the_arrow_covers_each_drawn_pixel_under_it_once() {
    let image = Arrow::new(16, crate::target::Orientation::Normal);
    let a = ArrowAt {
        image: &image,
        x: 3,
        y: 2,
    };
    // two cells drawn and a border piece overlapping one of them, and
    // a cell under the arrow the frame did not draw
    let drawn = [rect(0, 0, 8, 16), rect(8, 0, 8, 16), rect(4, 4, 10, 2)];
    let size = Size::new(40, 30);
    let quads = arrow_quads(a, &drawn, size);
    let n = cover(&quads, 40, 30);
    for y in 0..30u32 {
        for x in 0..40u32 {
            let under = x >= 3 && x < 3 + image.width() && y >= 2 && y < 2 + image.height();
            let in_drawn = drawn
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
            let want = u32::from(under && in_drawn);
            assert_eq!(n[(y * 40 + x) as usize], want, "{x},{y}");
        }
    }
    // each quad samples the arrow where it lies
    for q in &quads {
        assert_eq!(q.src[0] as i32, q.dst[0] - 3);
        assert_eq!(q.src[1] as i32, q.dst[1] - 2);
        assert_eq!((q.src[2] as i32, q.src[3] as i32), (q.dst[2], q.dst[3]));
    }
    // the lines alike are joined: the two cells give one quad down
    // their rows above the border piece
    assert!(quads.len() < 8, "{quads:?}");
}

#[test]
fn the_arrow_past_the_frames_edge_is_cut_to_it() {
    let image = Arrow::new(16, crate::target::Orientation::Normal);
    let a = ArrowAt {
        image: &image,
        x: -4,
        y: 26,
    };
    let size = Size::new(40, 30);
    let quads = arrow_quads(a, &[size.rect()], size);
    let n = cover(&quads, 40, 30);
    assert_eq!(n.iter().sum::<u32>(), (image.width() - 4) * 4);
    assert!(quads.iter().all(|q| q.src[0] >= 4));
}
