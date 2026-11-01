//! A connector's `PATH` property, which the head file's `connector-path:`
//! matches (ADR-1401 D4).
//!
//! The kernel gives each connector of a DisplayPort multi-stream (MST) hub
//! a `PATH` property when the driver creates the connector. Its value
//! names an immutable blob that holds `mst:`, the object id of the card's
//! connector the hub is plugged into, and the port number at each hub on
//! the way, joined by `-`, as `mst:213-1` or `mst:213-2-1`, followed by a
//! NUL. A connector that is not a hub port has no `PATH` property. The name
//! the driver gives such a connector (`DP-8`) can change after a replug or
//! a resume, while its `PATH` stays the same.

/// The text of a `PATH` blob as `connector-path:` compares it: the bytes
/// before the first NUL, which ends the kernel's string, with each byte
/// that is not printable ASCII, a space included, and `#`, which starts
/// the head file's comments, read as `_`, so the text is always one token
/// of the head file. `None` where no byte comes before the NUL.
pub(crate) fn text(blob: &[u8]) -> Option<String> {
    let end = blob.iter().position(|&b| b == 0).unwrap_or(blob.len());
    let bytes = blob.get(..end)?;
    (!bytes.is_empty()).then(|| {
        bytes
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() && b != b'#' {
                    char::from(b)
                } else {
                    '_'
                }
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn a_path_is_its_text_before_the_nul() {
        assert_eq!(text(b"mst:213-1\0").as_deref(), Some("mst:213-1"));
        assert_eq!(text(b"mst:213-2-1\0").as_deref(), Some("mst:213-2-1"));
        // a blob with no NUL is read to its end, and a NUL ends it early
        assert_eq!(text(b"mst:213-1").as_deref(), Some("mst:213-1"));
        assert_eq!(text(b"mst:213-1\0-9\0").as_deref(), Some("mst:213-1"));
    }

    #[test]
    fn a_path_with_bytes_no_token_holds_reads_them_as_underscores() {
        assert_eq!(text(b"mst:2 3#1\xff\0").as_deref(), Some("mst:2_3_1_"));
    }

    #[test]
    fn an_empty_path_is_none() {
        assert_eq!(text(b""), None);
        assert_eq!(text(b"\0"), None);
        assert_eq!(text(b"\0mst:213-1"), None);
    }
}
