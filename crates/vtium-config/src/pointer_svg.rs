//! The drawing of an SVG pointer file, which the font helper runs in its
//! sandbox for the console's pointer.
//!
//! An SVG pointer file holds each pointer shape as a `<symbol>` whose `id`
//! is the shape's OSC 22 name, such as `default` or `ew-resize`. Each
//! symbol has a `viewBox` and a `data-hotspot="x y"` attribute, the point
//! the pointer names in the viewBox's units. A symbol may use anything else
//! the file defines, such as a gradient in its `<defs>` or its CSS.
//!
//! To draw a shape, the helper finds the symbol with roxmltree and reads its
//! viewBox and hotspot. It then makes a new document from the file. The
//! new root keeps the file's own root attributes, its namespaces and
//! presentation attributes such as `shape-rendering`, but takes the
//! picture's size in pixels as its width, height and viewBox. Everything
//! inside the file's root goes into a `<defs>`, so nothing of the file
//! draws by itself, and one `<use>` of the symbol at the picture's size
//! follows. resvg draws that document into premultiplied RGBA. usvg loads
//! no image a file names, so a file cannot make the helper read another
//! file.
//!
//! The picture is a cell tall, kept between 8 pixels and
//! [`POINTER_SIDE_MAX`], and as wide as the viewBox's proportions make it,
//! rounded up. When that width would pass [`POINTER_SIDE_MAX`], the helper
//! draws at the scale that makes the width [`POINTER_SIDE_MAX`] instead.
//! The hotspot's pixel is each coordinate, from the viewBox's corner, times
//! the scale, rounded down and kept inside the picture.
//!
//! References:
//! - ADR-1309 D4: the symbol, its viewBox and data-hotspot, the size rule, the hotspot's pixel, how the helper draws a symbol, and the file's size limit.
//! - SVG 2: symbols drawn through `use`, whose width and height size the symbol's viewBox, and custom `data-` attributes.

use resvg::{tiny_skia, usvg};
use vtium_render::{POINTER_SIDE_MAX, POINTER_SVG_BYTES_MAX};

use crate::serve::PointerPicture;

/// The SVG namespace, which a symbol's element must be in.
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// The fewest pixels a picture is tall.
const HEIGHT_MIN: u32 = 8;

/// Why the helper draws no picture of a shape from a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoPicture {
    /// The file has no symbol for the shape.
    Missing,
    /// The file or its symbol breaks a rule: the words finish the sentence
    /// "because ...".
    Refused(String),
}

/// A symbol as the file states it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Symbol {
    /// The viewBox: its corner, width and height.
    view: (f64, f64, f64, f64),
    /// The hotspot in the viewBox's units.
    hot: (f64, f64),
}

/// Four or two numbers from an attribute's value, separated by spaces or
/// commas.
fn numbers(text: &str) -> Option<Vec<f64>> {
    text.split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect()
}

/// The symbol `id` of the document `doc`: its viewBox and hotspot checked.
fn symbol(doc: &roxmltree::Document<'_>, id: &str) -> Result<Symbol, NoPicture> {
    let node = doc
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "symbol"
                && n.tag_name().namespace() == Some(SVG_NS)
                && n.attribute("id") == Some(id)
        })
        .ok_or(NoPicture::Missing)?;
    let refused = |why: &str| NoPicture::Refused(why.to_owned());
    let view = match node.attribute("viewBox").and_then(numbers).as_deref() {
        Some(&[x, y, w, h]) if w > 0.0 && h > 0.0 => (x, y, w, h),
        Some(_) => {
            return Err(refused(
                "its symbol's viewBox is not four numbers with a width and a height above 0",
            ));
        }
        None => return Err(refused("its symbol has no viewBox")),
    };
    let hot = match node.attribute("data-hotspot").and_then(numbers).as_deref() {
        Some(&[x, y]) => (x, y),
        Some(_) => return Err(refused("its symbol's data-hotspot is not two numbers")),
        None => return Err(refused("its symbol has no data-hotspot")),
    };
    let (vx, vy, vw, vh) = view;
    if !(vx..=vx + vw).contains(&hot.0) || !(vy..=vy + vh).contains(&hot.1) {
        return Err(refused(
            "its symbol's data-hotspot lies outside its viewBox",
        ));
    }
    Ok(Symbol { view, hot })
}

/// The picture's width and height in pixels for a viewBox `vw` by `vh` at
/// cells `cell_height` tall, and the scale from the viewBox's units to
/// pixels.
fn size(vw: f64, vh: f64, cell_height: u32) -> (u32, u32, f64) {
    let side = f64::from(POINTER_SIDE_MAX);
    let h = f64::from(cell_height.clamp(HEIGHT_MIN, POINTER_SIDE_MAX));
    let mut scale = h / vh;
    let mut w = (vw * scale).ceil();
    let mut h = h;
    if w > side {
        scale = side / vw;
        w = side;
        h = (vh * scale).ceil().clamp(1.0, side);
    }
    let px = |v: f64| {
        // a value already kept between 1 and POINTER_SIDE_MAX
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "kept between 1 and 64"
        )]
        let v = v.clamp(1.0, side) as u32;
        v
    };
    (px(w), px(h), scale)
}

/// The attributes of the root's start tag `tag` (from `<svg` to its `>`),
/// written again without those that set the root's size and position.
fn root_attributes(tag: &str) -> Option<String> {
    const DROPPED: [&str; 6] = [
        "width",
        "height",
        "viewBox",
        "x",
        "y",
        "preserveAspectRatio",
    ];
    let mut rest = tag.strip_prefix('<')?.trim_start();
    rest = rest.get(rest.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')?..)?;
    let mut kept = String::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with('>') || rest.starts_with("/>") {
            return Some(kept);
        }
        let eq = rest.find('=')?;
        let name = rest.get(..eq)?.trim();
        let value = rest.get(eq + 1..)?.trim_start();
        let quote = value.chars().next().filter(|&c| c == '"' || c == '\'')?;
        let end = value.get(1..)?.find(quote)? + 1;
        let attribute = value.get(..=end)?;
        if !DROPPED.contains(&name) {
            kept.push(' ');
            kept.push_str(name);
            kept.push('=');
            kept.push_str(attribute);
        }
        rest = value.get(end + 1..)?;
    }
}

/// Where the root's start tag ends and its end tag starts in `text`, the
/// whole document, and the start tag's text: `None` for a root that
/// closes itself, which holds no symbol.
fn root_parts<'t>(text: &'t str, root: roxmltree::Node<'_, '_>) -> Option<(&'t str, &'t str)> {
    let range = root.range();
    let whole = text.get(range.clone())?;
    // the start tag ends at the first `>` outside a quoted value
    let mut quote = None;
    let mut tag_end = None;
    for (i, c) in whole.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                tag_end = Some(i);
                break;
            }
            (None, _) => {}
        }
    }
    let tag_end = tag_end?;
    let tag = whole.get(..=tag_end)?;
    if tag.ends_with("/>") {
        return None;
    }
    let inner_end = whole.rfind("</")?;
    let inner = whole.get(tag_end + 1..inner_end)?;
    Some((tag, inner))
}

/// The options usvg reads with: no image a file names is ever loaded.
fn options() -> usvg::Options<'static> {
    usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

/// Draws the shape `id` of the SVG file `bytes` for cells `cell_height`
/// pixels tall: a picture whose size is `cell_height`, or why there is
/// none.
pub fn draw(bytes: &[u8], id: &str, cell_height: u32) -> Result<PointerPicture, NoPicture> {
    let refused = |why: String| NoPicture::Refused(why);
    if bytes.len() > POINTER_SVG_BYTES_MAX {
        return Err(refused(format!(
            "it is longer than {} MiB",
            POINTER_SVG_BYTES_MAX / (1024 * 1024)
        )));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| refused("it is not UTF-8 text".to_owned()))?;
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        },
    )
    .map_err(|e| refused(format!("it is not an XML file vtium can read ({e})")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NS) {
        return Err(refused("it is not an SVG file".to_owned()));
    }
    let sym = symbol(&doc, id)?;
    let (vx, vy, vw, vh) = sym.view;
    let (w, h, scale) = size(vw, vh, cell_height);
    let (tag, inner) = root_parts(text, root).ok_or(NoPicture::Missing)?;
    let attributes = root_attributes(tag)
        .ok_or_else(|| refused("vtium cannot read the attributes of its svg element".to_owned()))?;
    let prologue = text.get(..root.range().start).unwrap_or_default();
    let document = format!(
        "{prologue}<svg{attributes} width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><defs>{inner}</defs><use href=\"#{id}\" x=\"0\" y=\"0\" width=\"{uw}\" height=\"{uh}\"/></svg>",
        uw = vw * scale,
        uh = vh * scale,
    );
    let tree = usvg::Tree::from_str(&document, &options())
        .map_err(|e| refused(format!("resvg cannot draw it ({e})")))?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| refused("its picture has no pixels".to_owned()))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let at = |v: f64, from: f64, n: u32| {
        let p = ((v - from) * scale)
            .floor()
            .clamp(0.0, f64::from(n.saturating_sub(1)));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "kept inside the picture"
        )]
        let p = p as u32;
        p
    };
    Ok(PointerPicture {
        size: cell_height,
        width: w,
        height: h,
        hot: (at(sym.hot.0, vx, w), at(sym.hot.1, vy, h)),
        rgba: pixmap.take(),
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn file(symbols: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"300\" height=\"100\" viewBox=\"0 0 300 100\" shape-rendering=\"crispEdges\">{symbols}<rect width=\"300\" height=\"100\" fill=\"red\"/></svg>"
        )
        .into_bytes()
    }

    fn pixel(p: &PointerPicture, x: u32, y: u32) -> [u8; 4] {
        let i = usize::try_from((y * p.width + x) * 4).unwrap();
        p.rgba.get(i..i + 4).unwrap().try_into().unwrap()
    }

    // A symbol that fills with a gradient from the file's defs draws at
    // the asked height, and nothing else of the file draws: the red
    // rectangle outside the symbol stays out of the picture.
    #[test]
    fn a_symbol_with_a_gradient_from_defs_draws_at_the_asked_height() {
        let f = file(
            "<defs><linearGradient id=\"g\" x1=\"0\" x2=\"0\" y1=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"#000\"/><stop offset=\"1\" stop-color=\"#00f\"/></linearGradient></defs>\
             <symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"url(#g)\"/></symbol>",
        );
        let p = draw(&f, "default", 16).unwrap();
        assert_eq!((p.size, p.width, p.height, p.hot), (16, 8, 16, (0, 0)));
        assert_eq!(p.rgba.len(), 8 * 16 * 4);
        let (top, bottom) = (pixel(&p, 4, 0), pixel(&p, 4, 15));
        assert_eq!(top[3], 255);
        assert_eq!(top[0], 0, "no red: {top:?}");
        assert!(
            bottom[2] > top[2],
            "the gradient runs down: {top:?} {bottom:?}"
        );
    }

    // A symbol with no data-hotspot, or one outside its viewBox, draws
    // nothing, and says why.
    #[test]
    fn a_symbol_without_a_hotspot_inside_its_viewbox_draws_nothing() {
        let rect = "<rect width=\"8\" height=\"16\" fill=\"#fff\"/>";
        let none = file(&format!(
            "<symbol id=\"wait\" viewBox=\"0 0 8 16\">{rect}</symbol>"
        ));
        assert_eq!(
            draw(&none, "wait", 16),
            Err(NoPicture::Refused(
                "its symbol has no data-hotspot".to_owned()
            ))
        );
        for hot in ["9 0", "0 16.5", "-1 0"] {
            let out = file(&format!(
                "<symbol id=\"wait\" viewBox=\"0 0 8 16\" data-hotspot=\"{hot}\">{rect}</symbol>"
            ));
            assert!(
                matches!(draw(&out, "wait", 16), Err(NoPicture::Refused(why)) if why.contains("outside")),
                "{hot}"
            );
        }
        // on the viewBox's far corner it passes, on the picture's last pixel
        let corner = file(&format!(
            "<symbol id=\"wait\" viewBox=\"0 0 8 16\" data-hotspot=\"8 16\">{rect}</symbol>"
        ));
        assert_eq!(draw(&corner, "wait", 16).unwrap().hot, (7, 15));
    }

    // An id that is not in the file is missing, as is a shape the file has
    // no symbol for.
    #[test]
    fn an_unknown_id_is_missing() {
        let f = file("<symbol id=\"defaults\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"/>");
        assert_eq!(draw(&f, "default", 16), Err(NoPicture::Missing));
    }

    // A wide viewBox draws POINTER_SIDE_MAX wide, at the scale that makes
    // it so, and its height follows.
    #[test]
    fn a_wide_viewbox_draws_pointer_side_max_wide() {
        let f = file(
            "<symbol id=\"ew-resize\" viewBox=\"0 0 40 10\" data-hotspot=\"20 5\"><rect width=\"40\" height=\"10\" fill=\"#fff\"/></symbol>",
        );
        let p = draw(&f, "ew-resize", 32).unwrap();
        assert_eq!((p.width, p.height), (64, 16));
        assert_eq!(p.hot, (32, 8));
    }

    // The height is a cell tall, kept between 8 and 64, and the width
    // follows the viewBox, rounded up.
    #[test]
    fn the_height_is_a_cell_kept_between_8_and_64() {
        let f = file(
            "<symbol id=\"text\" viewBox=\"0 0 7 18\" data-hotspot=\"3.5 9\"><rect width=\"7\" height=\"18\" fill=\"#fff\"/></symbol>",
        );
        for (cell, w, h, hot) in [
            (1, 4, 8, (1, 4)),
            (18, 7, 18, (3, 9)),
            (20, 8, 20, (3, 10)),
            (200, 25, 64, (12, 32)),
        ] {
            let p = draw(&f, "text", cell).unwrap();
            assert_eq!((p.width, p.height, p.hot), (w, h, hot), "cell {cell}");
            assert_eq!(p.size, cell);
        }
    }

    // A file past 1 MiB is refused before it is parsed, and at 1 MiB it
    // is read.
    #[test]
    fn a_file_past_1_mib_is_refused() {
        let mut f = file(
            "<symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol>",
        );
        let pad = POINTER_SVG_BYTES_MAX - f.len();
        f.splice(f.len() - 6..f.len() - 6, std::iter::repeat_n(b' ', pad));
        assert_eq!(f.len(), POINTER_SVG_BYTES_MAX);
        assert!(draw(&f, "default", 16).is_ok());
        f.insert(f.len() - 6, b' ');
        assert!(
            matches!(draw(&f, "default", 16), Err(NoPicture::Refused(why)) if why.contains("1 MiB"))
        );
    }

    // A file that names an image draws without it: usvg loads nothing.
    #[test]
    fn an_image_the_file_names_is_never_loaded() {
        let f = file(
            "<symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><image href=\"/etc/passwd\" width=\"8\" height=\"16\"/></symbol>",
        );
        let p = draw(&f, "default", 16).unwrap();
        assert!(p.rgba.iter().all(|&b| b == 0));
    }

    // Text that is not an SVG file, and an svg element in no namespace.
    #[test]
    fn a_file_that_is_not_svg_is_refused() {
        for f in [
            &b"not xml"[..],
            b"<svg><symbol id=\"default\"/></svg>",
            b"\xff\xfe",
        ] {
            assert!(matches!(draw(f, "default", 16), Err(NoPicture::Refused(_))));
        }
    }
}
