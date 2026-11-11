//! vtium follows a change to the system's keyboard layout while it runs, in
//! a build with `logind` (ADR-1400's L2 layer). vtium-platform's
//! `LayoutFollow` hears localed announce a new layout over the system bus,
//! works out the layout again in the order vtium uses at its start, and
//! gives the seat's keyboards the new keymap. This module puts its bus
//! connection in the loop, beside the seat's input that it changes.

use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, LoopHandle, Mode, PostAction};
use vtium_platform::input::{Compiled, KeymapConfig};
use vtium_platform::logind::LayoutFollow;

use super::App;

/// Starts following the system's keyboard layout. `own` is vtium.conf's
/// keymap settings, and `applied` what the seat's keymap was compiled from
/// at vtium's start. When vtium cannot watch localed, it logs why and keeps
/// the keymap it started with.
pub fn follow(handle: &LoopHandle<'static, App>, own: KeymapConfig, applied: Compiled) {
    let started = LayoutFollow::open(own, applied)
        .map_err(|e| e.to_string())
        .and_then(|mut follow| {
            let fd = follow.fd().map_err(|e| e.to_string())?;
            handle
                .insert_source(
                    Generic::new(fd, Interest::READ, Mode::Level),
                    move |_, _, app: &mut App| match follow.dispatch(app.input.as_mut()) {
                        Ok(()) => Ok(PostAction::Continue),
                        Err(e) => {
                            log::warn!(target: "input", "vtium stops following the system's keyboard layout, because its connection to the system bus failed: {e}. The keyboard layout stays as it is until vtium starts again.");
                            Ok(PostAction::Remove)
                        }
                    },
                )
                .map_err(|e| e.error.to_string())
        });
    if let Err(e) = started {
        log::info!(target: "input", "vtium does not follow a change to the system's keyboard layout while it runs, because it cannot watch localed on the system bus: {e}. A new layout takes effect when vtium starts again.");
    }
}
