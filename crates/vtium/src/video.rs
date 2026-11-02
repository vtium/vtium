//! A video device as the seat sees it: a DRM card or a framebuffer, or a
//! stand-in in tests, behind one trait.

use std::any::Any;
use std::io;

use vtium_platform::display::{
    Buffers, Card, Density, DisplayId, Dpms, Drm2d, Event, Fb, Fbdev, Identity, Rect, SwapError,
    Swapped, Token,
};
use vtium_render::Arrow;

use crate::pointer::Plane;

/// How many buffers a DRM display keeps when kmscon.conf does not say
/// (ADR-1401 D2): three in a build that adds features on top of minspec,
/// two in the minspec build, vtium's default build. One buffer is for a
/// build smaller than minspec, which none is yet; [`Buffers::One`] reaches
/// it through the setting.
///
/// The features named here are all of crates/vtium's features but
/// `default`. A test holds the two lists equal, so a feature added to
/// Cargo.toml fails it until it is named here too.
pub const BUILD_BUFFERS: Buffers = if cfg!(any(feature = "images", feature = "logind")) {
    Buffers::Three
} else {
    Buffers::Two
};

/// The buffers each DRM display keeps: `configured`, kmscon.conf's
/// `display-buffers` as its accessor reads it, where it is 1, 2 or 3, and
/// else the build's [`BUILD_BUFFERS`]. A value outside 1 to 3 never
/// reaches here, since the configuration refuses it.
pub fn display_buffers(configured: Option<u8>) -> Buffers {
    configured.and_then(Buffers::new).unwrap_or(BUILD_BUFFERS)
}

/// A video device: its displays, each lending a buffer token for a frame
/// and presenting it, and its sleep, wake-up and power.
pub trait Video: Any {
    /// The backend's name in the display's log line.
    fn backend(&self) -> &'static str;
    /// What happened since the last call.
    fn events(&mut self) -> Vec<Event>;
    /// The displays online.
    fn displays(&self) -> Vec<DisplayId>;
    /// A display's name in the log line.
    fn name(&self, id: DisplayId) -> String;
    /// A display's size in pixels.
    fn size(&self, id: DisplayId) -> Option<(u32, u32)>;
    /// A display's refresh in millihertz, if it reports one.
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32>;
    /// The prepare step: the buffer the next frame is drawn into, lent as
    /// a token with its age while the display is ready for a frame.
    fn lend(&mut self, id: DisplayId) -> Option<Token>;
    /// The swap: takes the token back and presents the frame drawn into
    /// its buffer with its damage, or a cursor change alone where the
    /// damage is empty. A token back too late is dropped.
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError>;
    /// Takes a token back with nothing drawn into it.
    fn give_back(&mut self, token: Token);
    /// Whether a frame swapped is still on its way to the screen.
    fn is_flipping(&self, id: DisplayId) -> bool;
    /// Whether the display takes a frame now: it would lend a token. A
    /// frame the display does not take waits until the display is ready.
    fn ready_for_frame(&self, id: DisplayId) -> bool;
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()>;
    fn sleep(&mut self);
    fn wake_up(&mut self) -> io::Result<()>;
    fn unblank(&mut self) -> io::Result<()>;
    /// The user pressed a key or acted with the pointer (a move, a button
    /// or the wheel): a failed wake-up is tried again.
    fn input(&mut self) -> io::Result<()>;
    /// The connectors changed.
    fn hotplug(&mut self) -> io::Result<()>;
    fn as_any(&mut self) -> &mut dyn Any;
    /// Puts the pointer's `arrow` on the display's cursor plane, hidden. A
    /// device without one refuses it as unsupported, and a display not
    /// online yet with `NotConnected` ([`Plane::set_up`]).
    fn setup_cursor(&mut self, _: DisplayId, _: &Arrow) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    /// Shows the cursor plane's arrow with its hotspot on the pixel `at`,
    /// from the next swap.
    fn show_cursor(&mut self, _: DisplayId, _: (i32, i32)) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    /// Hides the cursor plane's arrow from the next swap.
    fn hide_cursor(&mut self, _: DisplayId) {}
    /// Whether the cursor plane holds the arrow set up: a device gives it
    /// up when a commit with it fails and one without it takes.
    fn hardware_cursor(&self, _: DisplayId) -> bool {
        false
    }
    /// The levels of a 1-bit or gray display, as the device says; `None`
    /// for color.
    fn levels(&self, _: DisplayId) -> Option<u8> {
        None
    }
    /// Whether a swap with no damage has something to send, as a cursor
    /// shown, moved or hidden.
    fn need_commit(&self, _: DisplayId) -> bool {
        false
    }
    /// The density the display measures from the physical size it
    /// reports, in dots per inch across and down; `None` where it reports
    /// none, or one vtium-platform does not believe.
    fn dpi(&self, _: DisplayId) -> Option<(f64, f64)> {
        None
    }
    /// What names a display to the head file's rules (ADR-1401 D4): its
    /// connector, its card's `ID_PATH` and its EDID; `None` where the
    /// device reads none, as a framebuffer, whose display the rules see
    /// by its name alone.
    fn identity(&self, _: DisplayId) -> Option<Identity> {
        None
    }
    /// The identity of each connected display the device left dark at its
    /// bind by the head file's rules, never announced: a DRM card's.
    fn dark(&self) -> Vec<Identity> {
        Vec::new()
    }
    /// Lets go of the dark display named `connector`, which the next
    /// [`Video::hotplug`] asks the rules of again (ADR-1401 D9). Returns
    /// whether it was dark.
    fn release_dark(&mut self, _: &str) -> bool {
        false
    }
}

/// A video device's cursor plane, as the pointer drives it.
impl Plane for dyn Video {
    fn set_up(&mut self, id: DisplayId, arrow: &Arrow) -> io::Result<()> {
        self.setup_cursor(id, arrow)
    }
    fn show(&mut self, id: DisplayId, at: (i32, i32)) -> io::Result<()> {
        self.show_cursor(id, at)
    }
    fn hide(&mut self, id: DisplayId) {
        self.hide_cursor(id);
    }
    fn holds(&self, id: DisplayId) -> bool {
        self.hardware_cursor(id)
    }
}

impl Video for Drm2d<Card> {
    fn backend(&self) -> &'static str {
        "drm2d"
    }
    fn events(&mut self) -> Vec<Event> {
        Drm2d::events(self).collect()
    }
    fn displays(&self) -> Vec<DisplayId> {
        Drm2d::displays(self).collect()
    }
    fn name(&self, id: DisplayId) -> String {
        Drm2d::name(self, id).unwrap_or("?").to_owned()
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        Drm2d::size(self, id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        Drm2d::refresh_mhz(self, id)
    }
    fn levels(&self, id: DisplayId) -> Option<u8> {
        Drm2d::levels(self, id)
    }
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        Drm2d::density(self, id)
            .filter(Density::measured)
            .map(|d| d.dpi())
    }
    fn identity(&self, id: DisplayId) -> Option<Identity> {
        Drm2d::identity(self, id).cloned()
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        Drm2d::lend(self, id)
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        Drm2d::swap(self, token, damage)
    }
    fn give_back(&mut self, token: Token) {
        Drm2d::give_back(self, token);
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        Drm2d::is_flipping(self, id)
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        Drm2d::ready_for_frame(self, id)
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        Drm2d::set_dpms(self, id, dpms)
    }
    fn sleep(&mut self) {
        Drm2d::sleep(self);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        Drm2d::wake_up(self)
    }
    fn unblank(&mut self) -> io::Result<()> {
        Drm2d::unblank(self)
    }
    fn input(&mut self) -> io::Result<()> {
        Drm2d::input(self)
    }
    fn hotplug(&mut self) -> io::Result<()> {
        Drm2d::poll(self)
    }
    fn dark(&self) -> Vec<Identity> {
        Drm2d::dark(self).cloned().collect()
    }
    fn release_dark(&mut self, connector: &str) -> bool {
        Drm2d::release_dark(self, connector)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn setup_cursor(&mut self, id: DisplayId, arrow: &Arrow) -> io::Result<()> {
        let (hx, hy) = arrow.hot();
        let hot = (
            i32::try_from(hx).unwrap_or(0),
            i32::try_from(hy).unwrap_or(0),
        );
        Drm2d::setup_cursor(self, id, arrow.pixels(), arrow.width(), arrow.height(), hot)
    }
    fn show_cursor(&mut self, id: DisplayId, (x, y): (i32, i32)) -> io::Result<()> {
        Drm2d::show_cursor(self, id, x, y)
    }
    fn hide_cursor(&mut self, id: DisplayId) {
        Drm2d::hide_cursor(self, id);
    }
    fn hardware_cursor(&self, id: DisplayId) -> bool {
        Drm2d::hardware_cursor(self, id)
    }
    fn need_commit(&self, id: DisplayId) -> bool {
        Drm2d::need_commit(self, id)
    }
}

impl Video for Fbdev<Fb> {
    fn backend(&self) -> &'static str {
        "fbdev"
    }
    fn events(&mut self) -> Vec<Event> {
        Fbdev::events(self).collect()
    }
    fn displays(&self) -> Vec<DisplayId> {
        Fbdev::displays(self).collect()
    }
    fn name(&self, _: DisplayId) -> String {
        "fb".to_owned()
    }
    fn size(&self, id: DisplayId) -> Option<(u32, u32)> {
        Fbdev::size(self, id)
    }
    fn refresh_mhz(&self, id: DisplayId) -> Option<u32> {
        Fbdev::refresh_mhz(self, id)
    }
    fn levels(&self, id: DisplayId) -> Option<u8> {
        Fbdev::levels(self, id)
    }
    fn dpi(&self, id: DisplayId) -> Option<(f64, f64)> {
        Fbdev::density(self, id)
            .filter(Density::measured)
            .map(|d| d.dpi())
    }
    fn lend(&mut self, id: DisplayId) -> Option<Token> {
        Fbdev::lend(self, id)
    }
    fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        Fbdev::swap(self, token, damage)
    }
    fn give_back(&mut self, token: Token) {
        Fbdev::give_back(self, token);
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        Fbdev::is_flipping(self, id)
    }
    fn ready_for_frame(&self, id: DisplayId) -> bool {
        Fbdev::ready_for_frame(self, id)
    }
    fn set_dpms(&mut self, id: DisplayId, dpms: Dpms) -> io::Result<()> {
        Fbdev::set_dpms(self, id, dpms)
    }
    fn sleep(&mut self) {
        Fbdev::sleep(self);
    }
    fn wake_up(&mut self) -> io::Result<()> {
        Fbdev::wake_up(self)
    }
    fn unblank(&mut self) -> io::Result<()> {
        Fbdev::unblank(self)
    }
    fn input(&mut self) -> io::Result<()> {
        Fbdev::input(self)
    }
    fn hotplug(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-1401 D2: the minspec build, vtium's default build, keeps two
    // buffers a DRM display.
    #[cfg(not(any(feature = "images", feature = "logind")))]
    #[test]
    fn the_minspec_build_keeps_two_buffers() {
        assert_eq!(BUILD_BUFFERS, Buffers::Two);
    }

    // ADR-1401 D2: `display-buffers` set gives its count, and unset or out
    // of range the build's.
    #[test]
    fn the_setting_chooses_the_buffers_and_the_build_fills_in() {
        let counts: Vec<Buffers> = [Some(1), Some(2), Some(3)]
            .into_iter()
            .map(display_buffers)
            .collect();
        assert_eq!(counts, [Buffers::One, Buffers::Two, Buffers::Three]);
        assert_eq!(display_buffers(None), BUILD_BUFFERS);
        assert_eq!(display_buffers(Some(4)), BUILD_BUFFERS);
    }

    /// The features BUILD_BUFFERS names as added on top of minspec.
    const ON_TOP: [&str; 2] = ["images", "logind"];

    // Every feature of the crate but `default` adds to minspec, so each is
    // one BUILD_BUFFERS gives three buffers for: a new feature fails this
    // test until it is named in BUILD_BUFFERS and here.
    #[test]
    fn build_buffers_names_every_feature_on_top_of_minspec() {
        let manifest = include_str!("../Cargo.toml");
        let mut features: Vec<&str> = manifest
            .lines()
            .skip_while(|l| l.trim() != "[features]")
            .skip(1)
            .take_while(|l| !l.trim_start().starts_with('['))
            .filter_map(|l| l.split_once('=').map(|(name, _)| name.trim()))
            .filter(|name| !name.starts_with('#') && *name != "default")
            .collect();
        features.sort_unstable();
        assert_eq!(features, ON_TOP);
    }

    // ADR-1401 D2: a build that adds features on top of minspec keeps
    // three.
    #[cfg(any(feature = "images", feature = "logind"))]
    #[test]
    fn a_build_with_features_keeps_three_buffers() {
        assert_eq!(BUILD_BUFFERS, Buffers::Three);
    }
}
