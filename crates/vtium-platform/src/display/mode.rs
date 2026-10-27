//! Display modes, and the one each display opens in (vtium/vtium#5).

use drm_ffi::{
    DRM_MODE_FLAG_DBLSCAN, DRM_MODE_FLAG_INTERLACE, DRM_MODE_TYPE_PREFERRED, drm_mode_modeinfo,
};

/// A display mode, as the kernel lists it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode(pub(crate) drm_mode_modeinfo);

impl Mode {
    /// The visible size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (u32::from(self.0.hdisplay), u32::from(self.0.vdisplay))
    }

    /// The refresh rate the mode names, in Hz.
    pub fn vrefresh(&self) -> u32 {
        self.0.vrefresh
    }

    /// Whether the panel marks the mode as its preferred one.
    pub fn preferred(&self) -> bool {
        self.0.type_ & DRM_MODE_TYPE_PREFERRED != 0
    }

    /// The refresh rate in mHz from the pixel clock (kHz) and the totals,
    /// as the kernel's `drm_mode_vrefresh` counts it, or the mode's
    /// rounded rate when the totals are unset.
    pub fn refresh_mhz(&self) -> u32 {
        let m = &self.0;
        let mut num = u64::from(m.clock).saturating_mul(1_000_000);
        let mut den = u64::from(m.htotal).saturating_mul(u64::from(m.vtotal));
        if m.flags & DRM_MODE_FLAG_INTERLACE != 0 {
            num = num.saturating_mul(2);
        }
        if m.flags & DRM_MODE_FLAG_DBLSCAN != 0 {
            den = den.saturating_mul(2);
        }
        if m.vscan > 1 {
            den = den.saturating_mul(u64::from(m.vscan));
        }
        match num.checked_div(den) {
            Some(mhz) => u32::try_from(mhz).unwrap_or(u32::MAX),
            None => m.vrefresh.saturating_mul(1000),
        }
    }
}

/// A mode the configuration asks for, `mode=WxH[@rate]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub width: u32,
    pub height: u32,
    /// The refresh rate in Hz; `None` takes any.
    pub rate: Option<u32>,
}

/// The mode configured for a connector, by its name (`HDMI-A-1`).
pub type ModeLookup = dyn Fn(&str) -> Option<Wanted>;

/// How each display's mode is chosen.
#[derive(Default)]
pub struct ModePolicy {
    /// Keep the mode the console left, `use-original-mode`.
    pub use_original: bool,
    pub wanted: Option<Box<ModeLookup>>,
}

/// The mode a display opens in, and the one a failed modeset falls back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) current: Mode,
    /// The panel's preferred mode, or its first.
    pub(crate) default: Mode,
}

/// Chooses among a connector's `modes`, given the mode its CRTC shows now.
///
/// The default is the first mode the panel marks preferred, or the first
/// listed when none is. The current mode is the console's mode when
/// `use_original` is set and the connector lists it, else the first mode
/// that matches `wanted`, else the default.
pub(crate) fn choose(
    modes: &[Mode],
    shown: Option<&Mode>,
    wanted: Option<Wanted>,
    use_original: bool,
) -> Option<Choice> {
    let default = *modes
        .iter()
        .find(|m| m.preferred())
        .or_else(|| modes.first())?;
    let original = shown.and_then(|shown| modes.iter().find(|m| *m == shown));
    let desired = wanted.and_then(|w| {
        modes.iter().find(|m| {
            m.size() == (w.width, w.height) && w.rate.is_none_or(|rate| m.vrefresh() == rate)
        })
    });
    let current = match (use_original, original) {
        (true, Some(original)) => *original,
        _ => desired.copied().unwrap_or(default),
    };
    Some(Choice { current, default })
}

#[cfg(test)]
pub(crate) fn mode(width: u16, height: u16, rate: u32, preferred: bool) -> Mode {
    Mode(drm_mode_modeinfo {
        clock: u32::from(width)
            .saturating_mul(u32::from(height))
            .saturating_mul(rate)
            / 1000,
        hdisplay: width,
        htotal: width,
        vdisplay: height,
        vtotal: height,
        vrefresh: rate,
        type_: if preferred {
            DRM_MODE_TYPE_PREFERRED
        } else {
            0
        },
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Option<Wanted> = None;

    // covers: platform.native-mode
    #[test]
    fn the_preferred_mode_is_the_default() {
        let modes = [mode(1024, 768, 60, false), mode(1920, 1080, 60, true)];
        let c = choose(&modes, None, W, false);
        assert_eq!(c.map(|c| c.current.size()), Some((1920, 1080)));
        assert_eq!(c.map(|c| c.default.size()), Some((1920, 1080)));
    }

    // The operator's decision on vtium/vtium#5: the first listed mode, not
    // the largest.
    #[test]
    fn with_none_preferred_the_first_mode_is_the_default() {
        let modes = [mode(1024, 768, 60, false), mode(1920, 1080, 60, false)];
        let c = choose(&modes, None, W, false);
        assert_eq!(c.map(|c| c.current.size()), Some((1024, 768)));
    }

    #[test]
    fn the_first_preferred_mode_wins() {
        let modes = [
            mode(800, 600, 60, false),
            mode(1920, 1080, 60, true),
            mode(1280, 720, 60, true),
        ];
        let c = choose(&modes, None, W, false);
        assert_eq!(c.map(|c| c.default.size()), Some((1920, 1080)));
    }

    #[test]
    fn a_configured_mode_overrides_the_default() {
        let modes = [
            mode(1920, 1080, 60, true),
            mode(1280, 720, 50, false),
            mode(1280, 720, 60, false),
        ];
        let any = Wanted {
            width: 1280,
            height: 720,
            rate: None,
        };
        let c = choose(&modes, None, Some(any), false);
        assert_eq!(c.map(|c| c.current.vrefresh()), Some(50));
        assert_eq!(c.map(|c| c.default.size()), Some((1920, 1080)));
        let at_60 = Wanted {
            rate: Some(60),
            ..any
        };
        let c = choose(&modes, None, Some(at_60), false);
        assert_eq!(c.map(|c| c.current.vrefresh()), Some(60));
        let missing = Wanted {
            width: 640,
            height: 480,
            rate: None,
        };
        let c = choose(&modes, None, Some(missing), false);
        assert_eq!(c.map(|c| c.current.size()), Some((1920, 1080)));
    }

    #[test]
    fn use_original_keeps_the_consoles_mode() {
        let modes = [mode(7680, 2160, 60, true), mode(3840, 1080, 60, false)];
        let shown = mode(3840, 1080, 60, false);
        let c = choose(&modes, Some(&shown), W, true);
        assert_eq!(c.map(|c| c.current.size()), Some((3840, 1080)));
        let c = choose(&modes, Some(&shown), W, false);
        assert_eq!(c.map(|c| c.current.size()), Some((7680, 2160)));
    }

    #[test]
    fn use_original_without_a_listed_mode_takes_the_default() {
        let modes = [mode(1920, 1080, 60, true)];
        let c = choose(&modes, None, W, true);
        assert_eq!(c.map(|c| c.current.size()), Some((1920, 1080)));
    }

    #[test]
    fn a_connector_without_modes_has_no_choice() {
        assert_eq!(choose(&[], None, W, false), None);
    }

    #[test]
    fn the_refresh_rate_comes_from_the_clock() {
        // CEA 1920x1080@60: 148.5 MHz over 2200x1125.
        let mut m = mode(1920, 1080, 60, true);
        m.0.clock = 148_500;
        m.0.htotal = 2200;
        m.0.vtotal = 1125;
        assert_eq!(m.refresh_mhz(), 60_000);
        m.0.flags = DRM_MODE_FLAG_INTERLACE;
        assert_eq!(m.refresh_mhz(), 120_000);
        m.0.htotal = 0;
        assert_eq!(m.refresh_mhz(), 60_000);
    }
}
