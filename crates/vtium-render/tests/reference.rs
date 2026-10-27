//! Whole screens against the C reference: each screen of
//! `reference/screens.tsv`, drawn through vtium-term's frame with unifont,
//! must give the pixels `reference/refcells.c` drew with the C reference's
//! glyphs, sprites and blending (`reference/README.md`).

mod common;

use common::{digest, reference_screens, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::Age;

fn want(name: &str) -> u64 {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/cells.tsv");
    let tsv = std::fs::read_to_string(path).expect("cells.tsv");
    tsv.lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f[0] == "screen" && f[1] == name)
        .map(|f| u64::from_str_radix(f[2], 16).expect("hex"))
        .unwrap_or_else(|| panic!("no digest for {name}"))
}

#[test]
fn screens_match_the_c_reference() {
    let screens = reference_screens();
    assert_eq!(screens.len(), 3);
    for (name, screen) in &screens {
        let (w, h) = (screen.cols as u32 * 8, screen.rows as u32 * 16);
        let mut r = Renderer::new(unifont(), w, h, Config::default());
        let mut canvas = Canvas::new(w, h, 0);
        let state = DrawState {
            background: screen.palette.bg(),
            foreground: screen.palette.fg(),
            ..DrawState::default()
        };
        r.draw(&screen.frame(Age::NEVER), &mut canvas, 0, &state);
        assert_eq!(digest(&canvas), want(name), "screen {name}");
    }
}
