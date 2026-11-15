//! Every message vtium-clipboard shows a person, as whole messages. A value
//! that is not words, such as a name, a path, a number or the system's
//! error text, enters a message as a placeholder. The program puts its own
//! name and a colon in front of each message on standard error.
//!
//! References:
//! - ADR-1007 D3: the messages for a refused copy, for paste and for a
//!   terminal that did not answer as vtium.

/// The help page, for `--help`.
pub fn help(name: &str, max: usize) -> String {
    format!(
        "\
Usage: {name} copy [FILE]
       {name} paste
       {name} --help
       {name} --version

{name} copy copies text to the console's clipboard. It reads the text from
FILE, or from its standard input when you name no FILE. It sends the text to
this terminal as an OSC 52 sequence, so the copy also works over ssh and
through a terminal multiplexer that passes OSC 52 on. It never writes the
copy to its standard output.

For example: git rev-parse HEAD | {name} copy

The text can be at most {max} bytes, the most vtium's clipboard holds.
vtium puts the replacement character U+FFFD, which takes 3 bytes, in place
of each part of the text that is not valid UTF-8, and the limit counts the
text with those characters.

While {name} waits for this terminal's answer, it reads
the keys you type, so keys you type before the copy ends are lost. That
includes keys you type during an earlier command on the same command line.

{name} cannot see whether vtium kept the text, because OSC 52 has no answer.
When clipboard-write is off in vtium.conf, vtium ignores the copies that
programs send.

Paste is not available yet: vtium does not let programs read the clipboard.
Paste with Shift+Insert or the middle mouse button.

Exit status:
  0  The copy was sent, and the terminal answered as vtium.
  1  Nothing was copied: the text was too large, the file could not be read,
     there was no terminal, or the terminal did not take the copy.
  2  The command line was wrong, or the command was paste.
  3  The copy was sent, but the terminal did not answer as vtium. Under tmux
     or screen, the multiplexer answers for itself, so a copy there exits 3
     even when the copy reached vtium. In a background job, {name}
     cannot ask the terminal, so a copy there exits 3 too.
"
    )
}

/// The command line names no command.
pub fn no_command(name: &str) -> String {
    format!("Name a command: {name} copy [FILE] or {name} paste. Run {name} --help for help.")
}

/// The command line names a command the program does not have.
pub fn unknown_command(command: &str, name: &str) -> String {
    format!(
        "The command {command} is not one of {name}'s. The commands are copy and paste. Run {name} --help for help."
    )
}

/// `--help` or `--version` came with more words after it.
pub fn takes_nothing_more(option: &str, name: &str) -> String {
    format!("The option {option} takes nothing after it. Run {name} --help for help.")
}

/// `copy` came with more than one file.
pub fn copy_takes_one_file(name: &str) -> String {
    format!(
        "The copy command takes one FILE at most. To copy several files, run cat FILE... | {name} copy."
    )
}

/// The command was `paste`.
pub fn paste() -> String {
    "Paste is not available yet: vtium does not let programs read the clipboard. Paste with Shift+Insert or the middle mouse button.".to_owned()
}

/// The file named for copy could not be read.
pub fn file_unreadable(path: &str, error: &str) -> String {
    format!("The file {path} cannot be read: {error}. Nothing was copied.")
}

/// The standard input could not be read.
pub fn stdin_unreadable(error: &str) -> String {
    format!("The standard input cannot be read: {error}. Nothing was copied.")
}

/// The text is larger than vtium's clipboard holds.
pub fn too_large(max: usize) -> String {
    format!(
        "The text is more than {max} bytes, and vtium's clipboard holds at most {max} bytes. Nothing was copied."
    )
}

/// The text fits the clipboard as it is, but not once vtium has replaced
/// its bytes that are not valid UTF-8.
pub fn too_large_replaced(kept: usize, max: usize) -> String {
    format!(
        "The text is too large once vtium replaces its bytes that are not valid UTF-8. vtium puts the replacement character U+FFFD, which takes 3 bytes, in place of each part that is not valid UTF-8, and that makes the text {kept} bytes. vtium's clipboard holds at most {max} bytes. Nothing was copied."
    )
}

/// The process has no terminal to send the copy to.
pub fn no_terminal(name: &str, error: &str) -> String {
    format!(
        "This process has no terminal: /dev/tty cannot be opened ({error}). {name} sends the copy to its terminal and never to its standard output, so nothing was copied."
    )
}

/// The terminal did not take the bytes.
pub fn write_failed(name: &str, error: &str) -> String {
    format!(
        "The terminal /dev/tty did not take the copy ({error}). {name} cannot tell how much of it was sent, so the clipboard may hold nothing new."
    )
}

/// The terminal took no byte of the copy for the time the program allows.
pub fn stalled(name: &str, seconds: u64) -> String {
    format!(
        "The terminal /dev/tty took nothing of the copy for {seconds} seconds, so {name} stopped writing it. {name} cannot tell how much of it was sent, so the clipboard may hold nothing new."
    )
}

/// The text has bytes that are not valid UTF-8; it was copied anyway.
pub fn not_utf8() -> String {
    "The text is not all valid UTF-8. vtium shows the replacement character U+FFFD in place of the bytes that are not valid UTF-8.".to_owned()
}

/// The copy was sent, but no reply named vtium.
pub fn unconfirmed() -> String {
    "The copy was sent, but this terminal did not answer as vtium, so the text may not have reached a clipboard.".to_owned()
}

/// The copy was sent from a background job, which may not read the
/// terminal's answer.
pub fn background(name: &str) -> String {
    format!(
        "The copy was sent, but {name} runs in a background job, so it cannot wait for this terminal's answer. It cannot tell whether the terminal is vtium, so the text may not have reached a clipboard."
    )
}
