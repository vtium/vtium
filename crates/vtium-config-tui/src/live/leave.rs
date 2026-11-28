//! Leaving: the console gets back the font it showed when the TUI started,
//! and the TUI waits up to one second for the replies still due, so that no
//! late reply reaches the shell after it exits. A trial that counts down
//! ends here, and a font that `k` kept goes back too.
//!
//! References:
//! - ADR-1201 D4, "Giving the console back": leaving switches the console
//!   back to the font the start query named, then asks the query to check
//!   it, on `q`, Ctrl+C and the signals the loop holds.
//! - ADR-1206 D2, the note of 2026-11-19: the wait of up to one second for
//!   the replies still due when the TUI leaves.
//! - ADR-1201 D11: a switch the TUI could not give back lasts until the
//!   console's session ends.

use vtium_config_core::ui::{Asker, Effect, Live, Shared};

use super::{State, WAIT, queue, switch, words};

/// Leaves with `status`: at once where nothing is due, and otherwise once
/// the console has answered the switch back, or once the wait ends.
pub fn leave(shared: &mut Shared, state: &mut State, status: u8) -> Vec<Effect> {
    if state.leaving.is_some() {
        return Vec::new();
    }
    state.trial = None;
    state.leaving = Some(status);
    let console = &shared.console;
    // the font the console ends on once the switch in flight, if any, is
    // answered: the start font is sent only where it is another
    let last = console.in_flight.as_ref().or(console.shown_font.as_ref());
    let back = last != console.start_font.as_ref();
    match (console.live, console.start_font.clone()) {
        (Live::On, Some(start)) if back => {
            let bytes = queue::send(&mut shared.askers, switch(Asker::Leave, &start));
            shared.console.want = Some(start);
            shared.say(words::LEAVING.to_owned());
            vec![Effect::Write(bytes), Effect::Wait(WAIT)]
        }
        // a reply is still due, such as DA1's after a switch already
        // answered, or the answers at start: the wait keeps them from
        // reaching the shell
        _ if shared.askers.awaiting() => vec![Effect::Wait(WAIT)],
        _ => vec![Effect::Exit(status)],
    }
}
