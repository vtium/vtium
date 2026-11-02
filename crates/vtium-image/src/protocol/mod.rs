//! The two protocol handlers vtium-term's router sends strings to
//! (ADR-1100, D1, D6): [`sixel`] takes DCS `q` strings and [`kitty`] takes
//! APC `G` strings.

pub mod kitty;
pub mod sixel;

use vtium_parse::Chunk;

/// The work reading `data` costs: one for each character it holds (D5).
pub(crate) fn chunk_work(data: Chunk<'_>) -> u64 {
    match data {
        Chunk::Bytes(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        Chunk::Char(_) => 1,
    }
}
