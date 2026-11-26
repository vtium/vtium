//! The VT's events and the devices' events: the VT is acquired and released,
//! a video device reports its displays, and udev reports devices that come
//! and go. A video device opens through the VT and is wrapped for the loop.

use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use vtium_devices::Card;
use vtium_platform::display::Event;
use vtium_platform::udev::{DeviceEvent, DeviceKind};
use vtium_platform::vt::VtEvent;
use vtium_render::Depth;

use crate::config::keys;
use crate::config::{Config, ModeSpec};
use crate::head::place_display;
use crate::video::Video;

use super::drawing::power_down;
use super::{App, Exit};

impl App {
    pub(super) fn vt_event(&mut self, event: VtEvent, now: Instant) {
        match event {
            VtEvent::Acquired => {
                // vtium's VT shown again ends every hand-over, and the
                // wake-up below takes their cards back (ADR-1401 D16).
                self.hand_over_vt_shown();
                // The wake-up sets every device up, an owed re-take's too,
                // and each `Awake` it brings counts as activity.
                self.retake_owed = false;
                self.retaking.clear();
                if let Some(input) = &mut self.input {
                    input.wake_up();
                }
                // The keyboards start afresh, holding no modifier, so a
                // Ctrl held before the switch shows no overlay now.
                self.rehover();
                for (node, video) in &mut self.videos {
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot wake {}: {e}", node.display());
                    }
                }
                self.darken();
                self.wait.shown(now);
                // The blank's countdown starts afresh from the acquire, and
                // a blank in force ends; while the VT was another's it ran
                // no blank, since its displays were not vtium's.
                self.user_input(now);
                self.activate(now);
            }
            VtEvent::Release => {
                // The acquire sets every device up: no re-take is owed
                // to the VT that is not ours.
                self.retake_owed = false;
                // The drag's release reaches the VT that is ours next.
                for head in &mut self.heads {
                    head.pointer.drag = None;
                }
                self.deactivate();
                // Before the first frame showed, the start's wait stops
                // until the VT comes back (vtium/vtium#270).
                self.wait.released();
                if let Some(input) = &mut self.input {
                    input.sleep();
                }
                self.releasing = Some(self.videos.keys().cloned().collect());
                for video in self.videos.values_mut() {
                    video.sleep();
                }
                // A head on the GPU path gives back its textures while the
                // VT is another's (ADR-1303 D9).
                for head in &mut self.heads {
                    if let Some(painter) = &mut head.painter {
                        painter.release();
                    }
                }
                // The focus-out goes now, before the turn lets the VT go
                // (ADR-1401 D5).
                self.report_focus();
            }
            VtEvent::Retaken if self.blank.blanked() => {
                // As a hotplug while blanked: the set-up waits for the
                // input that ends the blank, and the displays stay dark
                // (the operator's decision on vtium/vtium#21).
                self.retake_owed = true;
            }
            VtEvent::Retaken => {
                for (node, video) in &mut self.videos {
                    // A card a hand-over gave up stays the program's.
                    if self.hand_overs.given_up(node) {
                        continue;
                    }
                    self.retaking.insert(node.clone());
                    if let Err(e) = video.wake_up() {
                        log::warn!("cannot set up {} again: {e}", node.display());
                    }
                }
                self.darken();
                self.lose_every_view();
            }
            VtEvent::Hangup => self.request_exit(Exit::Hangup),
        }
    }

    pub(super) fn video_event(&mut self, node: &Path, event: Event, now: Instant) {
        match event {
            Event::Added(id) => self.add_display(node, id),
            Event::Removed(id) => {
                let key = (node.to_owned(), id);
                self.lit.remove(&key);
                self.unlit.remove(&key);
                self.dark.remove(&key);
                self.guard_lit.take_if(|(k, _)| *k == key);
                self.guard_due = true;
                if let Some(h) = self.head_of(node, id) {
                    if let Some(head) = self.heads.get_mut(h) {
                        head.drop_views(|k| *k == key);
                    }
                    self.relayout(h);
                }
            }
            // A display the device cannot light beside the others is still
            // plugged in, so its head keeps it, and its sessions with it,
            // whatever `when-display-lost` says (ADR-1401 D8). It draws
            // nothing until the modeset that lights it says to redraw, and
            // until then the guards do not count it as lit (D9).
            Event::Unlit(id) => {
                let key = (node.to_owned(), id);
                if let Some(display) = self.lit.remove(&key) {
                    self.unlit.insert(key.clone(), display);
                }
                self.guard_lit.take_if(|(k, _)| *k == key);
                self.guard_due = true;
                // A device names a display it cannot name "?".
                let name = self
                    .videos
                    .get(node)
                    .map(|video| video.name(id))
                    .filter(|name| !name.is_empty() && name != "?")
                    .map_or_else(
                        || "an unnamed display".to_owned(),
                        |name| format!("display {name}"),
                    );
                log::info!(
                    "{name} is dark but still connected, so its head keeps its sessions until the display lights"
                );
            }
            // A modeset or a wake-up may have given the display a new
            // mode in place, on the same connector. Its view takes the
            // display's refresh rate again, which paces its frames. At a
            // new size it draws with a new renderer at that size, and the
            // head is laid out again: a view left at the old size never
            // draws, because every token the display lends has the new
            // size.
            Event::Redraw(id) => {
                let key = (node.to_owned(), id);
                // A display left unlit is lit again.
                if let Some(display) = self.unlit.remove(&key) {
                    self.lit.insert(key.clone(), display);
                    self.guard_due = true;
                }
                let mode = self
                    .videos
                    .get(node)
                    .and_then(|video| Some((video.size(id)?, video.refresh_mhz(id))));
                let name = self.videos.get(node).map(|video| video.name(id));
                let App {
                    heads,
                    config,
                    locale,
                    ..
                } = self;
                let mut resized = None;
                for (h, head) in heads.iter_mut().enumerate() {
                    let Some(v) = head.views.get(&key) else {
                        continue;
                    };
                    let height = v.height;
                    let new = mode.filter(|&(size, _)| size != v.size);
                    let font = new.map(|_| {
                        head.face(height, locale)
                            .map(|face| head.font_at(face, config))
                    });
                    let render = head.render_config(config);
                    head.draw_now = true;
                    let Some(v) = head.views.get_mut(&key) else {
                        continue;
                    };
                    v.lost = true;
                    if let Some((_, refresh)) = mode {
                        v.set_refresh(refresh);
                    }
                    match (new, font) {
                        (Some((size, _)), Some(Ok(font))) => {
                            v.resize(font, size, render);
                            resized = Some(h);
                        }
                        // With no font at its height the view keeps its
                        // size and its renderer, so it never draws at a
                        // size its renderer was not made for; it draws
                        // nothing until the next set-up.
                        (Some((size, _)), Some(Err(e))) => log::warn!(
                            "cannot draw display {} at its new size {}x{}: {e}",
                            name.as_deref().unwrap_or("?"),
                            size.0,
                            size.1
                        ),
                        _ => {}
                    }
                }
                if let Some(h) = resized {
                    self.relayout(h);
                }
                // The modeset may have given the display a size, refresh or
                // depth that the head's vtlook judges differently.
                #[cfg(feature = "filters")]
                if let Some(h) = self.head_of(node, id) {
                    self.give_colors(h, &key);
                }
            }
            // A frame waiting on this flip is drawn at the turn's end,
            // as a pending view is; the other displays keep their own
            // coalescing.
            // A frame on screen ends the start's wait.
            Event::Flipped(id) => {
                let key = (node.to_owned(), id);
                for head in &mut self.heads {
                    if let Some(v) = head.views.get_mut(&key) {
                        self.presented = true;
                        // A key's window on the display closes (ADR-1401
                        // D2).
                        v.flipped(&head.console.term);
                        if let Some(t) = v.timer.as_mut() {
                            t.flip();
                        }
                    }
                }
            }
            // A re-take's set-up keeps the blank and its timer: a retry
            // that lands after the blank powers the displays down again.
            Event::Awake if self.retaking.remove(node) => {
                if self.blank.blanked()
                    && let Some(video) = self.videos.get_mut(node)
                {
                    power_down(video.as_mut());
                }
            }
            Event::Awake => {
                // The card lists again the displays it left dark, which
                // the notices read (ADR-1401 D3).
                if !self.notices.is_empty() {
                    self.guard_due = true;
                }
                self.user_input(now);
            }
            Event::Asleep => {
                if let Some(r) = &mut self.releasing {
                    r.remove(node);
                }
            }
            Event::WakeFailed => {
                self.retaking.remove(node);
                // A DRM card goes on trying once a second while its VT is
                // shown; a framebuffer waits for the next key.
                let slowly = self
                    .videos
                    .get(node)
                    .is_some_and(|video| video.backend() == "drm2d");
                if slowly {
                    log::warn!(
                        "{}: the displays did not wake. vtium tries again once a second while this VT is shown, and at once at the next key press",
                        node.display()
                    );
                } else {
                    log::warn!(
                        "{}: the displays did not wake. vtium tries again at the next key press",
                        node.display()
                    );
                }
            }
        }
    }

    /// The device events of one turn. Each card scans once for all the
    /// hotplugs it had in the turn. Between a card's display work and the
    /// next, the loop reads a request for the VT, and once one comes it
    /// lets the VT go in this turn and skips the scans that remain. The
    /// VT comes back to vtium only through an acquire, whose wake-up scans
    /// every card, so a skipped scan needs no record (ADR-1401 D2, "A VT
    /// release while display work runs").
    pub(super) fn device_events(&mut self, events: Vec<DeviceEvent>, now: Instant) {
        let mut hotplugs: Vec<PathBuf> = Vec::new();
        for e in events {
            match e {
                DeviceEvent::Hotplug { node, .. } => {
                    if !hotplugs.contains(&node) {
                        hotplugs.push(node);
                    }
                }
                DeviceEvent::Added(dev) if dev.kind != DeviceKind::Input => {
                    // A card added wakes up, and so reads its connectors,
                    // only while the VT is vtium's.
                    self.release_now(now);
                    self.device_event(DeviceEvent::Added(dev));
                }
                e => self.device_event(e),
            }
        }
        for node in hotplugs {
            if self.release_now(now) {
                continue;
            }
            if let Some(video) = self.videos.get_mut(&node)
                && let Err(e) = video.hotplug()
            {
                log::warn!("{}: hotplug: {e}", node.display());
            }
        }
        self.release_now(now);
    }

    /// Reads a request for the VT that waits, as a card's display work may
    /// have stopped for, and handles it now, so the turn lets the VT go.
    /// Returns whether the VT is being released.
    fn release_now(&mut self, now: Instant) -> bool {
        let asked = self.releasing.is_none()
            && self.vt.is_active()
            && self.vt.release_check().is_some_and(|asked| asked());
        if asked {
            self.vt.take_release();
            let events: Vec<VtEvent> = self.vt.events().collect();
            for e in events {
                self.vt_event(e, now);
            }
        }
        self.releasing.is_some()
    }

    /// A device of the seat came, went or changed. A framebuffer a DRM
    /// driver backs is skipped for its card.
    pub(super) fn device_event(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Added(dev) => match dev.kind {
                DeviceKind::Input => {
                    let identity = dev
                        .input
                        .as_deref()
                        .map_or_else(vtium_settings::Identity::input, vtium_devices::input);
                    self.add_input(&dev.node, identity);
                }
                DeviceKind::Fbdev if dev.flags.drm_backed => {
                    log::debug!("{}: backed by a DRM card, skipped", dev.node.display());
                }
                kind => {
                    let card = Card::at(&dev.node, dev.id_path.as_deref(), dev.flags.primary);
                    self.add_video_device(&dev.node, kind, card);
                }
            },
            DeviceEvent::Removed { kind, node } => match kind {
                DeviceKind::Input => self.remove_input(&node),
                DeviceKind::Drm | DeviceKind::Fbdev => {
                    if self.videos.contains_key(&node) {
                        let what = if kind == DeviceKind::Drm {
                            "card"
                        } else {
                            "framebuffer"
                        };
                        log::info!(
                            "{}: udev reports the {what} removed, and vtium released the {what}",
                            node.display()
                        );
                    }
                    self.remove_video(&node);
                }
            },
            DeviceEvent::Hotplug { node, .. } => {
                if let Some(video) = self.videos.get_mut(&node)
                    && let Err(e) = video.hotplug()
                {
                    log::warn!("{}: hotplug: {e}", node.display());
                }
            }
        }
    }

    /// Opens a video device udev reported. One the seat manager is not
    /// ready to give waits, and opens once the VT names it settled
    /// ([`App::settled_videos`]).
    fn add_video_device(&mut self, node: &Path, kind: DeviceKind, card: Card) {
        match open_video(self, node, kind, card.clone()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                log::debug!("{} waits for the seat manager", node.display());
                self.waiting_videos.insert(node.to_owned(), (kind, card));
            }
            Err(e) => log::warn!("cannot open {}: {e}", node.display()),
        }
    }

    /// The waiting video devices whose open the VT has settled, opened now
    /// or reported as failed.
    pub(super) fn settled_videos(&mut self) {
        for node in self.vt.settled() {
            if let Some((kind, card)) = self.waiting_videos.remove(&node) {
                self.add_video_device(&node, kind, card);
            }
        }
    }
}

/// Opens a device through the VT and wraps it as a video device in the
/// loop, for [`App::add_video`], keeping what the rules read of its
/// `card`. A device opened that cannot be wrapped is closed through the VT
/// again.
pub fn open_video(app: &mut App, node: &Path, kind: DeviceKind, card: Card) -> io::Result<()> {
    let fd = app.vt.open_device(node)?;
    wrap_video(app, node, kind, card, fd).inspect_err(|_| app.vt.close_device(node))
}

/// The device `fd`, opened from `node`, wrapped as a video device.
fn wrap_video(
    app: &mut App,
    node: &Path,
    kind: DeviceKind,
    card: Card,
    fd: std::os::fd::OwnedFd,
) -> io::Result<()> {
    use vtium_platform::display::{Card as Drm, Drm2d, Fb, Fbdev, ModeLookup, ModePolicy, Wanted};
    let key = node.to_owned();
    let video: Box<dyn Video> = match kind {
        DeviceKind::Drm => {
            let modes = app.modes.clone();
            let gpu = card.id_path.clone();
            let policy = ModePolicy {
                use_original: app.use_original,
                wanted: (!modes.is_empty()).then(|| {
                    Box::new(move |connector: &str| {
                        ModeSpec::find(&modes, gpu.as_deref(), connector).map(|m| Wanted {
                            width: m.width,
                            height: m.height,
                            rate: m.rate,
                        })
                    }) as Box<ModeLookup>
                }),
            };
            let mut drm = Drm2d::new(Drm::new(fd)?, policy, app.display_buffers)?;
            drm.set_card_path(card.id_path.clone());
            // Under the privilege split, vtium's root process sets and
            // drops DRM master on its own copy of the card.
            //
            // References:
            // - ADR-1400 D4, "What the broker does": the broker holds DRM
            //   master for the drawer.
            #[cfg(feature = "split")]
            drm.set_master_keeper(app.vt.master_keeper(node));
            // A display the rules leave dark is set off before it is
            // announced, a frame another master left included (D3).
            let (rules, at, lit) = (app.rules.clone(), card.clone(), Rc::clone(&app.any_lit));
            let vt_heads = app.vt_heads.clone();
            let (wanted, node) = (Rc::clone(&app.wanted), key.clone());
            drm.set_dark(Box::new(move |named| {
                // While none is lit, it is announced for the display guard
                // to light (D9), which leaves dark again the ones it lights
                // not. One asked for its notice is announced (D3).
                if !lit.get()
                    || wanted
                        .borrow()
                        .contains(&(node.clone(), named.connector.clone()))
                {
                    return false;
                }
                let display = vtium_devices::display(named, &at, 0);
                place_display(&rules, &vt_heads, &display)
                    .map_err(|line| log::info!("{line}"))
                    .is_err()
            }));
            let k = key.clone();
            drm.insert(&app.handle, move |app: &mut App| {
                app.videos
                    .get_mut(&k)
                    .and_then(|v| v.as_any().downcast_mut::<Drm2d<Drm>>())
            })?;
            Box::new(drm)
        }
        DeviceKind::Fbdev => {
            let mut fb = Fbdev::new(Fb::new(fd));
            let k = key.clone();
            fb.insert(&app.handle, move |app: &mut App| {
                app.videos
                    .get_mut(&k)
                    .and_then(|v| v.as_any().downcast_mut::<Fbdev<Fb>>())
            });
            Box::new(fb)
        }
        DeviceKind::Input => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
    };
    app.cards.insert(key.clone(), card);
    app.add_video(key, video);
    Ok(())
}

/// A display's depth: `display-depth`, else, at `auto`, the levels the
/// device says it shows.
pub(super) fn depth(config: &Config, levels: Option<u8>) -> Depth {
    match config.text(&keys::DISPLAY_DEPTH) {
        Some("1") => Depth::One,
        Some("gray4") => Depth::Gray(4),
        Some("gray16") => Depth::Gray(16),
        Some("color") => Depth::Color,
        _ => match levels {
            None => Depth::Color,
            Some(2) => Depth::One,
            Some(n) => Depth::Gray(n),
        },
    }
}
