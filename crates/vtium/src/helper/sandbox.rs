//! What the console logs of the font helper's sandbox, and the options that
//! turn the sandbox off.

use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::Config;
use crate::config::keys::HELPER_SANDBOX;

use super::wire::OTHER_BUILD;

/// The reason the helper failed, as the console logs it: a helper of an
/// older build's refusal of this console's request says what to do. A
/// helper that does not know the active fonts' request is of an older
/// build too, since the console sends only kinds its own build knows.
pub(super) fn failure(why: &str) -> String {
    match why.trim() {
        "not a serve request" | "a malformed request: an unknown request kind" => {
            OTHER_BUILD.to_owned()
        }
        why => format!("the helper failed: {why}"),
    }
}
/// Why the helper's seccomp filters are not in force, as the byte of its
/// sandbox report at [`SANDBOX_SECCOMP_SHIFT`] numbers it: the setting is
/// off, vtium has no filter for the machine's architecture, or the kernel
/// offers no seccomp filters. `None` for any other number, 0 included,
/// which names no reason vtium knows.
fn seccomp_reason(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "the setting helper-sandbox=off turns them off",
        2 => "vtium has no seccomp filter for this machine's architecture",
        3 => "this kernel is new enough to offer seccomp but was built without it",
        4 => "this kernel was built with seccomp but without seccomp filters",
        5 => "this kernel is older than Linux 3.8, too old for vtium's seccomp filters",
        _ => return None,
    })
}

/// Why the helper's landlock domain is not in force, as the byte of its
/// sandbox report at [`SANDBOX_DOMAIN_SHIFT`] numbers it: the setting is
/// off, or the kernel is too old for landlock, was built without it, or
/// did not turn it on at boot. `None` for any other number, 0 included.
fn domain_reason(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "the setting helper-sandbox=off turns it off",
        2 => "this kernel is older than Linux 5.13, which added landlock",
        3 => "this kernel was built without landlock",
        4 => {
            "this kernel did not turn landlock on at boot, as when the boot's lsm= list leaves it out"
        }
        _ => return None,
    })
}

/// The end of a sentence that says a part of the helper's sandbox is
/// missing: the reason `why`, or, for a number vtium does not know, that
/// number, so the line never names a reason the helper did not give.
fn because(why: Option<&str>, code: u32) -> String {
    match why {
        Some(why) => format!(", because {why}."),
        None => format!(
            ". The font helper's report gives the reason as number {code}, which vtium does not know."
        ),
    }
}

/// The line the console logs when the helper's report of its sandbox,
/// `report`, lacks a part: each part not in force and the reason the
/// helper gives, and whether the fallback filter stands in for the
/// landlock domain. `None` when every part is in force.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": the line
///   and the reasons it tells apart.
pub(super) fn sandbox_notice(report: u32) -> Option<String> {
    let parts = report & SANDBOX_PARTS;
    let seccomp = report >> SANDBOX_SECCOMP_SHIFT & 0xff;
    let domain = report >> SANDBOX_DOMAIN_SHIFT & 0xff;
    if parts == 0 && seccomp == 1 && domain == 1 {
        return Some(
            "The font helper runs without its seccomp filters and its landlock domain, because the setting helper-sandbox=off turns them off. A font that took over the font helper could open network connections and change any file that the font helper's account may change.".to_owned(),
        );
    }
    let domain = (parts & SANDBOX_DOMAIN == 0).then(|| {
        let stands_in = if parts & SANDBOX_FALLBACK != 0 {
            "A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
        } else {
            "No filter takes the domain's place, so a font that took over the font helper could change any file that the font helper's account may change."
        };
        format!(
            "The font helper runs without its landlock domain{} {stands_in}",
            because(domain_reason(domain), domain)
        )
    });
    let sockets = (parts & SANDBOX_SOCKETS == 0).then(|| {
        format!(
            "The font helper runs without its seccomp filters{}",
            because(seccomp_reason(seccomp), seccomp)
        )
    });
    let lines: Vec<String> = domain.into_iter().chain(sockets).collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// Whether this boot has logged a font helper's sandbox line: one line a
/// boot, whichever helper reports first, for every font set and user.
pub(super) static SANDBOX_LOGGED: AtomicBool = AtomicBool::new(false);

/// Logs `line` unless `boot` says a helper's sandbox line was logged
/// already, and marks it logged; whether it logged.
pub(super) fn log_sandbox_once(boot: &AtomicBool, line: &str) -> bool {
    let first = !boot.swap(true, Ordering::Relaxed);
    if first {
        log::warn!("{line}");
    }
    first
}

/// Whether the helpers lock themselves in with their seccomp filters and
/// landlock domains: `config`'s `helper-sandbox`, on unless it is `off`.
///
/// References:
/// - ADR-1200 D15, "A setting turns the sandbox off": the key, on by
///   default, which only root's vtium.conf and vtium's command line set.
pub fn sandbox_on(config: &Config) -> bool {
    config.text(&HELPER_SANDBOX) != Some("off")
}

/// The options that tell the font helper `helper-sandbox=off`, `--sandbox
/// off`; none when the sandbox is on, since a helper started without the
/// option sandboxes itself in full.
pub fn sandbox_args(config: &Config) -> Vec<OsString> {
    if sandbox_on(config) {
        Vec::new()
    } else {
        vec!["--sandbox".into(), "off".into()]
    }
}

/// The bits of a [`FRAME_SANDBOX`]: the helper's landlock domain, its
/// fallback filter and its socket filter are in force.
pub(super) const SANDBOX_DOMAIN: u32 = 1;
pub(super) const SANDBOX_FALLBACK: u32 = 2;
pub(super) const SANDBOX_SOCKETS: u32 = 4;
const SANDBOX_PARTS: u32 = 0xff;
/// Where a [`FRAME_SANDBOX`] carries why the seccomp filters are not in
/// force ([`seccomp_reason`]) and why the landlock domain is not
/// ([`domain_reason`]), a byte each.
pub(super) const SANDBOX_SECCOMP_SHIFT: u32 = 8;
pub(super) const SANDBOX_DOMAIN_SHIFT: u32 = 16;
