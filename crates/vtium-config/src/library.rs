//! The library (ADR-1403 D8): looks, heads and rules the user keeps
//! outside any draft, in `library.conf`, written in vtium.conf's section
//! syntax so one parser reads a node in the library and in a draft. A node
//! is copied when it moves, either way, through the section writer, so the
//! library is written as a draft is.

use vtium_settings::{Config, Of, WhenDisplayLost, add_section};

use crate::outcome::Fail;

/// What survives of `text`'s sections, as vtium reads them, so a draft or
/// a library with errors elsewhere still gives its nodes.
fn config(text: &str) -> Result<Config, Fail> {
    Ok(vtium_settings::sections(text.as_bytes())
        .map(|r| r.parsed.config)
        .unwrap_or_default())
}

/// The keys the section `of` sets in `config`, each with its value as the
/// file writes it; `None` when there is no such section.
pub fn keys(config: &Config, of: Of<'_>) -> Option<Vec<(String, String)>> {
    match of {
        Of::Look(name) => {
            let look = config.looks.iter().find(|l| l.name == name)?;
            let mut out: Vec<(String, String)> = look
                .settings
                .iter()
                .map(|s| (s.key.name.to_owned(), s.value.clone()))
                .collect();
            if let Some(v) = &look.vtlook {
                out.push(("vtlook".to_owned(), v.clone()));
            }
            Some(out)
        }
        Of::Head(name) => {
            let head = config.head(name)?;
            let mut out = Vec::new();
            if head.look_line > 0 {
                out.push(("look".to_owned(), head.look.clone()));
            }
            if head.consoles_line > 0 {
                out.push(("consoles".to_owned(), head.consoles.to_string()));
            }
            if let Some(login) = &head.login {
                out.push(("login".to_owned(), login.clone()));
            }
            match head.when_display_lost {
                WhenDisplayLost::Keep => {}
                WhenDisplayLost::Hangup => {
                    out.push(("when-display-lost".to_owned(), "hangup".to_owned()));
                }
                WhenDisplayLost::HangupAfter(s) => {
                    out.push(("when-display-lost".to_owned(), format!("hangup {s}")));
                }
            }
            if let Some(isolated) = head.isolated {
                out.push((
                    "isolated".to_owned(),
                    if isolated { "yes" } else { "no" }.to_owned(),
                ));
            }
            // the VTs the head runs on, none for every VT (ADR-1403 D4)
            if !head.vts.is_empty() {
                let vts: Vec<String> = head.vts.iter().map(u16::to_string).collect();
                out.push(("vt".to_owned(), vts.join(" ")));
            }
            // `switch-on-enter` is `no` unless the file turns it on
            if head.switch_on_enter {
                out.push(("switch-on-enter".to_owned(), "yes".to_owned()));
            }
            Some(out)
        }
    }
}

/// `to` with the node `of` copied from `from`: the section made where `to`
/// has none, each key `from` sets set to its value, and each key only `to`
/// sets unset, so the copy is the same node.
pub fn copy(from: &str, to: &str, of: Of<'_>) -> Result<String, Fail> {
    let name = match of {
        Of::Head(n) | Of::Look(n) => n,
    };
    let source = keys(&config(from)?, of)
        .ok_or_else(|| Fail::Usage(format!("there is no section named {name} to copy")))?;
    let mut out = to.to_owned();
    let existing = match keys(&config(&out)?, of) {
        Some(keys) => keys,
        None => {
            out = add_section(&out, of).map_err(|e| Fail::Usage(e.to_string()))?;
            Vec::new()
        }
    };
    // a key the section lacks goes directly below its heading, so the
    // source's keys are set last first to keep their order
    let mut changes: Vec<(&str, Option<&str>)> = source
        .iter()
        .rev()
        .map(|(k, v)| (k.as_str(), Some(v.as_str())))
        .collect();
    for (k, _) in &existing {
        if !source.iter().any(|(s, _)| s == k) {
            changes.push((k.as_str(), None));
        }
    }
    crate::ops::section_keys(&out, of, &changes)
}

/// Whether the draft's node `of` differs from the library's, which a card
/// marks; `false` when either has no such node.
pub fn differs(draft: &str, library: &str, of: Of<'_>) -> bool {
    match (config(draft), config(library)) {
        (Ok(d), Ok(l)) => match (keys(&d, of), keys(&l, of)) {
            (Some(mut a), Some(mut b)) => {
                a.sort();
                b.sort();
                a != b
            }
            _ => false,
        },
        _ => false,
    }
}

/// The library's nodes, one line each: `look NAME`, `head NAME`, and
/// `rule N` with the rule's line.
pub fn list(library: &str) -> Vec<String> {
    let config = config(library).unwrap_or_default();
    let mut out: Vec<String> = config
        .looks
        .iter()
        .map(|l| format!("look {}", l.name))
        .collect();
    out.extend(config.heads.iter().map(|h| format!("head {}", h.name)));
    for (i, n) in vtium_settings::rule_lines(library).iter().enumerate() {
        let line = library
            .lines()
            .nth(n.saturating_sub(1))
            .unwrap_or_default()
            .trim();
        out.push(format!("rule {}\t{line}", i.saturating_add(1)));
    }
    out
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn a_look_is_copied_whole_and_marked_when_it_differs() {
        let draft = "version = 1\n[look big]\nfont-name = monoid\nfont-size = 14\n";
        let library = copy(draft, "", Of::Look("big")).unwrap();
        assert_eq!(
            library,
            "version = 1\n\n[look big]\nfont-name = monoid\nfont-size = 14\n"
        );
        assert!(!differs(draft, &library, Of::Look("big")));
        let changed = "version = 1\n[look big]\nfont-name = dejavu\n";
        assert!(differs(changed, &library, Of::Look("big")));
        let pulled = copy(changed, &library, Of::Look("big")).unwrap();
        assert_eq!(list(&pulled), ["look big"]);
        assert!(!differs(changed, &pulled, Of::Look("big")));
    }

    // a head goes to the library and back with every key it sets, its VTs
    // and switch-on-enter among them (ADR-1403 D4, D8; ADR-1401, "Enter
    // on a head's own keyboard")
    #[test]
    fn a_head_is_copied_with_its_vts_and_switch_on_enter() {
        let draft = "version = 1\n[head desk]\nconsoles = 2\nvt = 3 4\nswitch-on-enter = yes\n[rules]\ndisplay * -> desk\n";
        let library = copy(draft, "", Of::Head("desk")).unwrap();
        assert_eq!(
            library,
            "version = 1\n\n[head desk]\nconsoles = 2\nvt = 3 4\nswitch-on-enter = yes\n"
        );
        assert!(!differs(draft, &library, Of::Head("desk")));
        // pulled back into a draft whose head sets neither key
        let bare = "version = 1\n[head desk]\nconsoles = 2\n[rules]\ndisplay * -> desk\n";
        assert!(differs(bare, &library, Of::Head("desk")));
        // a key the head lacks goes in below its heading
        let pulled = copy(&library, bare, Of::Head("desk")).unwrap();
        assert_eq!(
            pulled,
            "version = 1\n[head desk]\nvt = 3 4\nswitch-on-enter = yes\nconsoles = 2\n[rules]\ndisplay * -> desk\n"
        );
        assert!(!differs(draft, &pulled, Of::Head("desk")));
        // and a head that sets neither takes both out of the library's
        let cleared = copy(bare, &library, Of::Head("desk")).unwrap();
        assert_eq!(cleared, "version = 1\n\n[head desk]\nconsoles = 2\n");
    }
}
