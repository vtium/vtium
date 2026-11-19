//! The console's font helper draws only from the active fonts. Each
//! helper the console starts is told the batch first, and the helper reads
//! a font file only while the batch lists it and the file passes the trust
//! rule. Here the console's `Helper::sources` asks vtium-config's real
//! serve mode which sources of a font set it leaves out.
//!
//! References:
//! - ADR-1205 D4: the batch, the list of a fresh install with no batch
//!   file, and a refused file counting as missing.

#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/helper.rs"]
mod stand_in;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vtium_settings::batch::{Batch, Entry};

use scratch::Scratch;

/// What the real helper answers a sources query for the font set `text`,
/// told that `batch` lists the active fonts under the test's own account:
/// one line for each source it leaves out.
fn sources(d: &Path, name: &str, text: &str, batch: &Batch) -> String {
    #[expect(
        clippy::disallowed_methods,
        reason = "the query's clock stands at the real time, so its wait is the whole wait"
    )]
    let at = Instant::now();
    let me = rustix::process::geteuid().as_raw();
    let command = stand_in::real_helper(&d.join(format!("{name}.kinds")), None);
    let launcher = vtium::helper::Launcher::open(command)
        .expect("the helper's program")
        .with_active(me, batch);
    let mut helper = vtium::helper::Helper::new(
        launcher,
        std::sync::Arc::new(|| {}),
        at,
        vtium::helper::FritText {
            name: name.to_owned(),
            text: text.to_owned(),
        },
        None,
        vtium::deferred::Shared::default(),
        0,
    );
    helper.quiet();
    helper
        .sources(Duration::from_secs(30), &|| at)
        .expect("an answer")
}

/// A BDF font whose one glyph, A, fills a cell `height` pixels tall and
/// half as wide.
fn bdf(height: u32) -> String {
    let width = height / 2;
    let bytes = width.div_ceil(8);
    let lit = (1u64 << width) - 1;
    let row = format!(
        "{:0digits$X}\n",
        lit << (bytes * 8 - width),
        digits = (bytes * 2) as usize
    );
    format!(
        "STARTFONT 2.1\nFONT -test-\nFONTBOUNDINGBOX {width} {height} 0 -1\n\
         STARTPROPERTIES 3\nFONT_ASCENT {}\nFONT_DESCENT 1\n\
         CHARSET_REGISTRY \"ISO10646\"\nENDPROPERTIES\nCHARS 1\n\
         STARTCHAR A\nENCODING 65\nDWIDTH {width} 0\nBBX {width} {height} 0 -1\nBITMAP\n{}ENDCHAR\nENDFONT\n",
        height - 1,
        row.repeat(height as usize)
    )
}

// ADR-1205 D4: a font file the batch does not list is refused, and the
// answer names the file and why; listed, the same file reads
#[test]
fn the_helper_reads_a_font_only_while_the_batch_lists_it() {
    let d = Scratch::new("active-fonts-listed");
    let font = d.join("f.bdf");
    std::fs::write(&font, bdf(16)).expect("the font");
    let text = format!("source=bdf:{}\nsource=builtin:unscii-8\n", font.display());
    let none = sources(&d, "none", &text, &Batch::default());
    assert_eq!(
        none,
        format!(
            "missing\t1\tbdf:{0}\t{0}: vtium treats this file as missing, because it is not among the active fonts.",
            font.display()
        )
    );
    let listed = Batch::new(vec![Entry::Path(font.clone())]);
    assert_eq!(sources(&d, "listed", &text, &listed), "");
}

// ADR-1205 D4 and its note of 2026-11-18: with no batch file, the list of
// a fresh install makes the default set's fonts active, so the default set
// terminus draws every source with no setup. Here the package's font
// folder is the test's own, holding a font of each name the package ships
#[test]
fn the_list_of_a_fresh_install_draws_the_default_set() {
    let d = Scratch::new("active-fonts-fresh");
    let fonts = d.join("fonts");
    std::fs::create_dir(&fonts).expect("the font folder");
    for name in vtium_settings::batch::FRESH_FONTS {
        let height = name
            .strip_prefix("ter-u")
            .and_then(|rest| rest.get(..2))
            .and_then(|h| h.parse().ok())
            .unwrap_or(16);
        std::fs::write(fonts.join(name), bdf(height)).expect("a font");
    }
    // the shipped default set, its fonts in the test's folder, without
    // its hashes and versions, which the test's fonts do not match, and
    // without its fallback layer, which the console splices in
    let shipped = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium-config/frits/console/terminus.vtfrit"),
    )
    .expect("the default set");
    let text: String = shipped
        .replace("/usr/share/vtium/fonts/", &format!("{}/", fonts.display()))
        .lines()
        .filter(|l| {
            !["sha256=", "version=", "layer="]
                .iter()
                .any(|k| l.starts_with(k))
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let fresh = Batch::fresh_install(&fonts, &d.join("no-palettes"), None, &[]);
    assert_eq!(sources(&d, "terminus", &text, &fresh), "");
    // a batch that lists another font leaves every source of the set out
    let other = Batch::new(vec![Entry::Path(PathBuf::from("/usr/share/fonts/x.ttf"))]);
    let left = sources(&d, "other", &text, &other);
    assert!(left.starts_with("nothing\t0\t\t"), "{left}");
    assert_eq!(
        left.lines().filter(|l| l.starts_with("missing\t")).count(),
        2,
        "{left}"
    );
}
