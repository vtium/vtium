//! A behaviour scenario's transcript (ADR-1504 D3), printed from the
//! blocks the replay printed at its cuts: each command echoed after `> `,
//! its side effects after `< `, and each dump in the views' own lines.

use super::{Dump, Scenario, quoted};
use crate::interface::check_output;
use crate::runner::mask_line;
use crate::spec::Spec;

/// One block of a replay's output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    /// The `test` lines printed before the block's `checkpoint` line,
    /// which belong to it.
    pub tests: Vec<String>,
    /// The `reply` and `host:` lines, and the `bell` lines once the
    /// interface has them, in the order printed.
    pub effects: Vec<String>,
    /// The `size`, `cursor` and `flags` lines and the view's lines.
    pub body: Vec<String>,
}

/// The blocks of a replay's output: one at each `checkpoint` line, or one
/// for an output printed without cuts.
pub fn blocks(output: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut pending = Vec::new();
    let mut sized = false;
    for line in output.lines() {
        if line.starts_with("test ") {
            pending.push(line.to_owned());
            continue;
        }
        let checkpoint = line.starts_with("checkpoint ");
        if checkpoint || (line.starts_with("size ") && (sized || out.is_empty())) {
            out.push(Block {
                tests: core::mem::take(&mut pending),
                ..Block::default()
            });
            sized = false;
        }
        if checkpoint {
            continue;
        }
        sized |= line.starts_with("size ");
        if let Some(b) = out.last_mut() {
            if ["reply ", "host: ", "bell "]
                .iter()
                .any(|p| line.starts_with(p))
            {
                b.effects.push(line.to_owned());
            } else {
                b.body.push(line.to_owned());
            }
        }
    }
    out
}

/// The bytes of a `reply HEX` line's hex.
fn unhex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .filter_map(|p| u8::from_str_radix(core::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

/// A `test` line as a side effect: what `copy` copied, what `clipboard`
/// took, a refusal, or any other test line as the interface prints it.
fn test_effect(line: &str) -> String {
    let rest = line.strip_prefix("test ").unwrap_or(line);
    for kind in ["clipboard", "copy"] {
        if let Some(v) = rest.strip_prefix(kind).and_then(|r| r.strip_prefix(' ')) {
            return match v {
                "none" => format!("< {kind} none"),
                hex => format!("< {kind} {}", quoted(&unhex(hex))),
            };
        }
    }
    let words: Vec<&str> = rest.split(' ').collect();
    if let [_, "error", n] = words[..] {
        return format!("< error {n}");
    }
    format!("< {}", mask_line(line))
}

/// The side effects of one command, from the blocks between the cut before
/// it and its own: its `test` lines, then its replies, host requests and
/// bells in the order printed, each run of replies as one string. With
/// `state`, the `test state` lines are the dump's and are left out.
fn effects(blocks: &[&Block], state: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut run: Vec<u8> = Vec::new();
    let flush = |run: &mut Vec<u8>, out: &mut Vec<String>| {
        if !run.is_empty() {
            out.push(format!("< reply {}", quoted(run)));
            run.clear();
        }
    };
    for b in blocks {
        for t in &b.tests {
            if state && t.starts_with("test state") {
                continue;
            }
            flush(&mut run, &mut out);
            out.push(test_effect(t));
        }
        for e in &b.effects {
            if let Some(hex) = e.strip_prefix("reply ") {
                run.extend(unhex(hex));
            } else {
                flush(&mut run, &mut out);
                out.push(format!("< {e}"));
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The transcript of `s` from its replays. `tags` gives, for each cut the
/// replays were cut at, the index of the command whose cut it is, or None
/// for an extra cut; `runs` gives each view's output, the first being the
/// view whose side effects the transcript prints.
pub fn print(
    s: &Scenario,
    tags: &[Option<usize>],
    runs: &[(Dump, &str)],
) -> Result<String, String> {
    let commands: Vec<_> = s.commands().collect();
    let want = tags.len().saturating_add(1);
    let mut views = Vec::new();
    for (view, output) in runs {
        let b = blocks(output);
        if b.len() != want {
            return Err(format!(
                "The replay in the {} view printed {} blocks, where the scenario's cuts make {want}.",
                view.name(),
                b.len()
            ));
        }
        views.push((*view, b));
    }
    let Some((_, first)) = views.first() else {
        return Err("The scenario was played in no view.".to_owned());
    };
    // the block at each command's cut
    let mut at = vec![None; commands.len()];
    for (j, tag) in tags.iter().enumerate() {
        if let Some(i) = *tag
            && let Some(slot) = at.get_mut(i)
        {
            *slot = Some(j);
        }
    }
    let mut out = String::new();
    let mut from = 0;
    for (i, c) in commands.iter().enumerate() {
        let Some(j) = at[i] else {
            return Err(format!("The command `{}` has no cut.", c.echo));
        };
        out.push_str("> ");
        out.push_str(&c.echo);
        out.push('\n');
        let mine: Vec<&Block> = first.iter().take(j.saturating_add(1)).skip(from).collect();
        let state = c.dump.contains(&Dump::State);
        for line in effects(&mine, state) {
            out.push_str(&line);
            out.push('\n');
        }
        for d in &c.dump {
            if *d == Dump::State {
                for b in &mine {
                    for t in b.tests.iter().filter(|t| t.starts_with("test state")) {
                        out.push_str(&mask_line(t));
                        out.push('\n');
                    }
                }
                continue;
            }
            let Some((_, b)) = views.iter().find(|(v, _)| v == d) else {
                return Err(format!(
                    "The dump `{}` asks for the {} view, which was not played.",
                    c.echo,
                    d.name()
                ));
            };
            for line in &b[j].body {
                out.push_str(&mask_line(line));
                out.push('\n');
            }
        }
        from = j.saturating_add(1);
    }
    Ok(out)
}

/// A transcript split at its `> ` lines: each command's line and the lines
/// after it.
fn sections(text: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    for line in text.lines() {
        if line.starts_with("> ") || out.is_empty() {
            out.push(Vec::new());
        }
        if let Some(s) = out.last_mut() {
            s.push(line);
        }
    }
    out
}

/// Where `got` differs from `want`, the transcript on file, or None: the
/// command after which they first differ, and the lines of its section
/// that differ, the expected beside the printed.
pub fn differs(want: &str, got: &str) -> Option<String> {
    if want == got {
        return None;
    }
    let (w, g) = (sections(want), sections(got));
    let k = w
        .iter()
        .zip(&g)
        .position(|(a, b)| a != b)
        .unwrap_or(w.len().min(g.len()));
    let (ws, gs) = (w.get(k), g.get(k));
    let command = |s: Option<&Vec<&str>>| {
        s.and_then(|s| s.first())
            .and_then(|l| l.strip_prefix("> "))
            .map(str::to_owned)
    };
    let mut out = match (command(ws), command(gs)) {
        (Some(a), Some(b)) if a == b => format!("The transcript differs after `{a}`."),
        (Some(a), Some(b)) => format!("The transcript has `{a}` where the scenario now has `{b}`."),
        (Some(a), None) => {
            format!("The transcript goes on with `{a}` after the scenario's last command.")
        }
        (None, Some(b)) => format!("The transcript ends before the command `{b}`."),
        (None, None) => "The transcript differs before its first command.".to_owned(),
    };
    let (ws, gs) = (
        ws.cloned().unwrap_or_default(),
        gs.cloned().unwrap_or_default(),
    );
    let mut shown = 0;
    for idx in 1..ws.len().max(gs.len()) {
        let (a, b) = (ws.get(idx), gs.get(idx));
        if a == b {
            continue;
        }
        if shown == 8 {
            out.push_str("\n    ...");
            break;
        }
        shown += 1;
        out.push_str(&format!(
            "\n    want {}\n    got  {}",
            a.copied().unwrap_or("(nothing)"),
            b.copied().unwrap_or("(nothing)")
        ));
    }
    Some(out)
}

/// The first fault of a transcript on file under the interface's grammar,
/// or None: each dump's lines, view by view, read as the interface check
/// reads a golden's block at the scenario's size, and every other line a
/// command or a side effect.
pub fn check(spec: &Spec, s: &Scenario, text: &str) -> Option<String> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Some("The transcript does not end in a newline.".to_owned());
    }
    for section in sections(text) {
        let Some(head) = section.first().and_then(|l| l.strip_prefix("> ")) else {
            return Some(format!(
                "The transcript starts with {:?}, not a command after `> `.",
                section.first().copied().unwrap_or_default()
            ));
        };
        let mut words: Vec<&str> = head.split(' ').collect();
        let rest: Vec<&str> = section.iter().skip(1).copied().collect();
        let dump_lines: Vec<&str> = rest
            .iter()
            .copied()
            .filter(|l| !l.starts_with("< "))
            .collect();
        if words.first() != Some(&"dump") {
            if let Some(l) = dump_lines.first() {
                return Some(format!(
                    "After `{head}`, the line {l:?} is neither a side effect after `< ` nor part of a dump."
                ));
            }
            continue;
        }
        words.remove(0);
        if words.is_empty() {
            words.push("screen");
        }
        let views: Vec<&str> = words.into_iter().filter(|w| *w != "state").collect();
        // each view's block starts at its `size` line; the state's lines
        // follow the views
        let mut parts: Vec<Vec<&str>> = Vec::new();
        let mut state = Vec::new();
        for l in &dump_lines {
            if l.starts_with("test state") {
                state.push(*l);
            } else if l.starts_with("size ") || parts.is_empty() {
                parts.push(vec![*l]);
            } else if let Some(p) = parts.last_mut() {
                p.push(*l);
            }
        }
        if parts.len() != views.len() {
            return Some(format!(
                "The dump `{head}` holds {} blocks, where it names {} views.",
                parts.len(),
                views.len()
            ));
        }
        for (view, part) in views.iter().zip(&parts) {
            let block = format!("{}\n", part.join("\n"));
            if let Some(fault) = check_output(spec, view, &block, s.cols, s.rows, false) {
                return Some(format!("The {view} view of `{head}`: {fault}"));
            }
        }
        if let Some(l) = state.iter().find(|l| !spec.parses(l, "screen", false)) {
            return Some(format!(
                "The state line {l:?} of `{head}` is not a line of the interface's grammar."
            ));
        }
    }
    None
}
