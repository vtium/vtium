//! The scan of the connectors: the displays gone, each new connector's
//! identity and modes, the display bound to it, and the placement of the
//! displays on CRTCs and their planes.

use std::io;

use rustix::io::Errno;

use super::super::edid::{Edid, EdidId};
use super::super::kms::{Connector, Encoder, Kms, Object, Property, Resources};
use super::super::mode::{self, Choice};
use super::super::{Ages, Damage, DisplayId, Dpms, Event, Identity, path};
use super::buffers::Source;
use super::dark::Dark;
use super::matching::{Place, index_bit, matching};
#[cfg(feature = "scroll-pan")]
use super::output;
use super::output::{
    ColorProps, Moved, Output, PLANE_CURSOR, PLANE_PRIMARY, Planes, Props, connector_name, value,
};
use super::{
    Drm2d, Found, Master, New, Placed, Scan, card_removed, drop_cursor, physical_size, released,
    same_monitor,
};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// The scan: lets go of the displays gone, binds a display to each new
    /// connected connector with modes, and sets modes as `scan` asks. The
    /// displays gone let their CRTCs go first, so a connector that comes
    /// back under a new id in one scan, as a DisplayPort multi-stream
    /// connector does when its monitor drops for a moment, can take the
    /// CRTC its old display held (vtium/vtium#252).
    pub(super) fn scan(&mut self, scan: Scan) -> io::Result<()> {
        if !self.awake {
            return Ok(());
        }
        // The card takes master as it wakes and lets it go only as it
        // falls asleep, so a scan always runs under master.
        debug_assert_eq!(self.master, Master::Held, "an awake card holds DRM master");
        let res = self.card.resources()?;
        #[cfg(feature = "scroll-pan")]
        {
            self.max_fb = (res.max_width, res.max_height);
        }
        // Every connector is read before the scan changes anything, and
        // the loop is asked between the reads whether the kernel wants
        // the VT, so a release stops the scan there with nothing changed.
        // The wake-up at an acquire probes every connector, so it finds a
        // monitor plugged in where the kernel detects none by itself. No
        // program waits on vtium then, and the release check below still
        // stops the scan between probes. The first wake-up and a hotplug probe only
        // the connectors that may have a monitor (ADR-1401 D2).
        let all = scan == Scan::WakeUp && self.scanned;
        let mut read = Vec::with_capacity(res.connectors.len());
        for &id in &res.connectors {
            if self.interrupted() {
                return Err(released());
            }
            let conn = self.connector(id, all);
            // A connector the removed card cannot read is not unplugged.
            if conn.is_none() && self.card.removed() {
                return Err(Errno::NODEV.into());
            }
            read.push((id, conn));
        }
        self.crtcs.clone_from(&res.crtcs);
        for o in &mut self.outputs {
            o.available = false;
        }
        for d in &mut self.dark {
            d.available = false;
        }
        for l in &mut self.left_off {
            l.available = false;
        }
        let mut needs = scan != Scan::Hotplug;
        // The set of displays changed: one was bound or let go.
        let mut changed = false;
        // A wake-up after the first scan places each display bound before
        // on the CRTC its connector is on now, from the connected
        // connectors it reads. The first scan binds every display afresh.
        // Every later scan reads which CRTCs each display bound before can
        // drive from those connectors, so it can move one ([`Self::place`]).
        let follow = scan == Scan::WakeUp && self.scanned;
        let mut seen = Vec::new();
        let mut new = Vec::new();
        let mut connected = Vec::new();
        for (id, conn) in read {
            let Some(conn) = conn else {
                continue;
            };
            if !conn.connected || conn.modes.is_empty() {
                continue;
            }
            connected.push(id);
            if self.scanned {
                seen.push(conn.clone());
            }
            // A connector that shows another monitor than the one its
            // display or dark entry was bound to, as when a monitor is
            // swapped faster than its disconnect is seen, is let go and
            // bound again below, as the new monitor, so the head file's
            // rules, the dark rule, its density and its mode follow it.
            let swapped = self
                .outputs
                .iter()
                .find(|o| o.id.0 == id)
                .is_some_and(|o| self.another_monitor(&o.name, self.known_edid(o.id)));
            if swapped {
                let (gone, kept) = std::mem::take(&mut self.outputs)
                    .into_iter()
                    .partition(|o| o.id.0 == id);
                self.outputs = kept;
                for o in gone {
                    log::info!("display {} shows another monitor", o.name);
                    if o.announced {
                        self.events.push(Event::Removed(o.id));
                    }
                    self.identities.retain(|(i, _)| *i != o.id);
                    self.free(o);
                    changed = true;
                }
            } else if self.dark.iter().find(|d| d.id == id).is_some_and(|d| {
                self.another_monitor(&d.identity.connector, d.identity.edid.as_ref())
            }) {
                self.dark.retain(|d| {
                    if d.id == id {
                        log::info!(
                            "display {}, left dark, shows another monitor",
                            d.identity.connector
                        );
                    }
                    d.id != id
                });
            }
            self.see_left_off(id);
            if let Some(o) = self.outputs.iter_mut().find(|o| o.id.0 == id) {
                o.available = true;
                // A display the last legacy modeset could not set is set
                // again at each scan, so a hotplug lights it where the card
                // now takes it.
                needs |= !o.online || self.unset.contains(&o.id);
                // The same monitor listing other modes than those the
                // display's mode was chosen from, as after a DisplayPort
                // link is trained again, gets its mode chosen again from
                // the new list. Under `use-original-mode` the mode vtium
                // set stands in for the mode the console left, which is
                // gone by now, so the display keeps its mode where the new
                // list has it.
                if conn.modes != o.modes {
                    let wanted = self.policy.wanted.as_ref().and_then(|f| f(&o.name));
                    let use_original = self.policy.use_original;
                    if let Some(choice) =
                        mode::choose(&conn.modes, Some(&o.mode), wanted, use_original)
                    {
                        if choice.current != o.mode {
                            let (w, h) = choice.current.size();
                            log::info!("display {} lists new modes: setting {w}x{h}", o.name);
                            o.mode = choice.current;
                            needs = true;
                        }
                        o.choice = choice;
                        o.held = None;
                        o.owed = None;
                    }
                    o.modes = conn.modes;
                }
                continue;
            }
            if let Some(d) = self.dark.iter_mut().find(|d| d.id == id) {
                d.available = true;
                continue;
            }
            // A connector whose properties cannot be read is left unbound
            // and read again at the next scan: its `PATH` belongs to its
            // identity, and an atomic commit needs its `CRTC_ID`.
            let found = match self.find(&conn) {
                Ok(found) => found,
                Err(e) if card_removed(&e) => return Err(e),
                Err(e) => {
                    log::warn!(
                        "cannot read the properties of connector {}: {e}",
                        connector_name(conn.kind, conn.kind_id)
                    );
                    continue;
                }
            };
            let identity = self.identify(&found);
            if self
                .dark_policy
                .as_ref()
                .is_some_and(|dark| dark(&identity))
            {
                log::info!("display {} is left dark", found.name);
                self.darken_left_off(id);
                self.dark.push(Dark {
                    id,
                    identity,
                    available: true,
                });
                needs = true;
                continue;
            }
            if let Some(n) = self.candidate(conn, found, identity) {
                new.push(n);
            }
        }
        self.dark.retain(|d| {
            if !d.available {
                log::info!("display {}, left dark, is gone", d.identity.connector);
            }
            d.available
        });
        self.drop_left_off_gone();
        self.dark
            .sort_by_key(|d| res.connectors.iter().position(|&c| c == d.id));
        let (kept, gone): (Vec<_>, Vec<_>) = self.outputs.drain(..).partition(|o| o.available);
        self.outputs = kept;
        // The CRTC each display gone held, with its identity, so a monitor
        // that comes back under a new connector id takes it back.
        let mut freed = Vec::new();
        for o in gone {
            if let Some((_, identity)) = self.identities.iter().find(|(i, _)| *i == o.id) {
                freed.push((identity.clone(), o.crtc));
            }
            log::info!("display {} is gone", o.name);
            if o.announced {
                self.events.push(Event::Removed(o.id));
            }
            let (crtc, name) = (o.crtc, o.name.clone());
            self.free(o);
            self.legacy_gone_off(crtc, &name);
            changed = true;
        }
        self.move_refused.retain(|id| connected.contains(id));
        // Blanked, a hotplug's modeset waits for the input that ends the
        // blank, so no lit display moves at it: a display that drew on
        // with its record moved would flip on a CRTC its connector is not
        // on. A new display that could light only by moving one waits for
        // that input too, whose scan places it.
        let defer = self.blanked && scan != Scan::WakeUp;
        let placed = self.place(&res, &seen, new, &freed, follow, defer);
        if self.card.removed() {
            return Err(Errno::NODEV.into());
        }
        if placed.changed {
            needs = true;
            changed = true;
        }
        if defer && placed.unplaced {
            self.owed = true;
        }
        self.scanned = true;
        // A mode held back is tried again as the card wakes, as another
        // program may have changed what the card can set, and when the set
        // of displays changes, which frees or takes the card's bandwidth.
        if scan == Scan::WakeUp || changed {
            for o in &mut self.outputs {
                if let Some(held) = o.held.take() {
                    o.mode = held;
                    needs = true;
                }
            }
        }
        // A mode owed is tried again by the retries and as the card wakes.
        // It stays owed until a modeset sets it, so a modeset that waits
        // for a flip keeps the retries running.
        if scan != Scan::Hotplug {
            for o in &mut self.outputs {
                if let Some(owed) = o.owed {
                    o.mode = owed;
                }
            }
        }
        let outputs = &self.outputs;
        self.identities
            .retain(|(id, _)| outputs.iter().any(|o| o.id == *id));
        // With no display the card keeps master while its VT is shown, so
        // the kernel's console, which lights a display at a hotplug while
        // no process holds master, never takes one plugged in meanwhile.
        if self.outputs.is_empty() {
            if needs && !self.dark.is_empty() {
                self.darken();
            }
            return Ok(());
        }
        if needs && defer {
            // Blanked, a hotplug commits nothing: the modeset waits for the
            // input that ends the blank, and a display plugged in stays
            // dark until then (the operator's decision on vtium/vtium#18).
            log::debug!("blanked: the hotplug's modeset waits for input");
            self.owed = true;
            Ok(())
        } else if needs {
            self.request_modeset(scan == Scan::WakeUp)
        } else {
            self.announce();
            Ok(())
        }
    }

    /// The monitor display `id` was bound to, as its EDID named it.
    pub(super) fn known_edid(&self, id: DisplayId) -> Option<&EdidId> {
        self.identities
            .iter()
            .find(|(i, _)| *i == id)
            .and_then(|(_, identity)| identity.edid.as_ref())
    }

    /// Whether connector `name`'s EDID in sysfs now names another monitor
    /// than `known`. A connector whose EDID cannot be read, as for a moment
    /// while a monitor is plugged in, counts as showing the same one.
    pub(super) fn another_monitor(&self, name: &str, known: Option<&EdidId>) -> bool {
        self.card
            .sysfs()
            .and_then(|dir| Edid::sysfs(&dir, name))
            .is_some_and(|now| known != Some(&now.id))
    }

    /// Connector `id`, or `None` where the card cannot give it. One gone
    /// since the card listed it, as during a monitor's power cycle, is
    /// logged at debug level; any other failure, a read refused because
    /// the connector changed between its two reads among them, is a
    /// warning.
    ///
    /// The connector is read first as the kernel last detected it. Only a
    /// connector the kernel reports a monitor on, or has not checked yet,
    /// is read again with a probe, and every connector when `all` asks, as
    /// the wake-up at an acquire does. A probe of a connector with no
    /// monitor can take seconds, as with a DisplayPort converter chip that
    /// does not answer, and holds the loop meanwhile (ADR-1401 D2).
    fn connector(&self, id: u32, all: bool) -> Option<Connector> {
        let current = self.read_logged(id, self.card.connector_current(id))?;
        if !all && !current.connected && !current.unknown {
            return Some(current);
        }
        self.read_logged(id, self.card.connector(id))
    }

    /// Whether the loop says the kernel has asked for the VT, which stops
    /// the display work at the step that asks ([`Drm2d::set_interrupt`]).
    pub(super) fn interrupted(&self) -> bool {
        let asked = self.interrupt.as_ref().is_some_and(|asked| asked());
        if asked {
            log::debug!("the kernel asks for the VT, so the card stops its display work");
        }
        asked
    }

    /// What a read of connector `id` gave, or `None`, logged as
    /// [`Self::connector`] logs it.
    pub(super) fn read_logged<T>(&self, id: u32, read: io::Result<T>) -> Option<T> {
        read.inspect_err(|e| {
            if card_removed(e) {
            } else if e.kind() == io::ErrorKind::NotFound {
                log::debug!("connector {id} is gone: {e}");
            } else {
                log::warn!("cannot read connector {id}: {e}");
            }
        })
        .ok()
    }

    /// What a scan finds of connector `conn`, or the failure to read its
    /// properties.
    fn find(&self, conn: &Connector) -> io::Result<Found> {
        let name = connector_name(conn.kind, conn.kind_id);
        let current = (conn.encoder != 0)
            .then(|| self.encoder(conn.encoder))
            .flatten();
        let shown = current.filter(|e| e.crtc != 0).and_then(|e| {
            self.card
                .crtc_mode(e.crtc)
                .inspect_err(|err| {
                    log::warn!("cannot read the mode of CRTC {}: {err}", e.crtc);
                })
                .ok()
                .flatten()
        });
        if self.card.removed() {
            return Err(Errno::NODEV.into());
        }
        let props = self.card.properties(conn.id, Object::Connector)?;
        let on = match current.map(|e| e.crtc) {
            _ if self.legacy => value(&props, "DPMS").is_none_or(|d| d == 0),
            Some(crtc) => self
                .card
                .properties(crtc, Object::Crtc)
                .inspect_err(|e| {
                    log::warn!("cannot read the properties of CRTC {crtc}: {e}");
                })
                .ok()
                .and_then(|p| value(&p, "ACTIVE"))
                .is_none_or(|a| a != 0),
            None => false,
        };
        let edid = self.card.sysfs().and_then(|dir| Edid::sysfs(&dir, &name));
        let path = self.path(&name, &props);
        Ok(Found {
            name,
            current,
            visible: on && shown.is_some(),
            shown,
            props,
            edid,
            path,
        })
    }

    /// The text of connector `name`'s `PATH` property, read from the blob
    /// its value names in `props`; `None` where it has no `PATH`, its
    /// value names no blob (0), or the card cannot give the blob.
    fn path(&self, name: &str, props: &[Property]) -> Option<String> {
        let blob = value(props, "PATH")
            .filter(|&b| b != 0)
            .and_then(|b| u32::try_from(b).ok())?;
        let bytes = self
            .card
            .blob(blob)
            .inspect_err(|e| log::warn!("cannot read the PATH of connector {name}: {e}"))
            .ok()?;
        path::text(&bytes)
    }

    /// The identity of the connector `found` describes.
    fn identify(&self, found: &Found) -> Identity {
        Identity {
            connector: found.name.clone(),
            card: self.card_path.clone(),
            edid: found.edid.as_ref().map(|e| e.id.clone()),
            connector_path: found.path.clone(),
            lit_at_start: !self.scanned && found.visible,
        }
    }

    /// Connector `conn` as a display the scan binds, with the identity
    /// `identity`, or `None` where no mode of its can be chosen.
    fn candidate(&self, conn: Connector, found: Found, identity: Identity) -> Option<New> {
        let wanted = self.policy.wanted.as_ref().and_then(|f| f(&found.name));
        let choice = mode::choose(
            &conn.modes,
            found.shown.as_ref(),
            wanted,
            self.policy.use_original,
        )?;
        Some(New {
            conn,
            found,
            identity,
            choice,
        })
    }

    /// A display for connector `conn` on `crtc`, at `pipe` in the card's
    /// list of CRTCs, at the mode `choice`, or `None` when that CRTC has no
    /// primary plane.
    fn bind(
        &self,
        conn: &Connector,
        found: Found,
        choice: Choice,
        crtc: u32,
        pipe: usize,
    ) -> Option<Output<S::Memory>> {
        let Found {
            name,
            props: connector_props,
            edid,
            ..
        } = found;
        // Its modeset turns the display on, whatever DPMS the connector was
        // left at.
        let dpms = connector_props
            .iter()
            .find(|p| p.name == "DPMS")
            .map(|p| (p.id, Dpms::On));
        let planes = if self.legacy {
            None
        } else {
            let Some(mut planes) = self.find_planes(crtc, pipe) else {
                log::error!("no primary plane for {name}");
                return None;
            };
            planes.connector_props = Props::new(&connector_props);
            Some(planes)
        };
        log::info!(
            "display {name} at {}x{}",
            choice.current.size().0,
            choice.current.size().1
        );
        let mm = physical_size(&name, conn.mm, edid.as_ref());
        // Where the range of `max bpc` cannot be read, the modeset sets the
        // bits vtium draws, and a card that refuses them sets the mode
        // without the color properties (ADR-1401 D14).
        let color = ColorProps::new(&connector_props, |id| {
            self.card
                .property_range(id)
                .inspect_err(|e| log::warn!("cannot read the range of max bpc of {name}: {e}"))
                .ok()
                .flatten()
        });
        Some(Output {
            id: DisplayId(conn.id),
            name,
            crtc,
            pipe,
            able: index_bit(pipe),
            planes,
            color,
            mode: choice.current,
            moved_from: None,
            set: None,
            held: None,
            owed: None,
            mm,
            choice,
            modes: conn.modes.clone(),
            mode_blob: 0,
            buffers: Vec::new(),
            back: 0,
            ages: Ages::default(),
            damage: Damage::default(),
            loan: None,
            lease: 0,
            clear_owed: false,
            fresh: false,
            cursor: None,
            cursor_set: None,
            dpms,
            down: false,
            online: false,
            redraw: false,
            flipping: false,
            vblank: false,
            flips: 0,
            flip_crtc: crtc,
            flip_timer: None,
            modeset_flip: false,
            modeset_flip_no: None,
            available: true,
            announced: false,
            returning: self.scanned,
            #[cfg(feature = "scroll-pan")]
            pan: output::Pan::default(),
        })
    }

    /// An encoder, or `None`, logged, when it cannot be read.
    pub(super) fn encoder(&self, id: u32) -> Option<Encoder> {
        if self.card.removed() {
            return None;
        }
        self.card
            .encoder(id)
            .inspect_err(|e| {
                if !card_removed(e) {
                    log::warn!("cannot read encoder {id}: {e}");
                }
            })
            .ok()
    }

    /// The CRTCs an encoder of connector `conn` can drive, a bit per index
    /// into the card's list of CRTCs; `current` is the encoder it is on, as
    /// read with it. An encoder that cannot be read adds none.
    fn reach(&self, conn: &Connector, current: Option<Encoder>) -> u32 {
        conn.encoders
            .iter()
            .filter_map(|&e| {
                if e == conn.encoder {
                    current
                } else {
                    self.encoder(e)
                }
            })
            .fold(0, |all, e| all | e.possible_crtcs)
    }

    /// Places the displays on CRTCs, one CRTC each: the new ones, `new`,
    /// and the ones bound before. Binds a display to each new connector
    /// placed, moves each display bound before to the CRTC it is placed
    /// on, and says whether it bound, moved or let go of any display and
    /// whether it left a new display without a CRTC.
    ///
    /// Every scan places the displays together, wherever the card has a
    /// CRTC for each. At a hotplug, and at the retry of a failed one, a lit
    /// display bound before stays on its CRTC whenever every new display
    /// can light without it moving. Where a new display can light only on a
    /// CRTC that a lit display holds, and that lit display can move to
    /// another CRTC its connector's encoders drive, the lit display moves
    /// and the new display takes its CRTC, and the placement moves as few
    /// lit displays as it can. The modeset after the scan sets the moved
    /// display's mode on its new CRTC anew, so it blanks once while it
    /// moves and then lights beside the new one, as a desktop such as KWin
    /// does at a hotplug (the maintainer's decision on vtium/vtium#268).
    /// Each display bound before that the placement puts off the CRTC it
    /// wants remembers that CRTC (`moved_from`), and whether it was lit
    /// there, until a modeset sets it on its new CRTC; a modeset that
    /// cannot set the displays in their new places moves it back
    /// ([`Self::move_back`]). A new display that no placement gives a CRTC
    /// is not bound, with an error, nothing moves for it, and the next scan
    /// tries it again.
    ///
    /// At a hotplug or a retry, a display online wants the CRTC vtium last
    /// set it on, which is the one it was moved off where a move still
    /// waits for its modeset. So a move whose reason is gone before its
    /// modeset, as when the new display is unplugged first, is undone
    /// without blanking the display.
    ///
    /// Where `pin` is set, as while the scan's modeset waits for the input
    /// that ends a blank, no display bound before moves at a hotplug or a
    /// retry. A new display the card refused beside the others even with
    /// lit displays moved for it (`move_refused`) moves no display: it is
    /// placed after the others, on a CRTC none of them takes.
    ///
    /// The displays are placed together, not each on its own as it is
    /// bound. Placed one at a time, the first display at the first scan
    /// could take the only CRTC a later one can drive, and a display
    /// plugged in would stay dark where a lit display held the only CRTCs
    /// it can drive.
    ///
    /// A wake-up after the first scan (`follow`) places each display bound
    /// before on the CRTC its connector is on now, read from the connectors
    /// `seen` as the wake-up read them. The program that held the card while
    /// vtium slept may have moved a connector to another CRTC. A modeset on
    /// the CRTC the display had would take the connector off a CRTC that
    /// program left lit, and the kernel refuses a lit CRTC with no connector
    /// whatever mode the commit sets (`drm_atomic_helper_check_modeset`), so
    /// the display would never wake again (vtium/vtium#249).
    ///
    /// Each display bound before wants one CRTC. At a wake-up that is the
    /// CRTC its connector is on, or, where its connector is on no CRTC or
    /// the display returns, its own CRTC; at any other scan it is the CRTC
    /// above. It can take that CRTC and any CRTC its connector's encoders
    /// drive, as read from the connectors `seen`. A new display wants the
    /// CRTC a display gone in this scan held (`freed`), where that display
    /// showed the same monitor ([`same_monitor`]) and no new display placed
    /// before it took that CRTC back, as a DisplayPort multi-stream monitor
    /// that comes back under a new connector id does; else it wants the
    /// CRTC its connector is on. A display that returns, as each display
    /// new after the first scan does, has its first modeset turn off the
    /// CRTC its connector is on ([`Self::set_off_first`]).
    ///
    /// The displays counted lit are placed first: at a wake-up and at the
    /// first scan those whose connector is on a CRTC, and at any other scan
    /// those bound before that are online. At a wake-up and at the first
    /// scan the displays whose connector is on no CRTC come next, and the
    /// rest last; at any other scan the rest come next. Within a group the
    /// displays bound before come first, then the new ones, each in the
    /// card's connector order. The matching ([`matching`]) places them one
    /// at a time, and of the placements of the displays it can place, it
    /// takes one that puts as few lit displays as it can off the CRTC they
    /// want, and then as few of the others.
    ///
    /// The CRTCs the displays bound before hold are a placement of them
    /// all, so the matching always finds one for them, and a later step
    /// only moves them. A display bound before whose new CRTC has no
    /// primary plane stays off, as one the scan cannot bind does, and the
    /// owner hears it removed. A new display the matching could not place
    /// is not bound, with an error, and the next scan tries it again.
    fn place(
        &mut self,
        res: &Resources,
        seen: &[Connector],
        new: Vec<New>,
        freed: &[(Identity, u32)],
        follow: bool,
        pin: bool,
    ) -> Placed {
        let index = |crtc: u32| res.crtcs.iter().position(|&c| c == crtc);
        let bit = |crtc: u32| index(crtc).map_or(0, index_bit);
        let olds = std::mem::take(&mut self.outputs);
        // Each CRTC a display gone in this scan held, until a new display
        // claims it.
        let mut freed: Vec<Option<&(Identity, u32)>> = freed.iter().map(Some).collect();
        let mut places = Vec::with_capacity(olds.len().saturating_add(new.len()));
        // The CRTC each display wants, and whether it is lit there.
        let mut homes = Vec::with_capacity(olds.len());
        // The CRTCs each display can take, for a modeset the card refuses
        // ([`Self::relight`]).
        let mut ables = Vec::with_capacity(olds.len().saturating_add(new.len()));
        for o in &olds {
            let conn = seen.iter().find(|c| c.id == o.id.0);
            let current = conn
                .filter(|c| c.encoder != 0)
                .and_then(|c| self.encoder(c.encoder));
            let reach = conn.map_or(0, |c| self.reach(c, current)) | bit(o.crtc);
            ables.push(reach | o.moved_from.map_or(0, |m| bit(m.crtc)));
            if !follow {
                let want = o.moved_from.filter(|_| o.online).map_or(o.crtc, |m| m.crtc);
                // Lit here means online. Only the displays online before the
                // modeset can go back ([`Self::going_back`]), so every display
                // whose record `going_back` reads is lit, and where two at a
                // hotplug would go back to one CRTC, connector order decides.
                homes.push((want, o.online));
                places.push(Place {
                    group: if o.online { 0 } else { 1 },
                    want: index(want),
                    able: if pin { bit(want) } else { reach | bit(want) },
                });
                continue;
            }
            let on = current.map_or(0, |e| e.crtc);
            let (group, want) = match on {
                _ if o.returning => (2, o.crtc),
                0 => (1, o.crtc),
                on => (0, on),
            };
            homes.push((want, group == 0));
            places.push(Place {
                group,
                want: index(want),
                able: reach | bit(on),
            });
        }
        for n in &new {
            let on = n.found.current.map_or(0, |e| e.crtc);
            let group = match on {
                _ if self.scanned => 2,
                0 => 1,
                _ => 0,
            };
            let able = self.reach(&n.conn, n.found.current) | bit(on);
            ables.push(able);
            let back = freed
                .iter_mut()
                .find(|slot| {
                    slot.is_some_and(|&(ref was, crtc)| {
                        same_monitor(was, &n.identity) && able & bit(crtc) != 0
                    })
                })
                .and_then(Option::take)
                .map(|&(_, crtc)| crtc);
            places.push(Place {
                group,
                want: index(back.unwrap_or(on)),
                able,
            });
        }
        // The new displays the card refused even with lit displays moved
        // for them take part in the matching only after it, on the CRTCs
        // left free.
        let refused: Vec<usize> = new
            .iter()
            .enumerate()
            .filter(|(_, n)| self.move_refused.contains(&n.conn.id))
            .map(|(i, _)| i.saturating_add(olds.len()))
            .collect();
        let mut matched = places.clone();
        for &d in &refused {
            if let Some(p) = matched.get_mut(d) {
                p.able = 0;
            }
        }
        let mut slots = matching(&matched, res.crtcs.len());
        for &d in &refused {
            let Some(p) = places.get(d) else {
                continue;
            };
            let free = |c: &usize| p.able & index_bit(*c) != 0 && !slots.contains(&Some(*c));
            let take = p
                .want
                .filter(free)
                .or_else(|| (0..res.crtcs.len()).find(free));
            if let Some(slot) = slots.get_mut(d) {
                *slot = take;
            }
        }
        let mut slots = slots.into_iter();
        let at = |slot: Option<usize>| slot.and_then(|c| Some((*res.crtcs.get(c)?, c)));
        let mut changed = false;
        let mut unplaced = false;
        let mut ables = ables.into_iter();
        for ((mut o, slot), (want, lit)) in olds.into_iter().zip(slots.by_ref()).zip(homes) {
            o.able = ables.next().unwrap_or(0) | bit(want);
            // A display placed off the CRTC it wants is moved off it for
            // another display, until a modeset sets it on its new CRTC.
            o.moved_from = at(index(want))
                .filter(|&(_, pipe)| Some(pipe) != slot)
                .map(|(crtc, pipe)| Moved { crtc, pipe, lit });
            let placed = match at(slot) {
                None => Err(format!("no free CRTC for {}", o.name)),
                Some((crtc, _)) if crtc == o.crtc => {
                    self.outputs.push(o);
                    continue;
                }
                Some((crtc, pipe)) => self.move_to(&mut o, crtc, pipe),
            };
            changed = true;
            match placed {
                Ok(()) => self.outputs.push(o),
                Err(why) => self.let_go(o, &why),
            }
        }
        for ((n, slot), able) in new.into_iter().zip(slots).zip(ables) {
            let New {
                conn,
                found,
                identity,
                choice,
            } = n;
            let Some((crtc, pipe)) = at(slot) else {
                log::error!("no free CRTC for {}", found.name);
                unplaced = true;
                continue;
            };
            if let Some(mut o) = self.bind(&conn, found, choice, crtc, pipe) {
                o.able = able | index_bit(pipe);
                self.identities.retain(|(i, _)| *i != o.id);
                self.identities.push((o.id, identity));
                self.outputs.push(o);
                changed = true;
            }
        }
        self.outputs
            .sort_by_key(|o| res.connectors.iter().position(|&c| c == o.id.0));
        Placed { changed, unplaced }
    }

    /// Moves display `o` to `crtc`, at `pipe` in the card's list of CRTCs,
    /// with that CRTC's planes, or says why it cannot. Where the new CRTC
    /// has no cursor plane, the display gives its pointer image up, and the
    /// owner draws the pointer itself. Where the display asked for tall
    /// buffers, the next modeset tests the pan on the new primary plane.
    ///
    /// References:
    /// - ADR-1401 D2, "A buffer taller than the screen": the pan is tested
    ///   where the display is set up.
    pub(super) fn move_to(
        &self,
        o: &mut Output<S::Memory>,
        crtc: u32,
        pipe: usize,
    ) -> Result<(), String> {
        if !self.legacy {
            let mut planes = self
                .find_planes(crtc, pipe)
                .ok_or_else(|| format!("no primary plane for {} on CRTC {crtc}", o.name))?;
            planes.connector_props = o
                .planes
                .take()
                .map(|p| p.connector_props)
                .unwrap_or_default();
            if planes.cursor.is_none() {
                drop_cursor(&self.card, o);
            }
            o.planes = Some(planes);
            // The new primary plane has not shown a window of tall buffers
            // yet, so the next modeset tests the pan on it afresh, whether
            // the old plane took the pan or refused it.
            #[cfg(feature = "scroll-pan")]
            {
                o.pan.proven = None;
                o.pan.refused = false;
            }
        }
        log::info!(
            "display {} moves from CRTC {} to CRTC {crtc}",
            o.name,
            o.crtc
        );
        o.crtc = crtc;
        o.pipe = pipe;
        // The new cursor plane shows nothing of vtium's yet.
        o.cursor_set = None;
        Ok(())
    }

    /// The primary and cursor planes for the CRTC at `index`, with their
    /// properties and the CRTC's.
    pub(super) fn find_planes(&self, crtc: u32, index: usize) -> Option<Planes> {
        let bit = u32::try_from(index)
            .ok()
            .and_then(|i| 1u32.checked_shl(i))?;
        let mut planes = Planes::default();
        let mut primary = None;
        let all = self
            .card
            .planes()
            .inspect_err(|e| log::warn!("cannot list the card's planes: {e}"))
            .ok()?;
        for plane in all {
            match self.card.plane_crtcs(plane) {
                Ok(crtcs) if crtcs & bit != 0 => {}
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("cannot read the CRTCs of plane {plane}: {e}");
                    continue;
                }
            }
            let props = match self.card.properties(plane, Object::Plane) {
                Ok(props) => props,
                Err(e) => {
                    log::warn!("cannot read the properties of plane {plane}: {e}");
                    continue;
                }
            };
            match value(&props, "type") {
                Some(PLANE_PRIMARY) if primary.is_none() => {
                    planes.damage = props.iter().any(|p| p.name == "FB_DAMAGE_CLIPS");
                    planes.primary_props = Props::new(&props);
                    primary = Some(plane);
                }
                Some(PLANE_CURSOR) if planes.cursor.is_none() => {
                    planes.cursor = Some((plane, Props::new(&props)));
                }
                _ => {}
            }
            if primary.is_some() && planes.cursor.is_some() {
                break;
            }
        }
        planes.primary = primary?;
        let crtc_props = self
            .card
            .properties(crtc, Object::Crtc)
            .inspect_err(|e| log::warn!("cannot read the properties of CRTC {crtc}: {e}"))
            .ok()?;
        planes.crtc_props = Props::new(&crtc_props);
        Some(planes)
    }
}
