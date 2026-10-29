//! SHA-256 (FIPS 180-4, section 6.2): the built-in sets' provenance names
//! the SHA-256 of the files they are packed from, and vtium-vitrify pins a
//! frit's font files by it (ADR-1200 D1).
//!
//! vtium-font writes it rather than taking a crate: it is one small
//! function, and `build.rs` includes this file to hash the built-in sets'
//! sources. The tests hold it to FIPS 180-4's examples and NIST's
//! long-message vector.

/// The first 32 bits of the fractional parts of the cube roots of the first
/// 64 primes (section 4.2.2).
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// The initial hash value (section 5.3.3).
const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// A SHA-256 hash in progress: bytes go in with [`Sha256::update`] and the
/// digest comes out of [`Sha256::finish`].
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    bytes: u64,
}

impl Default for Sha256 {
    fn default() -> Sha256 {
        Sha256::new()
    }
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            state: H0,
            block: [0; 64],
            filled: 0,
            bytes: 0,
        }
    }

    /// The digest of `bytes`.
    pub fn digest(bytes: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(bytes);
        h.finish()
    }

    /// Hashes `bytes` after what came before.
    pub fn update(&mut self, mut bytes: &[u8]) {
        self.bytes = self
            .bytes
            .wrapping_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        while !bytes.is_empty() {
            let free = self.block.get_mut(self.filled..).unwrap_or_default();
            let n = free.len().min(bytes.len());
            let (head, rest) = bytes.split_at(n);
            if let Some(dst) = free.get_mut(..n) {
                dst.copy_from_slice(head);
            }
            bytes = rest;
            self.filled = self.filled.saturating_add(n);
            if self.filled == 64 {
                compress(&mut self.state, &self.block);
                self.filled = 0;
            }
        }
    }

    /// The digest: the message padded with a 1 bit, zeros and its length in
    /// bits (section 5.1.1).
    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.bytes.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        let mut out = [0u8; 32];
        for (dst, word) in out.chunks_exact_mut(4).zip(self.state) {
            dst.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// One 512-bit block into the state (section 6.2.2).
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
        if let &[a, b, c, d] = bytes {
            *word = u32::from_be_bytes([a, b, c, d]);
        }
    }
    for t in 16..64usize {
        let at = |back: usize| w.get(t.wrapping_sub(back)).copied().unwrap_or(0);
        let (w2, w7, w15, w16) = (at(2), at(7), at(15), at(16));
        let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
        let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
        if let Some(word) = w.get_mut(t) {
            *word = s1.wrapping_add(w7).wrapping_add(s0).wrapping_add(w16);
        }
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (k, w) in K.iter().zip(w) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(*k)
            .wrapping_add(w);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// `digest` as 64 lower-case hex digits, as `sha256sum` prints it.
pub fn hex(digest: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in digest {
        for nibble in [b >> 4, b & 0xf] {
            s.push(char::from(
                DIGITS.get(usize::from(nibble)).copied().unwrap_or(b'0'),
            ));
        }
    }
    s
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn hash(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    // FIPS 180-4's examples (NIST's "SHA256.pdf", one block and two blocks)
    // and the empty message's digest
    #[test]
    fn fips_examples() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hash(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // NIST's long-message vector: a million 'a's, fed in pieces that cross
    // the block boundary
    #[test]
    fn a_million_a() {
        let mut h = Sha256::new();
        let piece = [b'a'; 1000];
        for _ in 0..1000 {
            h.update(&piece[..999]);
            h.update(&piece[..1]);
        }
        assert_eq!(
            hex(&h.finish()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    // a message of two blocks before padding: the 896-bit message NIST
    // gives for SHA-512, whose SHA-256 digest is published beside it
    #[test]
    fn a_two_block_message() {
        assert_eq!(
            hash(
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
                  ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
            ),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
    }

    // a 1 MiB message and 13 bytes, fed in uneven pieces, against the
    // digest Python's hashlib gives for the same bytes
    #[test]
    fn a_long_message_of_many_blocks() {
        let data: Vec<u8> = (0..(1usize << 20) + 13)
            .map(|i| u8::try_from((i * 31 + 7) % 251).expect("under 251"))
            .collect();
        let mut h = Sha256::new();
        for piece in data.chunks(1000) {
            h.update(piece);
        }
        assert_eq!(
            hex(&h.finish()),
            "59facc7129eb285381699c869e4d33112119bcdb753081c7ae5103cc01d29d77"
        );
    }

    // every length a padding block can end at
    #[test]
    fn split_updates_match_one() {
        let data: Vec<u8> = (0..200u8).collect();
        for n in 0..data.len() {
            let mut h = Sha256::new();
            let (a, b) = data.split_at(n);
            h.update(a);
            h.update(b);
            assert_eq!(h.finish(), Sha256::digest(&data), "split at {n}");
        }
    }
}
