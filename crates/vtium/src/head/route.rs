//! Where each input device's events go (ADR-1401 D5): to the head a rule
//! sends it to, or to the head holding the focus. A device walks the rules
//! alone when it comes, by its identity, and keeps its place until it
//! goes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vtium_config::{Class, Identity, Place};
use vtium_platform::input::{DeviceId, Disposition};

/// One input device and where the rules placed it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Route {
    /// Its handle in the seat's input, once it has one.
    id: Option<DeviceId>,
    identity: Identity,
    keyboard: bool,
    /// Where its rule placed it.
    place: Place,
    /// The rescue key moved it to the focus for the rest of the boot
    /// (ADR-1401 D9).
    rescued: bool,
    /// The anti-lockout guard moved it to the focus while no keyboard
    /// reaches a lit head (D9).
    guarded: bool,
}

impl Route {
    /// Where its events go: its rule's place, or the focus once rescued or
    /// guarded.
    fn place(&self) -> Place {
        if self.rescued || self.guarded {
            Place::Focus
        } else {
            self.place
        }
    }
}

/// Every input device the rules placed, by its node.
#[derive(Debug, Default)]
pub(crate) struct Routes {
    devices: BTreeMap<PathBuf, Route>,
}

impl Routes {
    /// Places the device at `node` by the first rule matching `identity`,
    /// and returns whether vtium opens it.
    pub(crate) fn add(
        &mut self,
        rules: &vtium_config::Config,
        node: &Path,
        identity: &Identity,
    ) -> Disposition {
        let placed = rules.walk(identity);
        self.devices.insert(
            node.to_owned(),
            Route {
                id: None,
                identity: identity.clone(),
                keyboard: identity.classes.has(Class::Keyboard),
                place: placed.place,
                rescued: false,
                guarded: false,
            },
        );
        disposition(rules, placed.place, identity)
    }

    /// The identities of the devices the rescue key has not moved, which
    /// the guards place, in the order [`Routes::guard`] takes them.
    pub(crate) fn unrescued(&self) -> impl Iterator<Item = &Identity> {
        self.devices
            .values()
            .filter(|r| !r.rescued)
            .map(|r| &r.identity)
    }

    /// Whether a keyboard-class device is present.
    pub(crate) fn any_keyboard(&self) -> bool {
        self.devices.values().any(|r| r.keyboard)
    }

    /// Whether a keyboard the rescue key moved counts toward the
    /// anti-lockout guard (D9).
    pub(crate) fn rescued_keyboard(&self) -> bool {
        self.devices
            .values()
            .any(|r| r.rescued && r.identity.counts_as_keyboard())
    }

    /// Each device [`Routes::unrescued`] gave goes to the focus where
    /// `guarded` says the anti-lockout guard moved it, and back to its rule
    /// where not. Returns each device whose disposition changed, with the
    /// new one.
    pub(crate) fn guard(
        &mut self,
        rules: &vtium_config::Config,
        guarded: impl IntoIterator<Item = bool>,
    ) -> Vec<(DeviceId, Disposition)> {
        let mut changed = Vec::new();
        let unrescued = self.devices.values_mut().filter(|r| !r.rescued);
        for (route, guarded) in unrescued.zip(guarded) {
            if route.guarded == guarded {
                continue;
            }
            route.guarded = guarded;
            let place = route.place();
            let now = disposition(rules, place, &route.identity);
            if let Some(id) = route.id {
                changed.push((id, now));
            }
        }
        changed
    }

    /// The seat's input took the device at `node` as `id`.
    pub(crate) fn opened(&mut self, node: &Path, id: DeviceId) {
        if let Some(route) = self.devices.get_mut(node) {
            route.id = Some(id);
        }
    }

    /// The rescue key pressed on the device at `node` (ADR-1401 D9): it
    /// follows the focus for the rest of the boot where `rules` let the
    /// rescue take it from its place. Returns its handle once moved.
    pub(crate) fn rescue(
        &mut self,
        rules: &vtium_config::Config,
        node: &Path,
    ) -> Option<Option<DeviceId>> {
        let route = self.devices.get_mut(node).filter(|r| !r.rescued)?;
        let placed = vtium_config::Placed {
            place: route.place,
            by: vtium_config::By::NoMatch,
        };
        route.rescued = rules.rescue(&placed);
        route.rescued.then_some(route.id)
    }

    pub(crate) fn remove(&mut self, node: &Path) {
        self.devices.remove(node);
    }

    /// The node of the device the seat's input calls `id`.
    pub(crate) fn node(&self, id: DeviceId) -> Option<&Path> {
        self.devices
            .iter()
            .find(|(_, r)| r.id == Some(id))
            .map(|(node, _)| node.as_path())
    }

    /// The head the events of the device at `node` go to, with `focus` the
    /// head holding the focus: its rule's head, else the focus's. A device
    /// no rule placed, as one a test types on, follows the focus; an
    /// ignored one reaches none.
    pub(crate) fn head(&self, node: Option<&Path>, focus: Option<usize>) -> Option<usize> {
        let place = node.and_then(|n| self.devices.get(n)).map(Route::place);
        head_of(place.unwrap_or(Place::Focus), focus)
    }

    /// The handles of the devices whose events go to head `h`.
    pub(crate) fn devices_of(&self, h: usize, focus: Option<usize>) -> Vec<DeviceId> {
        self.devices
            .values()
            .filter(|r| head_of(r.place(), focus) == Some(h))
            .filter_map(|r| r.id)
            .collect()
    }

    /// Whether the events of a keyboard `open` holds open go to head `h`.
    pub(crate) fn keyboard_reaches(
        &self,
        h: usize,
        focus: Option<usize>,
        open: impl Fn(Option<DeviceId>) -> bool,
    ) -> bool {
        self.devices
            .values()
            .any(|r| r.keyboard && head_of(r.place(), focus) == Some(h) && open(r.id))
    }
}

/// Whether vtium opens a device placed at `place`.
fn disposition(rules: &vtium_config::Config, place: Place, identity: &Identity) -> Disposition {
    let placed = vtium_config::Placed {
        place,
        by: vtium_config::By::NoMatch,
    };
    match rules.disposition(&placed, identity) {
        vtium_config::Disposition::Deliver => Disposition::Deliver,
        vtium_config::Disposition::RescueOnly => Disposition::Rescue,
        vtium_config::Disposition::Closed => Disposition::Closed,
    }
}

/// The head a device placed at `place` delivers to.
fn head_of(place: Place, focus: Option<usize>) -> Option<usize> {
    match place {
        Place::Head(h) => Some(h),
        Place::DefaultHead => Some(0),
        Place::Focus => focus,
        Place::Ignore | Place::Dark => None,
    }
}

/// The rules in force: `rules`, with the shipped rules' pointer rules
/// sending pointers to `ignore` under kmscon.conf's `mouse` off, its
/// shorthand (ADR-1401 D3), so a pointer is never opened, one with udev's
/// key class on its node too.
pub(crate) fn in_force(
    mut rules: vtium_config::Config,
    config: &crate::config::Config,
) -> vtium_config::Config {
    use vtium_config::{Match, Target, Token};
    if config.switch(&crate::config::keys::MOUSE) || rules != vtium_config::Config::builtin() {
        return rules;
    }
    for rule in &mut rules.rules {
        if matches!(&rule.matcher, Match::All(tokens)
            if matches!(tokens.as_slice(), [Token::Class(c)] if c.is_pointer()))
        {
            rule.target = Target::Ignore;
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard(path: &str) -> Identity {
        Identity {
            path: Some(path.to_owned()),
            ..Identity::input().with_class(Class::Keyboard)
        }
    }

    // ADR-1401 D5: a claimed device delivers to its head whatever the
    // focus; one following the focus moves with it; an ignored one is
    // opened for the rescue key alone and delivers to no head.
    #[test]
    fn a_device_delivers_to_its_rules_head_or_the_focus() {
        let rules = vtium_config::parse(
            "version = 1\n[head a]\n[head b]\n[rules]\ninput path:p-a -> a\ninput path:p-x -> ignore\ninput class:keyboard -> focus\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        let mut routes = Routes::default();
        let open = |routes: &mut Routes, node: &str, path: &str| {
            routes.add(&rules, Path::new(node), &keyboard(path))
        };
        assert_eq!(open(&mut routes, "/a", "p-a"), Disposition::Deliver);
        assert_eq!(open(&mut routes, "/f", "p-f"), Disposition::Deliver);
        assert_eq!(open(&mut routes, "/x", "p-x"), Disposition::Rescue);
        for focus in [Some(0), Some(1), None] {
            assert_eq!(routes.head(Some(Path::new("/a")), focus), Some(0));
            assert_eq!(routes.head(Some(Path::new("/f")), focus), focus);
            assert_eq!(routes.head(Some(Path::new("/x")), focus), None);
            assert_eq!(routes.head(None, focus), focus, "no device named");
        }
        assert!(routes.keyboard_reaches(1, Some(1), |_| true));
        assert_eq!(
            routes.rescue(&rules, Path::new("/f")),
            None,
            "a keyboard following the focus stays"
        );
        assert!(!routes.rescued_keyboard());
        assert!(!routes.keyboard_reaches(1, Some(0), |_| true));
        assert!(!routes.keyboard_reaches(1, Some(1), |_| false), "none open");
    }

    // ADR-1401 D9: the anti-lockout guard opens an ignored keyboard to
    // deliver while it holds it on the focus, and lets it go back to the
    // rescue key's watch; one the rescue key moved stays on the focus.
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_guard_opens_an_ignored_keyboard_while_it_holds_it_on_the_focus() {
        let rules = vtium_config::parse(
            "version = 1\n[head a]\n[rules]\ninput path:p-x -> ignore\ninput path:p-a -> a\n",
        )
        .map(|p| p.config)
        .unwrap_or_default();
        use vtium_platform::input::{Direct, Input, InputConfig, Keymap};
        // the seat's input, asleep, hands out the handles
        let Some(keymap) = Keymap::fallback() else {
            return;
        };
        let mut input = Input::new(Direct, keymap, InputConfig::default());
        let mut id = |node: &str| input.add_with(Path::new(node), Disposition::Closed);
        let (a, x) = (id("/a").unwrap(), id("/x").unwrap());
        let mut routes = Routes::default();
        routes.add(&rules, Path::new("/a"), &keyboard("p-a"));
        routes.add(&rules, Path::new("/x"), &keyboard("p-x"));
        routes.opened(Path::new("/a"), a);
        routes.opened(Path::new("/x"), x);
        let held = routes.guard(&rules, [true, true]);
        assert_eq!(held, [(a, Disposition::Deliver), (x, Disposition::Deliver)]);
        assert_eq!(routes.head(Some(Path::new("/x")), Some(0)), Some(0));
        let back = routes.guard(&rules, [false, false]);
        assert_eq!(back, [(a, Disposition::Deliver), (x, Disposition::Rescue)]);
        assert_eq!(routes.head(Some(Path::new("/x")), Some(0)), None);
        assert_eq!(routes.rescue(&rules, Path::new("/x")), Some(Some(x)));
        assert!(routes.rescued_keyboard());
        assert_eq!(routes.unrescued().count(), 1);
        assert_eq!(routes.guard(&rules, [false]), []);
        assert_eq!(
            routes.head(Some(Path::new("/x")), None),
            None,
            "the focus's, none"
        );
        assert_eq!(routes.head(Some(Path::new("/x")), Some(0)), Some(0));
    }
}
