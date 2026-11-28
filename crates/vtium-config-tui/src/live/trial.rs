//! The live trial: the console shows a font, or a look's font set, for ten
//! seconds. `k` keeps it for the session, and any other key, a click, or
//! the countdown's end puts back the font the console showed before. The
//! trial is a preview: it changes nothing in the document.
//!
//! A tab answers `t` with the font set or the look its highlight is on
//! ([`Change::Try`](vtium_config_core::ui::Change::Try) and
//! [`Change::TryLook`](vtium_config_core::ui::Change::TryLook)), and update
//! hands it here. Where the terminal cannot switch its font, the message
//! line says why and nothing is sent.
//!
//! References:
//! - ADR-1201 D4, "The live trial, on a key": `t`, the countdown of ten
//!   one-second waits, `k`, and any other key putting the font back and
//!   doing nothing else.
//! - ADR-1403 D11, "One flow for every change": the live trial is a
//!   preview, never a change.

use std::time::Duration;

use vtium_config_core::ui::{Effect, Shared};
use vtium_config_widgets::keys::Key;

use super::{State, follow, words};

/// How many one-second waits the countdown takes.
pub const TICKS: u8 = 10;

/// One second of the countdown.
pub const TICK: Duration = Duration::from_secs(1);

/// A trial that counts down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trial {
    /// The font the console tries: a font set's name, a built-in font's,
    /// or a font file's path.
    pub font: String,
    /// What the countdown names: the font, or the look and its font set.
    pub what: String,
    /// The font the console showed before, which the trial puts back.
    pub before: String,
    /// The seconds left.
    pub left: u8,
}

/// What a key does while a trial counts down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keyed {
    /// `k` kept the font for the session.
    Kept,
    /// The key put the font back and does nothing else; the effects send
    /// the switch back.
    PutBack(Vec<Effect>),
    /// Ctrl+C quits, and leaving gives the console its start font back.
    Quit,
}

/// `t` on the font `font`: switches the console to it and starts the
/// countdown, where the console can be switched.
pub fn try_font(shared: &mut Shared, state: &mut State, font: String) -> Vec<Effect> {
    let what = words::trial_of(&font, None);
    begin(shared, state, font, what)
}

/// `t` on the look `look`: tries its font set, or the font vtium.conf
/// names where the look names none. The look's size, color scheme and
/// vtlook take effect when vtium starts with them.
pub fn try_look(shared: &mut Shared, state: &mut State, look: &str) -> Vec<Effect> {
    if let Some(off) = words::off(shared.console.live) {
        shared.message = Some(off);
        return Vec::new();
    }
    let text = shared.document().text();
    let flow = vtium_config_core::flowmodel::build(text, &shared.present, None);
    let font = flow
        .looks
        .iter()
        .find(|l| l.name == look)
        .map(|l| l.frit.clone())
        .filter(|f| !f.is_empty())
        .or_else(|| vtium_settings::setting(text, "font-name"));
    let Some(font) = font else {
        shared.message = Some(words::look_has_no_font(look));
        return Vec::new();
    };
    let what = words::trial_of(&font, Some(look));
    begin(shared, state, font, what)
}

/// Starts the trial of `font`, named `what` on the countdown.
fn begin(shared: &mut Shared, state: &mut State, font: String, what: String) -> Vec<Effect> {
    if let Some(off) = words::off(shared.console.live) {
        shared.message = Some(off);
        return Vec::new();
    }
    if shared.console.refused.contains(&font) {
        shared.message = Some(words::refused(&font));
        return Vec::new();
    }
    let console = &shared.console;
    let before = console
        .want
        .clone()
        .or_else(|| console.shown_font.clone())
        .unwrap_or_default();
    if before == font {
        shared.message = Some(words::already(&font));
        return Vec::new();
    }
    // the trial's sheet counts down, so the message line stays clear
    shared.message = None;
    shared.console.want = Some(font.clone());
    state.trial = Some(Trial {
        font,
        what,
        before,
        left: TICKS,
    });
    let mut out = follow(shared);
    out.push(Effect::Wait(TICK));
    out
}

/// A key while the trial counts down.
pub fn key(shared: &mut Shared, state: &mut State, key: &Key) -> Keyed {
    if *key == Key::char('k') {
        if let Some(t) = state.trial.take() {
            shared.message = Some(words::kept(&t.font));
        }
        return Keyed::Kept;
    }
    if *key == Key::ctrl('c') {
        return Keyed::Quit;
    }
    Keyed::PutBack(put_back(shared, state))
}

/// Ends the trial and gives the console the font it showed before. While
/// the trial's own switch is in flight, the switch back goes once the
/// console answers it.
pub fn put_back(shared: &mut Shared, state: &mut State) -> Vec<Effect> {
    let Some(t) = state.trial.take() else {
        return Vec::new();
    };
    shared.message = Some(words::put_back(&t.before));
    shared.console.want = Some(t.before);
    follow(shared)
}

/// One second of the countdown: the next wait, or at its end, the font
/// put back.
pub fn tick(shared: &mut Shared, state: &mut State) -> Vec<Effect> {
    let Some(t) = state.trial.as_mut() else {
        return Vec::new();
    };
    t.left = t.left.saturating_sub(1);
    if t.left == 0 {
        return put_back(shared, state);
    }
    vec![Effect::Wait(TICK)]
}
