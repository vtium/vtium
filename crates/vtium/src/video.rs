//! A video device as the seat sees it: a DRM card or a framebuffer, or a
//! stand-in in tests, behind one trait.

use std::any::Any;
use std::io;

use vtium_platform::display::{
    Canvas, Card, Density, DisplayId, Dpms, Drm2d, Event, Fb, Fbdev, Identity, Rect, SwapError,
    Swapped,
};
use vtium_render::Arrow;

use crate::pointer::Plane;

/// A video device: its displays, each lending a canvas for a frame and
/// presenting it, and its sleep, wake-up and power.
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
    /// The back buffer, lent for one frame.
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>>;
    /// Presents the frame drawn into the canvas with its damage.
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError>;
    /// Whether a frame swapped is still on its way to the screen.
    fn is_flipping(&self, id: DisplayId) -> bool;
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
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        Drm2d::canvas(self, id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        Drm2d::swap(self, id, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        Drm2d::is_flipping(self, id)
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
    fn canvas(&mut self, id: DisplayId) -> Option<Canvas<'_>> {
        Fbdev::canvas(self, id)
    }
    fn swap(&mut self, id: DisplayId, damage: &[Rect]) -> Result<Swapped, SwapError> {
        Fbdev::swap(self, id, damage)
    }
    fn is_flipping(&self, id: DisplayId) -> bool {
        Fbdev::is_flipping(self, id)
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
