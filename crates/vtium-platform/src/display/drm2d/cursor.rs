//! The hardware cursor: the pointer image on a display's cursor plane,
//! its two buffers, where the plane shows the image, and the image taken
//! off the plane.

use std::io;

use drm_ffi::{DRM_CAP_CURSOR_HEIGHT, DRM_CAP_CURSOR_WIDTH};

use super::super::DisplayId;
use super::super::kms::{Dumb, Kms};
use super::buffers::Source;
use super::output::{ARGB8888, CURSOR_MAX, Cursor, invalid};
use super::{Drm2d, drop_cursor};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// Puts `pixels`, ARGB8888 rows of `width`, on the display's cursor
    /// plane, with its hotspot at `hot`; it stays hidden until
    /// [`Drm2d::show_cursor`]. A display that is not online refuses it
    /// with `NotConnected`, which no error from the card decodes to; one
    /// without a cursor plane, or whose plane, at most [`CURSOR_MAX`] a
    /// side, is smaller than the image, with `Unsupported`, so the owner
    /// draws the image whole rather than cut.
    ///
    /// A display that already holds an image, as when a program changes
    /// the pointer's shape, keeps two buffers for the plane and writes the
    /// new image into the one the plane does not show. The plane keeps
    /// showing the old image until the commit that shows the new one, so
    /// the pointer never vanishes in between, and a change of shape
    /// allocates nothing once both buffers exist. Removing the framebuffer
    /// the plane shows would instead take the plane off at once.
    pub fn setup_cursor(
        &mut self,
        id: DisplayId,
        pixels: &[u32],
        width: u32,
        height: u32,
        hot: (i32, i32),
    ) -> io::Result<()> {
        let awake = self.awake;
        let unsupported = || io::Error::from(io::ErrorKind::Unsupported);
        let o = self
            .outputs
            .iter()
            .find(|o| o.id == id && o.announced)
            .filter(|o| awake && o.online)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "the display is not online")
            })?;
        if o.planes.as_ref().is_none_or(|p| p.cursor.is_none()) {
            return Err(unsupported());
        }
        let side = |cap| {
            self.card
                .cap(cap)
                .ok()
                .map(|v| u32::try_from(v.min(CURSOR_MAX)).unwrap_or(0))
                .filter(|&v| v > 0)
                .ok_or_else(unsupported)
        };
        let (cw, ch) = (side(DRM_CAP_CURSOR_WIDTH)?, side(DRM_CAP_CURSOR_HEIGHT)?);
        if width > cw || height > ch {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("a {width}x{height} image on a {cw}x{ch} cursor plane"),
            ));
        }
        let image = CursorImage {
            pixels,
            width,
            height,
        };
        let shown = o.cursor_set.map(|(fb, _, _)| fb);
        let held = o
            .cursor
            .as_ref()
            .filter(|c| (c.width, c.height) == (cw, ch));
        let Some(held) = held else {
            self.destroy_cursor(id);
            let (dumb, fb) = cursor_buffer(&self.card, cw, ch)?;
            if let Err(e) = paint_cursor(&self.card, &dumb, &image) {
                let _ = self.card.rm_fb(fb);
                let _ = self.card.destroy_dumb(dumb.handle);
                return Err(e);
            }
            if let Some(o) = self.output_mut(id) {
                o.cursor = Some(Cursor {
                    dumb,
                    fb,
                    width: cw,
                    height: ch,
                    hot,
                    ..Cursor::default()
                });
            }
            return Ok(());
        };
        // The new image goes into the buffer the plane does not show: the
        // spare, made the first time, when the plane shows the held
        // buffer, and the held buffer itself otherwise. A buffer that the
        // commit still in flight takes off the plane may be on screen
        // until that flip completes, so the image goes into a new buffer
        // instead, and the busy one is freed: the kernel keeps it for the
        // flip, and no plane's committed state names it.
        let busy = |fb: u32| o.flipping && held.retiring == Some((fb, o.flips));
        let swap = shown == Some(held.fb);
        let into = if swap {
            held.spare
        } else {
            Some((held.dumb, held.fb))
        };
        let reuse = into.filter(|&(_, fb)| !busy(fb));
        let target = match reuse {
            Some(target) => target,
            None => match cursor_buffer(&self.card, cw, ch) {
                Ok(made) => made,
                Err(e) => {
                    self.destroy_cursor(id);
                    return Err(e);
                }
            },
        };
        if let Err(e) = paint_cursor(&self.card, &target.0, &image) {
            if reuse.is_none() {
                let _ = self.card.rm_fb(target.1);
                let _ = self.card.destroy_dumb(target.0.handle);
            }
            self.destroy_cursor(id);
            return Err(e);
        }
        if reuse.is_none()
            && let Some((dumb, fb)) = into
        {
            let _ = self.card.rm_fb(fb);
            let _ = self.card.destroy_dumb(dumb.handle);
        }
        if let Some(c) = self.output_mut(id).and_then(|o| o.cursor.as_mut()) {
            if swap {
                c.spare = Some((c.dumb, c.fb));
            }
            (c.dumb, c.fb) = target;
            c.hot = hot;
            c.visible = false;
        }
        Ok(())
    }

    /// Takes the cursor plane's image away (`drop_cursor`).
    pub fn destroy_cursor(&mut self, id: DisplayId) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) {
            drop_cursor(&self.card, o);
        }
    }

    /// Shows the cursor with its hotspot at (`x`, `y`), from the next
    /// swap.
    pub fn show_cursor(&mut self, id: DisplayId, x: i32, y: i32) -> io::Result<()> {
        let o = self
            .output_mut(id)
            .ok_or_else(|| invalid("no such display"))?;
        let c = o
            .cursor
            .as_mut()
            .ok_or_else(|| invalid("the display has no cursor set up"))?;
        c.at = (x, y);
        c.visible = true;
        Ok(())
    }

    /// Hides the cursor from the next swap.
    pub fn hide_cursor(&mut self, id: DisplayId) {
        if let Some(o) = self.output_mut(id)
            && let Some(c) = o.cursor.as_mut()
        {
            c.visible = false;
        }
    }

    /// Moves where the cursor's coordinates start, as for a grid placed
    /// away from the display's corner.
    pub fn set_cursor_offset(&mut self, id: DisplayId, x: i32, y: i32) {
        if let Some(o) = self.output_mut(id)
            && let Some(c) = o.cursor.as_mut()
        {
            c.offset = (x, y);
        }
    }
}

/// A pointer image for the cursor plane: ARGB8888 rows of `width`.
struct CursorImage<'a> {
    pixels: &'a [u32],
    width: u32,
    height: u32,
}

/// Makes a buffer for a cursor plane of `width` × `height`, and its
/// framebuffer.
fn cursor_buffer<K: Kms>(card: &K, width: u32, height: u32) -> io::Result<(Dumb, u32)> {
    let dumb = card.create_dumb(width, height)?;
    match card.add_fb(dumb.handle, dumb.pitch, width, height, ARGB8888) {
        Ok(fb) => Ok((dumb, fb)),
        Err(e) => {
            let _ = card.destroy_dumb(dumb.handle);
            Err(e)
        }
    }
}

/// Writes `image` into the cursor buffer `dumb` from its corner, and
/// clears the rest of the buffer, which may hold an earlier image.
fn paint_cursor<K: Kms>(card: &K, dumb: &Dumb, image: &CursorImage<'_>) -> io::Result<()> {
    let mut map = card.map_dumb(dumb)?;
    let dst = map.as_mut();
    dst.fill(0);
    let pitch = usize::try_from(dumb.pitch).unwrap_or(0);
    let cols = usize::try_from(image.width).unwrap_or(0);
    for y in 0..usize::try_from(image.height).unwrap_or(0) {
        let src = image
            .pixels
            .get(y.saturating_mul(cols)..)
            .unwrap_or_default();
        let row = dst.get_mut(y.saturating_mul(pitch)..).unwrap_or_default();
        for (px, out) in src.iter().take(cols).zip(row.as_chunks_mut::<4>().0) {
            *out = px.to_ne_bytes();
        }
    }
    Ok(())
}
