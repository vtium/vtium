//! `conformance scenarios --play` and `make tier1-play SCENARIO=NAME`: a
//! behaviour scenario played in the reviewer's terminal one command at a
//! time (ADR-1504 D6). Each step shows the command and prints its side
//! effects; a dump draws the screen through the live path
//! ([`crate::live`]), so the terminal the player runs in draws it in its
//! colors and attributes, its pictures included; then the player waits
//! for a key ([`Keys`]).

use std::collections::BTreeMap;
use std::io::{IsTerminal, Read, Write};
use std::process::{Command, Stdio};

use super::Scenario;
use super::run::Runner;
use super::transcript;
use crate::live;
use crate::replay;
use crate::view::View;

/// What a reviewer's key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Quit,
}

/// The reviewer's terminal in the mode the players read keys in, one at a
/// time without echo, put back as it was when dropped. On a standard input
/// that is not a terminal, the player does not wait.
pub struct Keys {
    saved: Option<String>,
}

impl Default for Keys {
    fn default() -> Self {
        Self::new()
    }
}

impl Keys {
    pub fn new() -> Self {
        if !std::io::stdin().is_terminal() {
            return Self { saved: None };
        }
        let stty = |args: &[&str]| {
            Command::new("stty")
                .args(args)
                .stdin(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        };
        let saved = stty(&["-g"]);
        if saved.is_some() {
            let _ = stty(&["-icanon", "-echo", "min", "1"]);
        }
        Self { saved }
    }

    pub fn wait(&self) -> Step {
        if self.saved.is_none() {
            return Step::Next;
        }
        let mut byte = [0u8; 1];
        match std::io::stdin().read(&mut byte) {
            Ok(1) if matches!(byte[0], b'q' | b'Q' | 0x03 | 0x04) => Step::Quit,
            Ok(1) => Step::Next,
            _ => Step::Quit,
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            let _ = Command::new("stty")
                .arg(saved)
                .stdin(Stdio::inherit())
                .status();
        }
    }
}

/// Plays `s` for a reviewer: `on_file` is its transcript on file, if any,
/// `provisional` the reason the provisional list gives, and `wait` waits
/// for the reviewer's key after each command. Returns whether the
/// reviewer played it to its end.
pub fn play(
    runner: &Runner,
    s: &Scenario,
    on_file: Option<&str>,
    provisional: Option<&str>,
    out: &mut dyn Write,
    wait: &mut dyn FnMut() -> Step,
) -> Result<bool, String> {
    let played = runner.play(s, &[])?;
    // the same input and cuts the pictures of `make tier1-shots` come from
    let (input, tags) = runner.input(s, View::Screen, &[]);
    let mut at = Vec::new();
    for (j, tag) in tags.iter().enumerate() {
        if tag.is_some() {
            at.push(j);
        }
    }
    let commands: Vec<_> = s.commands().collect();
    let dumped: Vec<usize> = commands
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.dump.is_empty())
        .filter_map(|(i, _)| at.get(i).copied())
        .collect();
    let mut screens: BTreeMap<usize, (Vec<u8>, usize, usize)> = BTreeMap::new();
    replay::run_with(&input, &mut |block, term| {
        if dumped.contains(&block) {
            let size = (term.screen().cols(), term.screen().rows());
            screens.insert(block, (live::screen(term), size.0, size.1));
        }
    })
    .map_err(|f| format!("the replay exited {}: {}", f.code, f.message))?;
    let mut text = format!("\x1b[1mscenario {}\x1b[0m  {}\n", s.name, s.file);
    text.push_str(&match provisional {
        Some(reason) => format!("provisional: {reason}\n"),
        None => "reviewed: its transcript judges\n".to_owned(),
    });
    for line in &s.header {
        text.push_str(&format!("\x1b[2m{line}\x1b[0m\n"));
    }
    text.push_str(
        &match on_file.map(|f| transcript::differs(f, &played.transcript)) {
            None => "This scenario has no transcript on file yet.\n".to_owned(),
            Some(None) => "The transcript on file matches this run.\n".to_owned(),
            Some(Some(why)) => format!("The transcript on file differs from this run. {why}\n"),
        },
    );
    let emit = |text: &mut String, out: &mut dyn Write| -> Result<(), String> {
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        text.clear();
        Ok(())
    };
    let sections: Vec<Vec<&str>> = {
        let mut v: Vec<Vec<&str>> = Vec::new();
        for line in played.transcript.lines() {
            if line.starts_with("> ") {
                v.push(Vec::new());
            }
            if let Some(last) = v.last_mut() {
                last.push(line);
            }
        }
        v
    };
    for (i, section) in sections.iter().enumerate() {
        let c = commands.get(i);
        text.push_str(&format!(
            "\n\x1b[1m{}\x1b[0m\n",
            section.first().copied().unwrap_or_default()
        ));
        let mut dump = Vec::new();
        for line in section.iter().skip(1) {
            if line.starts_with("< ") {
                text.push_str(&format!("\x1b[36m{line}\x1b[0m\n"));
            } else {
                dump.push(*line);
            }
        }
        let screen = c
            .filter(|c| !c.dump.is_empty())
            .and_then(|_| at.get(i))
            .and_then(|j| screens.get(j));
        if screen.is_some() {
            for line in dump.iter().filter(|l| {
                ["cursor ", "flags ", "test state"]
                    .iter()
                    .any(|p| l.starts_with(p))
            }) {
                text.push_str(&format!("{line}\n"));
            }
        }
        text.push_str("\x1b[2m(any key: next command, q: stop)\x1b[0m");
        let mut bytes = std::mem::take(&mut text).into_bytes();
        if let Some((screen, cols, _)) = screen {
            bytes.extend_from_slice(format!("\r\n{}\r\n", "─".repeat(*cols)).as_bytes());
            bytes.extend_from_slice(screen);
        }
        out.write_all(&bytes).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        let step = wait();
        match screen {
            Some((_, cols, rows)) => {
                text.push_str(&String::from_utf8_lossy(&live::below(*rows)));
                text.push_str(&format!("{}\n", "─".repeat(*cols)));
            }
            None => text.push_str("\r\x1b[2K"),
        }
        if step == Step::Quit {
            text.push('\n');
            emit(&mut text, out)?;
            return Ok(false);
        }
    }
    text.push_str(&format!(
        "\nThe scenario {} ends here. Its assertions: {}\n",
        s.name,
        match &played.assertions {
            crate::probes::Outcome::Pass => "pass".to_owned(),
            crate::probes::Outcome::Fail(why) | crate::probes::Outcome::Unjudged(why) =>
                format!("FAIL, {why}"),
        }
    ));
    emit(&mut text, out)?;
    Ok(true)
}
