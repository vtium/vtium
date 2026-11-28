//! The live console's two sheets over the viewer: the trial's countdown,
//! which stays small so the preview stays in sight, and the question
//! before a system font is installed, which shows the exact command
//! before it runs. The view draws them after the open tab and before an
//! open menu. Each line is cut to the sheet's width, and the command runs
//! on over as many lines as it needs.
//!
//! References:
//! - ADR-1201 D4, "The live trial, on a key": the countdown.
//! - ADR-1205 D5: the question before a font of the person's own is
//!   installed shows the exact command first.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_widgets::menu::sheet;
use vtium_config_widgets::roles::Role;

use super::words;
use crate::app::App;

/// The widest a sheet is drawn.
const WIDEST: u16 = 94;

/// Draws the sheet that `app` has open, if any, inside `viewer`, above its
/// last row, which holds the message or the question.
pub fn draw(app: &App, buf: &mut Buffer, viewer: Rect, color: bool) {
    let within = Rect {
        height: viewer.height.saturating_sub(1),
        ..viewer
    };
    let w = within.width.saturating_sub(4).min(WIDEST);
    // inside the borders, a cell in from each side
    let inner = usize::from(w.saturating_sub(4)).max(1);
    if let Some(t) = &app.live().trial {
        let line = words::countdown(&t.what, t.left);
        let mut lines = wrapped(&line.line, inner, Role::Heading);
        lines.extend(wrapped(&line.more.unwrap_or_default(), inner, Role::Dim));
        // under the tab's list, above the message row, so the preview
        // stays in sight
        let h = rows(&lines).min(within.height);
        let area = Rect::new(
            within.x.saturating_add(within.width.saturating_sub(w) / 2),
            within.bottom().saturating_sub(h),
            w,
            h,
        );
        show(buf, area, words::TRIAL_TITLE, &lines, color);
        return;
    }
    let asking = app
        .nav()
        .question()
        .is_some_and(|q| q.then.row == super::INSTALL_FONT);
    if let (true, Some(ready)) = (asking, &app.live().font) {
        let mut lines = wrapped(&words::system_font_copy(ready), inner, Role::Row);
        lines.push((String::new(), Role::Row));
        // the command runs on over as many lines as it needs, unbroken by
        // words, so it reads as it would be typed
        let command: Vec<char> = ready.command.chars().collect();
        for piece in command.chunks(inner.saturating_sub(2).max(1)) {
            lines.push((
                format!("  {}", piece.iter().collect::<String>()),
                Role::Heading,
            ));
        }
        lines.push((String::new(), Role::Row));
        lines.extend(wrapped(words::SYSTEM_FONT_NOW, inner, Role::Dim));
        let area = sheet::centered(within, w, rows(&lines));
        show(buf, area, words::SYSTEM_FONT_TITLE, &lines, color);
    }
}

/// The height of a sheet holding `lines`, its borders included.
fn rows(lines: &[(String, Role)]) -> u16 {
    u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
}

/// `text` broken between words into lines of at most `width` characters,
/// each in `role`. A word longer than a line is cut where the line ends.
fn wrapped(text: &str, width: usize, role: Role) -> Vec<(String, Role)> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let fits = line.chars().count() + 1 + word.chars().count() <= width;
        if !line.is_empty() && !fits {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines.into_iter().map(|l| (l, role)).collect()
}

/// A sheet titled `title` in `area`, holding `lines`, each in its role.
fn show(buf: &mut Buffer, area: Rect, title: &str, lines: &[(String, Role)], color: bool) {
    let frame = sheet::layout(area, title, false);
    frame.draw(buf, color);
    let inner = frame.inner();
    for (i, (line, role)) in lines.iter().enumerate() {
        let Some(y) = u16::try_from(i).ok().and_then(|i| inner.y.checked_add(i)) else {
            break;
        };
        if y >= inner.bottom() {
            break;
        }
        buf.set_stringn(
            inner.x,
            y,
            line,
            usize::from(inner.width),
            role.style(color).cell(),
        );
    }
}
