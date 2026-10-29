//! The kernel command line's escape from the head file (ADR-1401 D9).

/// What the kernel command line's `vtium.heads=` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadsWord {
    /// No `vtium.heads=`: vtium reads the head file.
    File,
    /// `vtium.heads=builtin`: vtium ignores the head file for this boot
    /// and runs the compiled-in configuration, every guard on.
    Builtin,
    /// A value vtium does not know, which it logs; it reads the head file.
    Unknown,
}

/// The `vtium.heads=` word of `cmdline`, `/proc/cmdline`'s text; the last
/// one wins, as for `vconsole.font=`.
pub fn heads_word(cmdline: &str) -> HeadsWord {
    let mut word = HeadsWord::File;
    for w in cmdline.split_whitespace() {
        if let Some(value) = w.strip_prefix("vtium.heads=") {
            word = if value == "builtin" {
                HeadsWord::Builtin
            } else {
                HeadsWord::Unknown
            };
        }
    }
    word
}
