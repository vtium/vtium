//! The commands of the scenario language (ADR-1504 D2), each read from
//! its line and turned into what it adds to the replay input: the
//! program's bytes, or the interface's test commands.

use std::collections::BTreeSet;

use regex::bytes::Regex as BytesRegex;

use super::{Command, Dump, Item, Scenario, keys, quoted, strings, unquote};
use crate::probes::Step;
use crate::replay::{COLS_MAX, ROWS_MAX};
use crate::spec::whole;

/// A decimal number from `min` to `max` with no sign or leading zero.
fn number(text: &str, min: u64, max: u64) -> Option<u64> {
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|n| (min..=max).contains(n))
}

/// `COL,ROW` or `X,Y`, each from 0 to `max`.
fn pair(text: &str, max: u64) -> Option<(u64, u64)> {
    let (a, b) = text.split_once(',')?;
    Some((number(a, 0, max)?, number(b, 0, max)?))
}

/// Lowercase hex, two digits a byte.
fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

/// The item one line after the header makes, or None for a line that only
/// sets something.
pub(super) fn line_item(
    s: &mut Scenario,
    at: &str,
    n: usize,
    word: &str,
    rest: &str,
) -> Result<Option<Item>, String> {
    // the assertions and the placeholder take the rest of the line whole,
    // `#` included, since a view's line may hold one
    let raw = match word {
        "want" | "reject" => Some(Step::Want(rest.to_owned())),
        "want-re" | "reject-re" => {
            let re = whole(rest).map_err(|e| {
                format!("{at}: The regular expression after `{word}` does not compile: {e}")
            })?;
            Some(if word == "want-re" {
                Step::WantRe(re)
            } else {
                Step::RejectRe(re)
            })
        }
        "placeholder" => {
            if rest.trim().is_empty() {
                return Err(format!(
                    "{at}: This placeholder gives no reason. A placeholder says what vtium lacks, as in `placeholder vtium has no kitty keyboard protocol`."
                ));
            }
            s.placeholder = Some(rest.trim().to_owned());
            return Ok(None);
        }
        _ => None,
    };
    if let Some(step) = raw {
        if rest.is_empty() {
            return Err(format!(
                "{at}: This `{word}` line is empty. It takes the line it expects after a space."
            ));
        }
        let step = match (word, step) {
            ("reject", Step::Want(line)) => Step::Reject(line),
            (_, step) => step,
        };
        return Ok(Some(Item::Assert(step)));
    }
    let words = strings::words(&format!("{word} {rest}")).map_err(|e| format!("{at}: {e}"))?;
    let args: Vec<&str> = words.iter().skip(1).map(String::as_str).collect();
    let echo = words.join(" ");
    let mut c = Command::new(n, echo);
    let string = |arg: Option<&&str>| -> Result<Vec<u8>, String> {
        arg.and_then(|a| unquote(a)).ok_or_else(|| {
            format!(r#"{at}: `{word}` takes one string in double quotes, as in `{word} "\e[?1h"`. Inside it, \e, \a, \b, \t, \n, \r, \\, \" , \xHH and \u{{HEX}} stand for bytes."#)
        })
    };
    let only = |count: usize, form: &str| -> Result<(), String> {
        if args.len() == count {
            Ok(())
        } else {
            Err(format!(
                "{at}: This `{word}` line has {} words after the command. It reads `{form}`.",
                args.len()
            ))
        }
    };
    match word {
        "size" | "resize" => {
            only(2, &format!("{word} COLS ROWS"))?;
            let (Some(cols), Some(rows)) = (
                number(args[0], 1, COLS_MAX as u64),
                number(args[1], 1, ROWS_MAX as u64),
            ) else {
                return Err(format!(
                    "{at}: The size {} by {} is outside the replay's bounds. A screen is 1 to {COLS_MAX} columns by 1 to {ROWS_MAX} rows.",
                    args[0], args[1]
                ));
            };
            if word == "size" {
                s.cols = usize::try_from(cols).unwrap_or(1);
                s.rows = usize::try_from(rows).unwrap_or(1);
            } else {
                c.tests.push(format!("resize;c={cols};r={rows}"));
            }
        }
        "view" => {
            only(1, "view screen|frame|display")?;
            s.view = match args[0] {
                "screen" => Dump::Screen,
                "frame" => Dump::Frame,
                "display" => Dump::Display,
                v => {
                    return Err(format!(
                        "{at}: `{v}` is not a view. The views are screen, frame and display."
                    ));
                }
            };
        }
        "needs" => {
            if args.is_empty() {
                return Err(format!(
                    "{at}: This `needs` line names no feature. It reads `needs FEATURE...`, with the interface's feature names."
                ));
            }
            s.needs.extend(args.iter().map(|f| (*f).to_owned()));
        }
        "fixture" => {
            only(1, "fixture NAME")?;
            s.fixtures.push(args[0].to_owned());
        }
        "send" => {
            only(1, r#"send "...""#)?;
            c.send = string(args.first())?;
        }
        "key" => c.tests.push(key(at, &args)?),
        "type" => {
            only(1, r#"type "...""#)?;
            let text = String::from_utf8(string(args.first())?)
                .map_err(|_| format!("{at}: The text after `type` is not UTF-8. `type` types characters; `send` writes any bytes."))?;
            for ch in text.chars() {
                if ch.is_control() {
                    return Err(format!(
                        "{at}: The text after `type` holds the control character {}. `type` presses one key for each printable character; press Return, Tab or Escape with `key`.",
                        quoted(ch.to_string().as_bytes())
                    ));
                }
                let sym = keys::keysym_of_char(ch);
                c.tests
                    .push(key_text(sym, keys::ascii_of(sym), Some(ch), 0));
            }
        }
        "mouse" => c.tests.push(mouse(at, &args)?),
        "wheel" => {
            let up = match args.first() {
                Some(&"up") => true,
                Some(&"down") => false,
                _ => {
                    return Err(format!(
                        "{at}: `wheel` takes `up` or `down`, then `at COL,ROW`, as in `wheel down at 3,1`."
                    ));
                }
            };
            let p = position(at, word, args.get(1..).unwrap_or_default())?;
            let button = if up { "wheel-up" } else { "wheel-down" };
            c.tests.push(mouse_text(&p, button, "press"));
        }
        "paste" => {
            only(1, r#"paste "...""#)?;
            let data = string(args.first())?;
            if data.is_empty() || data.contains(&0) {
                return Err(format!(
                    "{at}: The text after `paste` is empty or holds the byte 00. The interface's `paste` carries at least one byte and never 00."
                ));
            }
            c.tests.push(format!("paste;hex={}", hex(&data)));
        }
        "focus" => {
            only(1, "focus in|out")?;
            let on = match args[0] {
                "in" => 1,
                "out" => 0,
                _ => return Err(format!("{at}: `focus` takes `in` or `out`.")),
            };
            c.tests.push(format!("focus;in={on}"));
        }
        "select" => c.tests.push(select(at, &args)?),
        "copy" => {
            only(0, "copy")?;
            c.tests.push("copy".to_owned());
        }
        "clipboard" => {
            only(0, "clipboard")?;
            c.tests.push("clipboard".to_owned());
        }
        "scroll" => {
            let text = match args[..] {
                ["up", n] | ["down", n] => {
                    let n = number(n, 1, u64::from(u32::MAX)).ok_or_else(|| {
                        format!("{at}: `{n}` is not a count of lines. `scroll up N` and `scroll down N` move the view N lines, 1 or more.")
                    })?;
                    format!("sb-{};n={n}", args[0])
                }
                ["bottom"] => "sb-reset".to_owned(),
                ["prompt-up"] => "prompt;dir=up".to_owned(),
                ["prompt-down"] => "prompt;dir=down".to_owned(),
                _ => {
                    return Err(format!(
                        "{at}: This `scroll` line reads none of the forms `scroll up N`, `scroll down N`, `scroll bottom`, `scroll prompt-up` and `scroll prompt-down`."
                    ));
                }
            };
            c.tests.push(text);
        }
        "scrollback" => {
            only(1, "scrollback N")?;
            let n = number(args[0], 0, u64::from(u32::MAX)).ok_or_else(|| {
                format!("{at}: `{}` is not a number of lines. `scrollback N` sets the most lines the scrollback holds.", args[0])
            })?;
            c.tests.push(format!("sb-max;n={n}"));
        }
        "palette" => {
            only(1, "palette NAME")?;
            if !vtium_term::PALETTE_NAMES.contains(&args[0]) {
                return Err(format!(
                    "{at}: `{}` is not a palette vtium has. The palettes are {}.",
                    args[0],
                    vtium_term::PALETTE_NAMES.join(", ")
                ));
            }
            c.tests.push(format!("palette;name={}", args[0]));
        }
        "backspace" => {
            only(1, "backspace bs|del")?;
            let del = match args[0] {
                "bs" => 0,
                "del" => 1,
                _ => return Err(format!("{at}: `backspace` takes `bs` or `del`.")),
            };
            c.tests.push(format!("backspace;del={del}"));
        }
        "cmd" => {
            only(1, "cmd TEXT")?;
            c.tests.push(args[0].to_owned());
        }
        "dump" => {
            for a in &args {
                let d = match *a {
                    "screen" => Dump::Screen,
                    "frame" => Dump::Frame,
                    "display" => Dump::Display,
                    "state" => Dump::State,
                    _ => {
                        return Err(format!(
                            "{at}: `dump` prints the views screen, frame and display, and the state; `{a}` is none of them."
                        ));
                    }
                };
                if c.dump.contains(&d) {
                    return Err(format!("{at}: This `dump` names {a} twice."));
                }
                c.dump.push(d);
            }
            if c.dump.is_empty() {
                c.dump.push(Dump::Screen);
            }
            if c.dump.contains(&Dump::State) {
                c.tests.push("state".to_owned());
            }
        }
        "check" => {
            only(0, "check")?;
            return Ok(Some(Item::Check));
        }
        "reply" | "reply-re" | "example" => {
            only(1, &format!(r#"{word} "...""#))?;
            let data = string(args.first())?;
            return Ok(Some(match word {
                "reply" => Item::Assert(Step::Reply(data)),
                "example" => Item::Example(data),
                _ => {
                    let re = bytes_regex(&data).map_err(|e| {
                        format!("{at}: The pattern after `reply-re` does not compile: {e}")
                    })?;
                    Item::Assert(Step::ReplyRe(re, crate::probes::shown(&data)))
                }
            }));
        }
        _ => {
            return Err(format!(
                "{at}: `{word}` is not a command of the scenario language. The README's \"Scenarios\" section lists the commands."
            ));
        }
    }
    Ok(Some(Item::Command(c)))
}

/// A byte pattern, matched whole.
fn bytes_regex(pattern: &[u8]) -> Result<BytesRegex, String> {
    crate::probes::bytes_regex(pattern)
}

/// `key NAME [mods=M+M] [ascii=NAME] [char="c"|none]` as the interface's
/// `key` command.
fn key(at: &str, args: &[&str]) -> Result<String, String> {
    let Some((name, opts)) = args.split_first() else {
        return Err(format!(
            "{at}: `key` names a key, as in `key Up` or `key a mods=ctrl`."
        ));
    };
    let sym = keys::keysym(name).ok_or_else(|| {
        format!("{at}: `{name}` is not a key name. A key is an X11 keysym name such as Up, KP_Enter or F5, one printable ASCII character, or 0x and a keysym in hex.")
    })?;
    let mut ascii = keys::ascii_of(sym);
    let mut ch = keys::char_of(sym);
    let mut mods = 0;
    let mut seen = BTreeSet::new();
    for opt in opts {
        let (k, v) = opt.split_once('=').unwrap_or((opt, ""));
        if !seen.insert(k) {
            return Err(format!("{at}: This `key` line gives {k}= twice."));
        }
        match k {
            "mods" => mods = modifiers(at, v)?,
            "ascii" => {
                ascii = keys::keysym(v).ok_or_else(|| {
                    format!("{at}: `{v}` after ascii= is not a key name. It names the key an ASCII layout has in the same place, as in ascii=c.")
                })?;
            }
            "char" if v == "none" => ch = None,
            "char" => {
                let text = unquote(v).and_then(|b| String::from_utf8(b).ok());
                let mut chars = text.as_deref().unwrap_or_default().chars();
                ch = match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(c),
                    _ => {
                        return Err(format!(
                            r#"{at}: char= takes one character in double quotes, as in char="с", or none."#
                        ));
                    }
                };
            }
            _ => {
                return Err(format!(
                    "{at}: `{opt}` is not an option of `key`. Its options are mods=, ascii= and char=."
                ));
            }
        }
    }
    Ok(key_text(sym, ascii, ch, mods))
}

/// The interface's `key` command: `sym`, then `ascii` and `unicode` when
/// the key has them, then `mods` when any is held.
fn key_text(sym: u32, ascii: u32, ch: Option<char>, mods: u8) -> String {
    let mut text = format!("key;sym={sym:x}");
    if ascii != 0 {
        text.push_str(&format!(";ascii={ascii:x}"));
    }
    if let Some(c) = ch {
        text.push_str(&format!(";unicode={:x}", u32::from(c)));
    }
    if mods != 0 {
        text.push_str(&format!(";mods={mods}"));
    }
    text
}

fn modifiers(at: &str, text: &str) -> Result<u8, String> {
    keys::mods(text).ok_or_else(|| {
        let names: Vec<&str> = keys::MODS.iter().map(|(n, _)| *n).collect();
        format!("{at}: `mods={text}` does not name modifiers. Join each of {} once with +, as in mods=ctrl+shift.", names.join(", "))
    })
}

/// Where a pointer event happens: its cell, its pixel when given, and the
/// modifiers held.
struct Pointer {
    col: u64,
    row: u64,
    px: Option<(u64, u64)>,
    mods: u8,
}

/// `at COL,ROW [px=X,Y] [mods=M+M]`, the place of a pointer event.
fn position(at: &str, word: &str, args: &[&str]) -> Result<Pointer, String> {
    let (Some(&"at"), Some(cell)) = (args.first(), args.get(1)) else {
        return Err(format!(
            "{at}: This `{word}` line has no `at COL,ROW`, the cell under the pointer, as in `at 3,1`."
        ));
    };
    let (col, row) = pair(cell, u64::from(u32::MAX)).ok_or_else(|| {
        format!("{at}: `{cell}` is not a cell. A cell is COL,ROW, each counted from 0.")
    })?;
    let (mut px, mut mods) = (None, 0);
    let mut seen = BTreeSet::new();
    for opt in args.get(2..).unwrap_or_default() {
        let (k, v) = opt.split_once('=').unwrap_or((opt, ""));
        if !seen.insert(k) {
            return Err(format!("{at}: This `{word}` line gives {k}= twice."));
        }
        match k {
            "px" => {
                px = Some(pair(v, 65535).ok_or_else(|| {
                    format!("{at}: `px={v}` is not a pixel. A pixel is X,Y, each from 0 to 65535, counted from the text area's top left.")
                })?);
            }
            "mods" => mods = modifiers(at, v)?,
            _ => {
                return Err(format!(
                    "{at}: `{opt}` is not an option of `{word}`. Its options are px= and mods=."
                ));
            }
        }
    }
    Ok(Pointer { col, row, px, mods })
}

/// The interface's `mouse` command, with `px` and `py` when given.
fn mouse_text(p: &Pointer, button: &str, event: &str) -> String {
    let mut text = format!("mouse;col={};row={}", p.col, p.row);
    if let Some((x, y)) = p.px {
        text.push_str(&format!(";px={x};py={y}"));
    }
    text.push_str(&format!(";button={button};event={event}"));
    if p.mods != 0 {
        text.push_str(&format!(";mods={}", p.mods));
    }
    text
}

/// `mouse press|release BUTTON at ...` and `mouse move [BUTTON] at ...`.
fn mouse(at: &str, args: &[&str]) -> Result<String, String> {
    const BUTTONS: [&str; 3] = ["left", "middle", "right"];
    let (event, button, rest) = match args {
        [e @ ("press" | "release"), b, rest @ ..] if BUTTONS.contains(b) => (*e, *b, rest),
        ["move", b, rest @ ..] if BUTTONS.contains(b) => ("move", *b, rest),
        ["move", rest @ ..] => ("move", "none", rest),
        _ => {
            return Err(format!(
                "{at}: This `mouse` line reads neither `mouse press|release BUTTON at COL,ROW` nor `mouse move [BUTTON] at COL,ROW`. A button is left, middle or right; the wheel is `wheel up|down`."
            ));
        }
    };
    Ok(mouse_text(&position(at, "mouse", rest)?, button, event))
}

/// `select start|to|word|line|output at COL,ROW` and `select clear`.
fn select(at: &str, args: &[&str]) -> Result<String, String> {
    match args {
        ["clear"] => Ok("select;how=clear".to_owned()),
        [
            how @ ("start" | "to" | "word" | "line" | "output"),
            "at",
            cell,
        ] => {
            let (col, row) = pair(cell, u64::from(u32::MAX)).ok_or_else(|| {
                format!("{at}: `{cell}` is not a cell. A cell is COL,ROW, each counted from 0.")
            })?;
            Ok(format!("select;how={how};col={col};row={row}"))
        }
        _ => Err(format!(
            "{at}: This `select` line reads neither `select start|to|word|line|output at COL,ROW` nor `select clear`."
        )),
    }
}
