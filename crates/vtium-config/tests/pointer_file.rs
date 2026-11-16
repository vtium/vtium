//! vtium's shipped pointer file, `crates/vtium/install/pointers.svg`,
//! against the rules of the pointer file reference
//! (`docs/reference/pointer-file.md`): an SVG file within
//! `POINTER_SVG_BYTES_MAX`, in which each of the ten pointer shapes is a
//! `<symbol>` exactly once, with a viewBox and a `data-hotspot` inside it,
//! and in which the font helper draws each of them.
//!
//! References:
//! - ADR-1309 D7: the third test, the shipped file held to the reference page's rules.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use vtium_config::pointer_svg::draw;
use vtium_render::POINTER_SVG_BYTES_MAX;
use vtium_term::PointerShape;

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../vtium/install/pointers.svg");
const SVG_NS: &str = "http://www.w3.org/2000/svg";

fn numbers(text: &str) -> Vec<f64> {
    text.split_ascii_whitespace()
        .map(|t| t.parse().unwrap())
        .collect()
}

#[test]
fn the_shipped_pointer_file_keeps_the_reference_pages_rules() {
    let bytes = std::fs::read(FILE).unwrap();
    assert!(
        bytes.len() <= POINTER_SVG_BYTES_MAX,
        "{} bytes",
        bytes.len()
    );
    let text = std::str::from_utf8(&bytes).unwrap();
    let doc = roxmltree::Document::parse(text).unwrap();
    let root = doc.root_element();
    assert_eq!(
        (root.tag_name().name(), root.tag_name().namespace()),
        ("svg", Some(SVG_NS))
    );
    for shape in PointerShape::ALL {
        let name = shape.name();
        let symbols: Vec<_> = doc
            .descendants()
            .filter(|n| n.tag_name().name() == "symbol" && n.attribute("id") == Some(name))
            .collect();
        let [symbol] = symbols.as_slice() else {
            panic!("{name}: {} symbols, not one", symbols.len());
        };
        let view = numbers(symbol.attribute("viewBox").unwrap());
        let hot = numbers(symbol.attribute("data-hotspot").unwrap());
        let ([x, y, w, h], [hx, hy]) = (view.as_slice(), hot.as_slice()) else {
            panic!("{name}: a viewBox {view:?} or data-hotspot {hot:?} of the wrong length");
        };
        assert!(*w > 0.0 && *h > 0.0, "{name}");
        assert!(
            (*x..=x + w).contains(hx) && (*y..=y + h).contains(hy),
            "{name}"
        );
        assert!(draw(&bytes, name, 16).is_ok(), "{name}");
    }
    // no symbol other than the ten
    let ids: Vec<&str> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "symbol")
        .filter_map(|n| n.attribute("id"))
        .collect();
    assert_eq!(ids.len(), 10, "{ids:?}");
}
