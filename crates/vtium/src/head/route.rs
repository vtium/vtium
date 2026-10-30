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
    keyboard: bool,
    place: Place,
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
                keyboard: identity.classes.has(Class::Keyboard),
                place: placed.place,
            },
        );
        match rules.disposition(&placed, identity) {
            vtium_config::Disposition::Deliver => Disposition::Deliver,
            vtium_config::Disposition::RescueOnly => Disposition::Rescue,
            vtium_config::Disposition::Closed => Disposition::Closed,
        }
    }

    /// The seat's input took the device at `node` as `id`.
    pub(crate) fn opened(&mut self, node: &Path, id: DeviceId) {
        if let Some(route) = self.devices.get_mut(node) {
            route.id = Some(id);
        }
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
        let place = node.and_then(|n| self.devices.get(n)).map(|r| r.place);
        head_of(place.unwrap_or(Place::Focus), focus)
    }

    /// The handles of the devices whose events go to head `h`.
    pub(crate) fn devices_of(&self, h: usize, focus: Option<usize>) -> Vec<DeviceId> {
        self.devices
            .values()
            .filter(|r| head_of(r.place, focus) == Some(h))
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
            .any(|r| r.keyboard && head_of(r.place, focus) == Some(h) && open(r.id))
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
        assert!(!routes.keyboard_reaches(1, Some(0), |_| true));
        assert!(!routes.keyboard_reaches(1, Some(1), |_| false), "none open");
    }
}
