//! A video device as the seat sees it: a DRM card or a framebuffer, or a
//! stand-in in tests, behind one trait.

use std::any::Any;
use std::io;

use vtium_platform::display::{
    Canvas, Card, DisplayId, Dpms, Drm2d, Event, Fb, Fbdev, Rect, SwapError, Swapped,
};

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
    /// A key or pointer event came: a failed wake-up is tried again.
    fn input(&mut self) -> io::Result<()>;
    /// The connectors changed.
    fn hotplug(&mut self) -> io::Result<()>;
    fn as_any(&mut self) -> &mut dyn Any;
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
    fn as_any(&mut self) -> &mut dyn Any {
        self
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
