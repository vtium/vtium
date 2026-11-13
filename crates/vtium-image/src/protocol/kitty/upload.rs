//! The data of one kitty transmission sent directly: a whole command, or
//! an upload in chunks, whose data comes in several commands (ADR-1100,
//! D16).
//!
//! A [`Transfer`] holds what the first command's keys decided: those keys,
//! where the data goes, and what the budget reserved for it. Every chunk
//! decodes its own base64 into the same transfer, so a check of the data,
//! such as raw data short of its picture, counts the bytes of every chunk
//! together. An upload's data is capped at its reservation: a chunk that
//! would pass the cap fails the upload at once, and the handler answers
//! that chunk.
//!
//! References: ADR-1100 D5, D7, D15, D16; ADR-1102 D1, D2.

use crate::decode::base64::Sink;
use crate::decode::raw::Raw;
use crate::handler::Context;
use crate::store::Reservation;

use super::HOST_FIRST;
use super::command::{Command, Error};

/// Where a transfer's decoded bytes go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Data {
    /// Raw pixels, converted as they arrive, for a picture or a frame.
    Raw(Raw),
    /// Raw data a query reads: only counted, since a query stores
    /// nothing. `want` is the bytes the picture needs.
    Count { want: u64 },
    /// A PNG or zlib payload, kept whole for the image helper. `room` is
    /// the bytes the reservation holds for it.
    Bytes { data: Vec<u8>, room: u64 },
    /// Nothing more is kept: the data failed, and the transfer answers
    /// its failure when it ends.
    Dropped,
}

/// One transmission's data, from its first command to its last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Transfer {
    /// The first command's keys, which decide the picture and the answer.
    pub(super) cmd: Command,
    /// Where the data goes.
    pub(super) data: Data,
    /// What the budget, or a frame's allowance, holds for the data.
    pub(super) reservation: Option<Reservation>,
    /// The decoded bytes taken, every chunk together, past the picture's
    /// own bytes included.
    pub(super) taken: u64,
    /// It is an upload in chunks: its first command carried `m=1`.
    pub(super) chunked: bool,
    /// The most bytes an upload in chunks of raw data may take: its
    /// reservation's whole cost. A whole command has no cap, since raw
    /// data past its picture is ignored as it arrives; a buffered payload
    /// is bounded by each growth of its reservation instead.
    pub(super) cap: Option<u64>,
    /// The data's failure, answered when the transfer ends.
    pub(super) failed: Option<Error>,
    /// The data passed the cap, or the budget refused its growth: the
    /// upload ends at the chunk that did it, whatever that chunk's `m`.
    pub(super) ended: bool,
}

impl Transfer {
    /// A transfer of `cmd`'s data into `data`, which `reservation` pays
    /// for. An upload in chunks, `chunked`, is capped at `cost`, its
    /// reservation's whole cost, when it has one.
    pub(super) fn new(
        cmd: Command,
        data: Data,
        reservation: Option<Reservation>,
        chunked: bool,
        cost: Option<u64>,
    ) -> Self {
        Self {
            cmd,
            data,
            reservation,
            taken: 0,
            chunked,
            cap: cost.filter(|_| chunked),
            failed: None,
            ended: false,
        }
    }

    /// A later chunk's `q`: one other than 0 replaces the upload's, and
    /// `q=0` or no `q` leaves it (ADR-1100, D16).
    pub(super) fn chunk_quiet(&mut self, quiet: u32) {
        if quiet != 0 {
            self.cmd.quiet = quiet;
        }
    }

    /// Drops the data, gives the reservation back, and records `error`
    /// as the transfer's answer.
    pub(super) fn fail(&mut self, ctx: &mut dyn Context, error: Error) {
        self.data = Data::Dropped;
        if let Some(r) = self.reservation.take() {
            ctx.unreserve(r);
        }
        if self.failed.is_none() {
            self.failed = Some(error);
        }
    }

    /// Gives back what the transfer holds and answers nothing.
    pub(super) fn drop_quietly(mut self, ctx: &mut dyn Context) {
        if let Some(r) = self.reservation.take() {
            ctx.unreserve(r);
        }
    }

    /// Takes `bytes` of decoded data.
    fn take(&mut self, ctx: &mut dyn Context, bytes: &[u8]) {
        if self.failed.is_some() {
            return;
        }
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.taken = self.taken.saturating_add(len);
        if self.cap.is_some_and(|cap| self.taken > cap) {
            // the program sent more than its own keys allow
            self.fail(ctx, Error::Invalid);
            self.ended = true;
            return;
        }
        match &mut self.data {
            Data::Raw(raw) => raw.push(bytes),
            Data::Count { .. } | Data::Dropped => {}
            Data::Bytes { data, room } => {
                let need = u64::try_from(data.len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(len);
                if need > *room {
                    // the reservation grows before the buffer does, doubling
                    let grown = room.saturating_mul(2).max(HOST_FIRST).max(need);
                    let more = grown.saturating_sub(*room);
                    let grew = match self.reservation.as_mut() {
                        Some(r) => ctx.grow(r, more).is_ok(),
                        None => false,
                    };
                    if !grew {
                        self.fail(ctx, Error::NoSpace);
                        self.ended = true;
                        return;
                    }
                    *room = grown;
                }
                data.extend_from_slice(bytes);
            }
        }
    }
}

/// A transfer as the base64 decoder's sink.
pub(super) struct TransferSink<'a, 'c> {
    pub(super) transfer: &'a mut Transfer,
    pub(super) ctx: &'a mut (dyn Context + 'c),
}

impl Sink for TransferSink<'_, '_> {
    fn open(&self) -> bool {
        self.transfer.failed.is_none()
    }

    fn bytes(&mut self, data: &[u8]) {
        self.transfer.take(self.ctx, data);
    }

    fn error(&mut self) {
        // a byte outside base64: the data fails, answered when the
        // transfer ends, so the chunks that follow stay its own
        self.transfer.fail(self.ctx, Error::Invalid);
    }
}
