//! Pictures through kitty graphics' direct transmission of raw pixels
//! (ADR-1201 D4, ADR-1100 D7): what the TUI sends to show a size's
//! specimen, which vtium decodes in the console without the image helper.
//!
//! A picture is sent and shown at the cursor in one transmission, `a=T`
//! with `f=32` (8-bit RGBA), `t=d` (the pixels in the command itself), its
//! width and height in `s` and `v`, and its base64 split into chunks of
//! [`CHUNK`] bytes, `m=1` on every chunk but the last. Text written over
//! its cells and the erases the TUI draws with leave a kitty placement in
//! place, so it stays while the screen is drawn again, until `a=d` deletes
//! it. Every command carries `q=2`, so the terminal answers none of them,
//! and the first `C=1`, so the cursor stays where it was.

/// The image id the TUI's specimen takes.
pub const IMAGE_ID: u32 = 1201;

/// The most base64 bytes one command carries, as the protocol asks.
pub const CHUNK: usize = 4096;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in base64 with padding (RFC 4648 section 4).
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for group in bytes.chunks(3) {
        let b = [
            group.first().copied().unwrap_or(0),
            group.get(1).copied().unwrap_or(0),
            group.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextets = [n >> 18, n >> 12, n >> 6, n];
        for (i, s) in sextets.iter().enumerate() {
            if i <= group.len() {
                let at = usize::try_from(s & 63).unwrap_or(0);
                out.push(char::from(ALPHABET.get(at).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The commands that send `rgba`, `width` by `height` pixels, as the image
/// `id` and show it at the cursor.
pub fn show(id: u32, width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let encoded = base64(rgba);
    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(CHUNK).collect();
    let mut out = Vec::with_capacity(
        encoded
            .len()
            .saturating_add(chunks.len().saturating_mul(24)),
    );
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i < last);
        if i == 0 {
            out.extend_from_slice(
                format!("\x1b_Ga=T,f=32,t=d,i={id},s={width},v={height},C=1,q=2,m={more};")
                    .as_bytes(),
            );
        } else {
            out.extend_from_slice(format!("\x1b_Gm={more};").as_bytes());
        }
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

/// The command that deletes the image `id` and frees its pixels.
pub fn delete(id: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\").into_bytes()
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648s_test_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain:?}");
        }
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn a_small_picture_is_one_command() {
        assert_eq!(
            show(7, 1, 1, &[1, 2, 3, 255]),
            b"\x1b_Ga=T,f=32,t=d,i=7,s=1,v=1,C=1,q=2,m=0;AQID/w==\x1b\\"
        );
    }

    #[test]
    fn a_large_picture_comes_in_chunks_marked_until_the_last() {
        // 3072 bytes are 4096 base64 bytes, one chunk; one more byte makes two
        let one = show(7, 768, 1, &[0; 3072]);
        assert_eq!(String::from_utf8_lossy(&one).matches("\x1b_G").count(), 1);
        let two = String::from_utf8(show(7, 769, 1, &[0; 3076])).unwrap_or_default();
        let commands: Vec<&str> = two.split("\x1b\\").filter(|c| !c.is_empty()).collect();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].starts_with("\x1b_Ga=T,f=32,t=d,i=7,s=769,v=1,C=1,q=2,m=1;"));
        assert_eq!(
            commands[0].len() - commands[0].find(';').unwrap_or(0) - 1,
            CHUNK
        );
        assert_eq!(commands[1], "\x1b_Gm=0;AAAAAA==");
    }

    #[test]
    fn deleting_names_the_image() {
        assert_eq!(delete(7), b"\x1b_Ga=d,d=I,i=7,q=2\x1b\\");
    }
}
