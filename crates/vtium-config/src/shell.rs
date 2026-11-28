//! A word quoted for a shell, as vtium-config shows a command line that a
//! person can copy.

/// `word` quoted for a shell where it needs it.
pub fn shell(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:,=@+".contains(c))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}
