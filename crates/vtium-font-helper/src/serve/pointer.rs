//! The pointer pictures the helper draws for a pointer ask, from an SVG
//! pointer file or a cursor theme's file.

use std::io::{self, Write};

use super::wire::{
    FRAME_DONE, FRAME_NO_POINTER, FRAME_POINTER, NO_POINTER_MISSING, NO_POINTER_REFUSED,
    PointerAsk, PointerFile, REASON_BYTES_MAX, put32, shape_code, write_frame,
};

impl PointerAsk {
    /// Draws the ask's pictures and writes the answer: a [`FRAME_POINTER`]
    /// for each picture, or one [`FRAME_NO_POINTER`], then
    /// [`FRAME_DONE`].
    pub(super) fn answer(&self, out: &mut dyn Write) -> io::Result<()> {
        let code = shape_code(self.shape);
        let none = |out: &mut dyn Write, why: u32, reason: &str| {
            let mut payload = Vec::new();
            put32(&mut payload, code);
            put32(&mut payload, why);
            let mut reason = reason.to_owned();
            while reason.len() > REASON_BYTES_MAX {
                reason.pop();
            }
            payload.extend_from_slice(reason.as_bytes());
            write_frame(out, FRAME_NO_POINTER, &payload)
        };
        match self.file {
            PointerFile::Svg => {
                let mut heights = self.heights.clone();
                heights.sort_unstable();
                heights.dedup();
                for h in heights {
                    match crate::pointer_svg::draw(&self.bytes, self.shape.name(), h) {
                        Ok(p) => write_frame(out, FRAME_POINTER, &p.payload(self.shape))?,
                        Err(crate::pointer_svg::NoPicture::Missing) => {
                            none(out, NO_POINTER_MISSING, "")?;
                            break;
                        }
                        Err(crate::pointer_svg::NoPicture::Refused(why)) => {
                            none(out, NO_POINTER_REFUSED, &why)?;
                            break;
                        }
                    }
                }
            }
            PointerFile::Xcursor => match crate::xcursor::read(&self.bytes, self.size) {
                Ok(pictures) => {
                    for p in pictures {
                        write_frame(out, FRAME_POINTER, &p.payload(self.shape))?;
                    }
                }
                Err(why) => none(out, NO_POINTER_REFUSED, &why)?,
            },
        }
        write_frame(out, FRAME_DONE, &[])
    }
}
