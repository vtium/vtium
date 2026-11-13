//! The words of the overlay screens (ADR-1401 D5, D7, D8, D9), each from
//! the console function that gives it, so a reworded notice moves its
//! golden. The names and the device are the screens' own: the head `desk`,
//! active and showing console 2 of 3, the head `side` beside it, and the
//! keyboard at `/dev/input/event3`.

use std::time::Duration;

use vtium::notice;
use vtium_render::{Notice, Report};

/// The keyboard the rescue key moves.
const KEYBOARD: &str = "/dev/input/event3";

/// A vtium.conf with one error in its sections, a rule naming a head the file does not
/// declare, which drops that rule alone: partly broken.
const ONE_ERROR: &str = "version = 1\n[head desk]\n[rules]\ndisplay * -> desc\n";

/// A vtium.conf with two errors in its sections: the same rule, and a `[head]` key vtium
/// does not know, which drops the head: broken, under open.
const TWO_ERRORS: &str =
    "version = 1\n[head desk]\nfont-weight = bold\n[rules]\ndisplay * -> desc\n";

/// A closed kiosk's vtium.conf with four errors in its sections: a key
/// its head section does not take, a font key that only the top level
/// and a look may set, which drops the head, a rule naming a head the file does not declare, and a second
/// `[rules]`. No head survives, so the start shows the panic screen
/// (ADR-1401 D7).
const BROKEN_CLOSED: &str = "version = 1\npolicy = closed\n[head kiosk]\nfocs = 1\nfont-size = 24\n[rules]\ndisplay * -> kisok\n[rules]\ninput * -> kiosk\n";

/// A closed kiosk's vtium.conf with one error in its sections: a key its
/// head section does not take, which drops the head. No head survives, so
/// the start shows the panic screen with that one error (ADR-1401 D7).
const ONE_ERROR_CLOSED: &str =
    "version = 1\npolicy = closed\n[head kiosk]\nfocs = 1\n[rules]\ndisplay * -> kiosk\n";

/// `osd = full`'s line on the active head.
pub fn osd_active() -> Result<String, String> {
    Ok(notice::osd_line(true, "desk", 2, 3))
}

/// `osd = full`'s line on a head that is not the active one.
pub fn osd_other() -> Result<String, String> {
    Ok(notice::osd_line(false, "side", 1, 1))
}

/// The render timer's line (`render-timer = osd`, `Report::osd`) for an
/// interval of ten seconds in which 599 frames were presented, the median
/// frame took 12.345 ms, and each frame's damage covered the whole
/// display: `59.9 fps 12.345 ms 100.0%`.
pub fn render_timer() -> Result<String, String> {
    let report = Report {
        interval: Duration::from_secs(10),
        flips: 599,
        frames: 599,
        cpu_median: Duration::from_micros(12_345),
        damage_px_mean: 1,
        screen_px: 1,
        ..Report::default()
    };
    Ok(report.osd())
}

pub fn no_keyboard() -> Result<String, String> {
    Ok(notice::NO_KEYBOARD.to_owned())
}

pub fn anti_lockout() -> Result<String, String> {
    Ok(notice::anti_lockout("desk"))
}

/// The rescue key's notice while `desk` is active.
pub fn rescue() -> Result<String, String> {
    Ok(notice::rescue(KEYBOARD, "desk"))
}

/// The rescue key's notice while no screen is active.
pub fn rescue_no_head() -> Result<String, String> {
    Ok(notice::rescue_no_head(
        KEYBOARD,
        &vtium::grab::Bindings::default(),
    ))
}

/// The notice of a console the open chord opened whose login could not
/// start (ADR-1401 D5).
pub fn no_console_opened() -> Result<String, String> {
    Ok(notice::NO_CONSOLE_OPENED.to_owned())
}

pub fn oneshot() -> Result<String, String> {
    Ok(notice::session_ended(&vtium::grab::Bindings::default()))
}

pub fn head_file_one_error() -> Result<String, String> {
    head_file(ONE_ERROR)
}

pub fn head_file_two_errors() -> Result<String, String> {
    head_file(TWO_ERRORS)
}

/// The text of the panic screen of a broken closed vtium.conf, every error
/// it holds, as the start at vtium's own path builds it from the file's
/// bytes (ADR-1401 D7).
pub fn panic() -> Result<String, String> {
    panic_report().map(|r| r.text())
}

/// The panic screen as the screen the display guard lights draws it in a
/// build without image support: a report, which counts the errors it has
/// no room for, with the drawing of a panicked Tux in text.
pub fn panic_screen() -> Result<Notice, String> {
    panic_shown(BROKEN_CLOSED, text_drawing())
}

/// The panic screen of a closed vtium.conf with one error, in a build
/// without image support.
pub fn panic_one_screen() -> Result<Notice, String> {
    panic_shown(ONE_ERROR_CLOSED, text_drawing())
}

/// The panic screen in a build with image support, which draws the sprite
/// in the place of the drawing in text.
pub fn panic_images_screen() -> Result<Notice, String> {
    panic_shown(BROKEN_CLOSED, sprite_drawing()?)
}

/// The panic screen of a closed vtium.conf with one error, in a build with
/// image support.
pub fn panic_images_one_screen() -> Result<Notice, String> {
    panic_shown(ONE_ERROR_CLOSED, sprite_drawing()?)
}

/// The drawing a build without image support shows (ADR-1401 D7).
fn text_drawing() -> vtium_render::Drawing {
    vtium_render::Drawing::text(notice::penguin::FULL, notice::penguin::SMALL)
}

/// The drawing a build with image support shows (ADR-1401 D7): the whole
/// sprite in the full drawing's place, and the sprite cropped to its
/// monitor in the small one's. Tier 2 builds vtium without the feature,
/// so it names the sprite and its crop itself.
fn sprite_drawing() -> Result<vtium_render::Drawing, String> {
    use notice::penguin::{PALETTE, SPRITE_MONITOR_AT, SPRITE_MONITOR_SIZE, SPRITE_SIZE, samples};
    let samples = samples().ok_or("the sprite's PPM has another header")?;
    let sprite = vtium_render::Sprite::new(SPRITE_SIZE.0, SPRITE_SIZE.1, &PALETTE, samples);
    let monitor = sprite.crop(
        SPRITE_MONITOR_AT.0,
        SPRITE_MONITOR_AT.1,
        SPRITE_MONITOR_SIZE.0,
        SPRITE_MONITOR_SIZE.1,
    );
    Ok(text_drawing().with_sprite(sprite, monitor))
}

/// The panic screen of the vtium.conf `file`, showing `drawing`.
fn panic_shown(file: &str, drawing: vtium_render::Drawing) -> Result<Notice, String> {
    let mut report = report_of(file)?;
    report.drawing = Some(drawing);
    Ok(Notice::report(report))
}

fn panic_report() -> Result<vtium_render::NoticeReport, String> {
    report_of(BROKEN_CLOSED)
}

/// The panic screen's report for the vtium.conf `file`, as the start at
/// vtium's own path builds it from the file's bytes.
fn report_of(file: &str) -> Result<vtium_render::NoticeReport, String> {
    let path = std::path::Path::new(vtium_settings::CONF_FILE);
    vtium::head::Start::trusted(path, file.as_bytes())
        .report()
        .cloned()
        .ok_or_else(|| format!("the file {file:?} shows no panic screen"))
}

/// The console line of a partly broken closed file whose surviving heads
/// have no display, alone on the boot display (ADR-1401 D7, D9).
pub fn partly() -> Result<Notice, String> {
    Ok(Notice::new(vtium::head::outcome_notice(
        vtium_settings::Line::Partly,
    )))
}

/// The notice of a crash loop that stopped every head under the closed
/// policy, alone on the boot display (ADR-1401 D1).
pub fn stopped() -> Result<Notice, String> {
    Ok(Notice::new(notice::stopped()))
}

/// The notice the start shows for the vtium.conf `text`, read as the
/// installed file, its console line and the next step, as
/// [`vtium::head::Start::trusted`] reads it (ADR-1401 D7, ADR-1403 D3).
fn head_file(text: &str) -> Result<String, String> {
    let path = std::path::Path::new(vtium_settings::CONF_FILE);
    vtium::head::Start::trusted(path, text.as_bytes())
        .notice()
        .ok_or_else(|| format!("the file {text:?} shows no notice"))
}
