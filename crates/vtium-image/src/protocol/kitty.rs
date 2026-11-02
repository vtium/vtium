//! The kitty graphics handler, which takes APC `G` strings (ADR-1100, D6).
//!
//! It reads each string and changes nothing yet: it counts the bytes it
//! reads as input work and places no picture.

use vtium_parse::Chunk;

use crate::decode::Reply;
use crate::handler::{Context, Handler, Intro};
use crate::protocol::chunk_work;

/// The kitty graphics protocol's front end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kitty {}

impl Handler for Kitty {
    fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}

    fn feed(&mut self, ctx: &mut dyn Context, data: Chunk<'_>) {
        ctx.add_work(chunk_work(data));
    }

    fn finish(&mut self, _: &mut dyn Context) {}

    fn cancel(&mut self, _: &mut dyn Context) {}

    fn decoded(&mut self, _: &mut dyn Context, _: Reply) {}

    fn clone_box(&self) -> Box<dyn Handler> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn same(&self, other: &dyn Handler) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}
