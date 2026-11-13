//! Where a display's buffers come from. A buffer source makes each buffer
//! a display shows, with the framebuffer over it, and frees both. The rest
//! of drm2d (its modesets, flips, waits, DPMS, dark connectors, hotplug and
//! cursor plane) sees a buffer only as that framebuffer and the memory a
//! frame is drawn into. drm2d's own source is [`Dumbs`], which makes dumb
//! buffers; the GPU path (vtium/vtium#33) supplies GBM buffers to the same
//! logic through a source of its own (vtium/vtium#187).
//!
//! The cursor plane's image is a dumb buffer whatever the source, and
//! drm2d makes it itself.

use std::io;
use std::sync::{Arc, Mutex};

use super::super::kms::Kms;
use super::output::XRGB8888;

/// The format of every buffer a source makes, a DRM fourcc.
pub(super) const FORMAT: u32 = XRGB8888;

/// A display's buffer with its framebuffer.
pub(crate) struct Buffer<M> {
    pub(super) fb: u32,
    /// The bytes from one line of pixels to the next.
    pub(super) pitch: u32,
    /// The memory a frame is drawn into, which a token the display lends
    /// shares while it is out (ADR-1401 D2).
    pub(super) map: Arc<Mutex<M>>,
    /// The size it was made for.
    pub(super) size: (u32, u32),
    /// The card's handle on the buffer, which the framebuffer was made
    /// over.
    pub(super) handle: u32,
}

/// Makes and frees the buffers a display shows, in [`FORMAT`], each with
/// its framebuffer.
pub(crate) trait Source<K: Kms> {
    /// A buffer's memory, which the display lends in a token, so the
    /// head's render thread draws into it.
    type Memory: AsMut<[u8]> + Send + 'static;

    /// A buffer of `width` by `height` pixels and the framebuffer over it.
    /// Its pixels hold whatever the memory held.
    fn create(&self, card: &K, width: u32, height: u32) -> io::Result<Buffer<Self::Memory>>;

    /// Frees a buffer and its framebuffer. drm2d frees a buffer only once
    /// no token holds its memory.
    fn destroy(&self, card: &K, buffer: Buffer<Self::Memory>);
}

/// The source of dumb buffers, mapped into memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dumbs;

impl<K: Kms> Source<K> for Dumbs {
    type Memory = K::Map;

    fn create(&self, card: &K, width: u32, height: u32) -> io::Result<Buffer<K::Map>> {
        let dumb = card.create_dumb(width, height)?;
        let fb = match card.add_fb(dumb.handle, dumb.pitch, width, height, FORMAT) {
            Ok(fb) => fb,
            Err(e) => {
                let _ = card.destroy_dumb(dumb.handle);
                return Err(e);
            }
        };
        match card.map_dumb(&dumb) {
            Ok(map) => Ok(Buffer {
                fb,
                pitch: dumb.pitch,
                map: Arc::new(Mutex::new(map)),
                size: (width, height),
                handle: dumb.handle,
            }),
            Err(e) => {
                let _ = card.rm_fb(fb);
                let _ = card.destroy_dumb(dumb.handle);
                Err(e)
            }
        }
    }

    /// A card the kernel removed answers ENODEV to the framebuffer's
    /// removal. That is the state of a card that is gone and no failure:
    /// the kernel frees the card's buffers when the card's file closes, so
    /// the dumb buffer is left to it.
    fn destroy(&self, card: &K, buffer: Buffer<K::Map>) {
        drop(buffer.map);
        let _ = card.rm_fb(buffer.fb);
        if card.removed() {
            return;
        }
        if let Err(e) = card.destroy_dumb(buffer.handle)
            && !card.removed()
        {
            log::warn!("cannot destroy a dumb buffer: {e}");
        }
    }
}
