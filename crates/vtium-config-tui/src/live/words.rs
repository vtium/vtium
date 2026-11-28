//! The words the live console's module puts on the screen: the trial's
//! countdown and its end, its refusals, the line while vtium-config gives
//! the console its font back, the results of the Actions menu's items, and
//! the reasons beside a greyed item. Each message is one line at 80
//! columns, and the rest of it moves behind F1.
//!
//! References:
//! - crates/AGENTS.md, "Strings on screen", rules 7 and 8: one line where
//!   a paragraph was, and keys in brackets.
//! - ADR-1201 D4, "The live trial, on a key": the countdown, `k`, and the
//!   font put back.

use vtium_config_core::ui::Message;

/// Try live, greyed in a terminal that is not a vtium console.
pub const NOT_VTIUM: &str = "not a vtium console";

/// Try live, greyed on a vtium console with `allow-font-ops` off.
pub const OPS_OFF: &str = "allow-font-ops is off";

/// Try live, greyed while this terminal has not answered the questions
/// at start.
pub const ASKING: &str = "still asking this terminal";

/// Try live, greyed on a screen with nothing to try.
pub const NOTHING_TO_TRY: &str = "nothing to try here";

/// Install as a system font…, greyed where the highlighted item is not a
/// font file from the person's own folders.
pub const NOT_OWN: &str = "on a font of your own";

/// The line while vtium-config gives the console its font back as it
/// leaves.
pub const LEAVING: &str = "Giving the console its font back…";

/// How a font reads on the screen: a font file by its file's name, and a
/// font set or a built-in font by its name.
pub fn shown(font: &str) -> &str {
    if font.starts_with('/') {
        font.rsplit('/').next().unwrap_or(font)
    } else {
        font
    }
}

/// Why `t` cannot try anything in this terminal, as the message line says
/// it, where it cannot.
pub fn off(live: vtium_config_core::ui::Live) -> Option<Message> {
    use vtium_config_core::ui::Live;
    let (line, more) = match live {
        Live::On => return None,
        Live::Asking => (
            "Try live waits for this terminal's answers. Try again in a moment.",
            "vtium-config asks this terminal what it is when it starts, and waits at most one second for the answers.",
        ),
        Live::NotVtium => (
            "Try live is off: this terminal is not a vtium console. [F1] says more.",
            "Only a vtium console switches its font when vtium-config asks. You can still save fonts and looks, and the next login on the console uses them. Run vtium-config on the vtium console to try a font there.",
        ),
        Live::OpsOff => (
            "Try live is off: allow-font-ops is off. Settings › Running vtium turns it on.",
            "A vtium console switches its font for vtium-config only while allow-font-ops is on. Turn it on in Settings › Running vtium, save, then start vtium and vtium-config again.",
        ),
    };
    Some(Message {
        line: line.to_owned(),
        more: Some(more.to_owned()),
    })
}

/// The countdown of the trial of `what` with `left` seconds to go.
pub fn countdown(what: &str, left: u8) -> Message {
    Message {
        line: format!("Trying {what} on this console: {left} s left."),
        more: Some(
            "[k] keeps it for this session; any other key, or the end of the countdown, puts it back."
                .to_owned(),
        ),
    }
}

/// What a trial is of: a font, or a look and its font set.
pub fn trial_of(font: &str, look: Option<&str>) -> String {
    match look {
        Some(look) => format!("{look} ({})", shown(font)),
        None => shown(font).to_owned(),
    }
}

/// The line after `k` kept the font `font`.
pub fn kept(font: &str) -> Message {
    Message {
        line: format!(
            "The console keeps {} until you quit. To keep it after, choose it and save.",
            shown(font)
        ),
        more: None,
    }
}

/// The line once the console shows `font` again after a trial.
pub fn put_back(font: &str) -> Message {
    Message {
        line: format!("The console shows {} again.", shown(font)),
        more: None,
    }
}

/// The line when the console refused to switch to `font`.
pub fn refused(font: &str) -> Message {
    Message {
        line: format!(
            "The console refused {}: it takes only trusted system fonts. [F1] says more.",
            shown(font)
        ),
        more: Some(
            "The console takes only fonts that root installed, activated and trusts. Its log names the check the font failed. A font activated since you logged in can be tried after the next login."
                .to_owned(),
        ),
    }
}

/// The line when the console already shows the font `t` would try.
pub fn already(font: &str) -> Message {
    Message {
        line: format!("The console already shows {}.", shown(font)),
        more: None,
    }
}

/// The line when the look `look` names no font set and vtium.conf names
/// no font, so a trial would change nothing.
pub fn look_has_no_font(look: &str) -> Message {
    Message {
        line: format!("The look {look} names no font set, so there is nothing to try."),
        more: None,
    }
}

/// The line when `t` asks for a size, which the live trial does not try
/// yet.
pub fn size_not_built() -> Message {
    Message {
        line: "Trying a size live is not built yet. The preview shows it.".to_owned(),
        more: None,
    }
}

/// The line after Read devices again: what came and what went.
pub fn devices(came: &[String], went: &[String]) -> Message {
    let list = |names: &[String]| names.join(", ");
    let line = match (came.is_empty(), went.is_empty()) {
        (true, true) => "Read devices again: nothing came or went.".to_owned(),
        (false, true) => format!("Read devices again: connected {}.", list(came)),
        (true, false) => format!("Read devices again: disconnected {}.", list(went)),
        (false, false) => format!(
            "Read devices again: connected {}; disconnected {}.",
            list(came),
            list(went)
        ),
    };
    Message { line, more: None }
}

/// The line when the devices cannot be read here.
pub fn devices_unread() -> Message {
    Message {
        line: "Read devices again: vtium-config cannot read the devices here.".to_owned(),
        more: None,
    }
}

/// The title of the trial's sheet.
pub const TRIAL_TITLE: &str = "Try live";

/// The title of the sheet before a system font is installed.
pub const SYSTEM_FONT_TITLE: &str = "Install as a system font";

/// The last line of that sheet.
pub const SYSTEM_FONT_NOW: &str =
    "sudo asks for your password. This acts now: it is not a change that waits for Save.";

/// The question on the message row before the font `source` is installed
/// as a system font, while the sheet above it shows the command.
pub fn system_font_question(source: &str) -> String {
    format!("Install {} as a system font?", shown(source))
}

/// The sheet's first line: what the install copies where, and that the
/// command follows.
pub fn system_font_copy(ready: &super::machine::Ready) -> String {
    let source = ready.source.display().to_string();
    let target = ready.target.display();
    match ready.placement {
        vtium_config_core::apply::Placement::Replaces => format!(
            "Replace {target} with {}? vtium-config runs:",
            shown(&source)
        ),
        _ => format!("Copy {} to {target}? vtium-config runs:", shown(&source)),
    }
}

/// The line when the font is already installed with the same bytes.
pub fn system_font_identical(target: &str) -> Message {
    Message {
        line: format!("{target} already holds this font, so nothing needs doing."),
        more: None,
    }
}

/// The line after the install of `target` ended.
pub fn system_font_installed(target: &str) -> Message {
    Message {
        line: format!("Installed {target}. Activate it in Fonts to use it."),
        more: None,
    }
}

/// The line when the install of a system font failed, and why.
pub fn system_font_failed(why: &str) -> Message {
    Message {
        line: "The font was not installed. [F1] says why.".to_owned(),
        more: Some(why.to_owned()),
    }
}
