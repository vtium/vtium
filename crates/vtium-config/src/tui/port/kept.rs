//! The port's preview cache, [`Kept`], which spares the measurer and the
//! painter the work the last preview already did.

use std::sync::Mutex;

use super::{Drawn, Filterer, Measurer, Painter};

/// The last cell the measurer gave and the last picture the painter drew,
/// each with what it was asked for. Moving the Effects tab's highlight
/// asks for the same cell and the same unfiltered sample screen each time,
/// so the port measures and paints once, and each new effect costs only
/// its color table and the mapping of the pixels.
#[derive(Default)]
pub(super) struct Kept {
    cell: Option<(String, u32, (vtium_font::Cell, u32))>,
    picture: Option<(crate::specimen::Shot, crate::specimen::Picture)>,
}

/// The cell of `font` at `height`, from `kept` where the last measure
/// asked the same, else from `measure`, which `kept` then keeps.
pub(super) fn measured(
    font: &str,
    height: u32,
    measure: &Measurer,
    kept: &Mutex<Kept>,
) -> Result<(vtium_font::Cell, u32), String> {
    if let Ok(k) = kept.lock()
        && let Some((f, h, cell)) = &k.cell
        && f == font
        && *h == height
    {
        return Ok(*cell);
    }
    let cell = measure(font, height)?;
    if let Ok(mut k) = kept.lock() {
        k.cell = Some((font.to_owned(), height, cell));
    }
    Ok(cell)
}

/// What the loop answers for the picture `shot` asks for: the picture
/// `paint` draws, and where the shot carries an effect, the picture drawn
/// through it by `filter`, with what became of the effect. A shot with no
/// effect, such as No filter's, keeps the painter's pixels. The painter
/// draws the shot without its effect, and `kept` keeps that picture, so a
/// shot that differs from the last only in its effect is not painted
/// again.
pub(super) fn painted(
    shot: &crate::specimen::Shot,
    paint: &Painter,
    filter: &Filterer,
    kept: &Mutex<Kept>,
) -> Drawn {
    let plain = crate::specimen::Shot {
        filter: None,
        ..shot.clone()
    };
    let again = kept
        .lock()
        .ok()
        .and_then(|k| k.picture.clone())
        .filter(|(s, _)| *s == plain)
        .map(|(_, p)| p);
    let painted = match again {
        Some(picture) => Ok(picture),
        None => {
            let painted = paint(&plain);
            if let (Ok(picture), Ok(mut k)) = (&painted, kept.lock()) {
                k.picture = Some((plain, picture.clone()));
            }
            painted
        }
    };
    match (painted, &shot.filter) {
        (Ok(mut picture), Some(filtered)) => {
            let outcome = filter(&mut picture, filtered);
            Drawn::Filtered { picture, outcome }
        }
        (painted, _) => Drawn::Pictured(painted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shot that differs from the last only in its effect reuses the
    /// unfiltered picture and the cell, so moving the Effects tab's
    /// highlight measures and paints once; a shot of another font or size
    /// is painted again.
    #[test]
    fn a_new_effect_on_the_same_screen_is_not_painted_again() {
        use crate::specimen::{Filter, Filtered, Outcome, Picture, Shot};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let paints = std::sync::Arc::new(AtomicUsize::new(0));
        let measures = std::sync::Arc::new(AtomicUsize::new(0));
        let counted = std::sync::Arc::clone(&paints);
        let paint: Painter = std::sync::Arc::new(move |shot| {
            counted.fetch_add(1, Ordering::SeqCst);
            assert_eq!(shot.filter, None, "the painter draws no effect");
            Ok(Picture {
                width: 1,
                height: 1,
                rgba: vec![128, 128, 128, 255],
            })
        });
        let filter: Filterer = std::sync::Arc::new(|_, _| Outcome::Applied);
        let counted = std::sync::Arc::clone(&measures);
        let measure: Measurer = std::sync::Arc::new(move |_, _| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok((
                vtium_font::Cell {
                    width: 8,
                    height: 16,
                },
                16,
            ))
        });
        let effect = |file: &str| {
            Some(Filtered {
                filter: Filter::Cube(file.into()),
                active: Vec::new(),
            })
        };
        let mut shot = Shot {
            font: "unifont".to_owned(),
            height: 16,
            cell: vtium_font::Cell {
                width: 8,
                height: 16,
            },
            face_height: 16,
            display: (640, 480),
            grid: (80, 30),
            screen: Vec::new(),
            palette: String::new(),
            color: true,
            max: (8, 16),
            filter: effect("/etc/vtium/looks/warm.cube"),
        };
        let kept = Mutex::new(Kept::default());
        for file in ["/etc/vtium/looks/warm.cube", "/etc/vtium/looks/sepia.cube"] {
            shot.filter = effect(file);
            painted(&shot, &paint, &filter, &kept);
            assert!(measured("unifont", 16, &measure, &kept).is_ok());
        }
        shot.filter = None;
        painted(&shot, &paint, &filter, &kept);
        assert_eq!(paints.load(Ordering::SeqCst), 1);
        assert_eq!(measures.load(Ordering::SeqCst), 1);
        shot.height = 32;
        painted(&shot, &paint, &filter, &kept);
        assert!(measured("unifont", 32, &measure, &kept).is_ok());
        assert_eq!(paints.load(Ordering::SeqCst), 2);
        assert_eq!(measures.load(Ordering::SeqCst), 2);
    }

    /// A shot with no effect keeps the painter's pixels and is answered as
    /// any picture; a shot with one is drawn through it, and the answer
    /// says what became of it.
    #[test]
    fn a_picture_is_drawn_through_its_effect_and_one_without_keeps_its_pixels() {
        use crate::specimen::{Filter, Filtered, Outcome, Picture, Shot};
        let grey = Picture {
            width: 1,
            height: 1,
            rgba: vec![128, 128, 128, 255],
        };
        let painted_grey = grey.clone();
        let paint: Painter = std::sync::Arc::new(move |_| Ok(painted_grey.clone()));
        let filter: Filterer = std::sync::Arc::new(|picture, _| {
            picture.rgba = vec![255, 176, 0, 255];
            Outcome::Applied
        });
        let mut shot = Shot {
            font: "unifont".to_owned(),
            height: 16,
            cell: vtium_font::Cell {
                width: 8,
                height: 16,
            },
            face_height: 16,
            display: (640, 480),
            grid: (80, 30),
            screen: Vec::new(),
            palette: String::new(),
            color: true,
            max: (8, 16),
            filter: None,
        };
        let kept = Mutex::new(Kept::default());
        assert_eq!(
            painted(&shot, &paint, &filter, &kept),
            Drawn::Pictured(Ok(grey))
        );
        shot.filter = Some(Filtered {
            filter: Filter::Cube("/etc/vtium/looks/warm.cube".into()),
            active: Vec::new(),
        });
        assert_eq!(
            painted(&shot, &paint, &filter, &kept),
            Drawn::Filtered {
                picture: Picture {
                    width: 1,
                    height: 1,
                    rgba: vec![255, 176, 0, 255],
                },
                outcome: Outcome::Applied,
            }
        );
    }
}
