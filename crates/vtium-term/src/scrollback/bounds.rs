//! How much the scrollback keeps: `sb-size`'s default and its cap, by
//! default from the machine's memory or as the configuration sets it,
//! `sb-memory`'s byte budget, its default derived from the minspec peak, and
//! its fence, and what each row costs that budget.

use crate::bounds::{
    COLUMNS_MAX, LINKS_BYTES_MAX, ROWS_MAX, SB_MEMORY_MAX, SB_MEMORY_MIN, SB_SIZE_CAP_LARGE,
    SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_MAX,
};
use crate::damage::Damage;
use crate::line::Line;

/// `sb-size` when the configuration does not set it.
pub const SB_SIZE_DEFAULT: usize = 1000;

pub(super) const GIB: u64 = 1 << 30;

/// The most scrollback lines `sb-size` may ask for. By default it follows
/// the machine's `ram`: 10,000 lines below 2 GiB, 30,000 from 2 GiB,
/// 100,000 from 4 GiB. A configured `cap` replaces that default, higher or
/// lower, clamped to [`SB_SIZE_MAX`].
pub fn sb_size_cap(ram: u64, cap: Option<usize>) -> usize {
    match cap {
        Some(cap) => cap.min(SB_SIZE_MAX),
        None if ram >= 4 * GIB => SB_SIZE_CAP_LARGE,
        None if ram >= 2 * GIB => SB_SIZE_CAP_MEDIUM,
        None => SB_SIZE_CAP_SMALL,
    }
}

/// The scrollback lines to keep for a requested `sb-size` on a machine
/// with `ram` bytes and an optional configured `cap`: the request, clamped
/// to the cap.
pub fn sb_size(requested: usize, ram: u64, cap: Option<usize>) -> usize {
    requested.min(sb_size_cap(ram, cap))
}

/// The minspec machine's peak resident size, which the default scrollback
/// is held inside: the maintainer's round figure (vtium/vtium#131).
pub const MINSPEC_PEAK: usize = 16 << 20;

/// The minspec machine's grid: its 1366x768 display in unifont's 8x16
/// cells, 170 columns by 48 rows.
pub const MINSPEC_GRID: (usize, usize) = (170, 48);

/// What vtium's process holds on the minspec machine outside one console's
/// two screens, its scrollback and its link store, rounded up to a KiB:
/// 10.3 MiB measured after the first screen, less two screens of 32-byte
/// cells at [`MINSPEC_GRID`] (ADR-1001, "Memory"; `tests/memory.rs`
/// derives it).
pub const MINSPEC_REST: usize = 10_038 << 10;

/// The most bytes a console's main and alternate screens hold at `cols`
/// by `rows`, with their change ages: every row at
/// [`Line::bytes_max`], each screen `rows` rows, inside the screen's own
/// bounds.
pub const fn screens_bytes_max(cols: usize, rows: usize) -> usize {
    screens_bytes_max_in(cols, rows, cfg!(feature = "images"))
}

/// [`screens_bytes_max`] in a build with images (`images` true) or
/// without ([`Line::bytes_max_in`]).
const fn screens_bytes_max_in(cols: usize, rows: usize, images: bool) -> usize {
    let rows = if rows == 0 {
        1
    } else if rows > ROWS_MAX {
        ROWS_MAX
    } else {
        rows
    };
    lines_bytes_max_in(cols, rows, rows.saturating_mul(2), images)
}

/// The most bytes `lines` rows of `cols` cells hold, with the change ages
/// of a screen of `cols` by `rows`.
pub(crate) const fn lines_bytes_max(cols: usize, rows: usize, lines: usize) -> usize {
    lines_bytes_max_in(cols, rows, lines, cfg!(feature = "images"))
}

const fn lines_bytes_max_in(cols: usize, rows: usize, lines: usize, images: bool) -> usize {
    let cols = if cols == 0 {
        1
    } else if cols > COLUMNS_MAX {
        COLUMNS_MAX
    } else {
        cols
    };
    lines
        .saturating_mul(Line::bytes_max_in(cols, images))
        .saturating_add(Damage::heap_bytes_for(cols, rows))
}

/// `sb-memory` when the configuration does not set it, for a console whose
/// screens hold at most `screens` bytes: what [`MINSPEC_PEAK`] leaves
/// after [`MINSPEC_REST`], those screens and a full link store, rounded
/// down to 64 KiB. The fence, [`sb_memory`], applies on top.
pub(crate) const fn sb_memory_left(screens: usize) -> usize {
    let left = MINSPEC_PEAK
        .saturating_sub(MINSPEC_REST)
        .saturating_sub(screens)
        .saturating_sub(LINKS_BYTES_MAX);
    left.saturating_sub(left % (64 << 10))
}

/// `sb-memory` when the configuration does not set it, for a console of
/// `cols` by `rows`. On minspec's grids, up to [`MINSPEC_GRID`], it is
/// what [`MINSPEC_PEAK`] leaves after [`MINSPEC_REST`], the two screens at
/// their worst ([`screens_bytes_max`]) and a full link store, rounded down
/// to 64 KiB, so the worst a program can write into the console stays
/// inside the peak, and a smaller grid keeps more. A larger grid keeps
/// [`SB_MEMORY_DEFAULT`], the value at the minspec grid: its console uses
/// more memory than the peak by design, and the default does not grow
/// with the display or the machine, so a user who wants more scrollback
/// raises `sb-memory` (ADR-1001, "Memory"). The fence, [`sb_memory`],
/// applies on top. A screen follows its grid through
/// [`Screen::set_scrollback_memory_from_grid`](crate::Screen::set_scrollback_memory_from_grid).
pub const fn sb_memory_default(cols: usize, rows: usize) -> usize {
    sb_memory_held(screens_bytes_max(cols, rows))
}

/// [`sb_memory_default`] for a console whose screens hold at most
/// `screens` bytes: what the peak leaves after them, and never less than
/// [`SB_MEMORY_DEFAULT`].
pub(crate) const fn sb_memory_held(screens: usize) -> usize {
    let left = sb_memory_left(screens);
    if left > SB_MEMORY_DEFAULT {
        left
    } else {
        SB_MEMORY_DEFAULT
    }
}

/// `sb-memory`'s default at [`MINSPEC_GRID`], and on every larger grid:
/// what the peak leaves after the minspec grid's two screens at their
/// worst. It is the budget a scrollback starts with, and the most
/// `sb-memory` may be when the machine's memory cannot be read. It is
/// [`SB_MEMORY_DEFAULT_IMAGES`] in a build with images and
/// [`SB_MEMORY_DEFAULT_MINSPEC`] without.
pub const SB_MEMORY_DEFAULT: usize = if cfg!(feature = "images") {
    SB_MEMORY_DEFAULT_IMAGES
} else {
    SB_MEMORY_DEFAULT_MINSPEC
};

/// [`SB_MEMORY_DEFAULT`] in a build without images, 5,373,952 bytes on
/// 64-bit targets.
pub const SB_MEMORY_DEFAULT_MINSPEC: usize =
    sb_memory_left(screens_bytes_max_in(MINSPEC_GRID.0, MINSPEC_GRID.1, false));

/// [`SB_MEMORY_DEFAULT`] in a build with images, 4,915,200 bytes on 64-bit
/// targets. It is smaller than the minspec figure because a screen row may
/// hold kitty placeholder clusters outside the row's budget.
///
/// References:
/// - ADR-1001, "Placeholder cells": why placeholder clusters sit outside a
///   row's budget.
pub const SB_MEMORY_DEFAULT_IMAGES: usize =
    sb_memory_left(screens_bytes_max_in(MINSPEC_GRID.0, MINSPEC_GRID.1, true));

const _: () = assert!(SB_MEMORY_MIN <= SB_MEMORY_DEFAULT && SB_MEMORY_DEFAULT <= SB_MEMORY_MAX);

/// The most bytes `sb-memory` may set on a machine with `ram` bytes: a
/// quarter of it, from [`SB_MEMORY_MIN`] to [`SB_MEMORY_MAX`].
pub fn sb_memory_cap(ram: u64) -> usize {
    usize::try_from(ram / 4)
        .unwrap_or(usize::MAX)
        .clamp(SB_MEMORY_MIN, SB_MEMORY_MAX)
}

/// The scrollback's byte budget for a requested `sb-memory` on a machine
/// with `ram` bytes: the request inside its fence, from [`SB_MEMORY_MIN`]
/// to [`sb_memory_cap`].
pub fn sb_memory(requested: usize, ram: u64) -> usize {
    requested.clamp(SB_MEMORY_MIN, sb_memory_cap(ram))
}

/// What a row costs the scrollback's budget: its struct and its heap, by
/// capacity.
pub(super) fn cost(line: &Line) -> usize {
    size_of::<Line>().saturating_add(line.heap_bytes())
}

/// What `rows` cost the scrollback's budget.
pub(super) fn charge<'a>(rows: impl IntoIterator<Item = &'a Line>) -> usize {
    rows.into_iter().map(cost).fold(0, usize::saturating_add)
}
