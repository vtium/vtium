//! The heads of one VT (ADR-1401 D2). Each vtium runs on one VT, and it
//! builds the heads of vtium.conf that run there: every head whose `vt`
//! names that VT, and every head with no `vt`. vtium-settings names a head
//! by its place among all the file's heads, and vtium keeps only the heads
//! it builds, so [`VtHeads`] turns one numbering into the other.
//!
//! vtium places each device for its own VT by the four steps of ADR-1401
//! D3. A device whose rules tie it to a head on another VT stays with that
//! head: on this VT a display stays dark and a keyboard types nowhere.

use vtium_settings::{Config, Identity, Place, Placed, Placement, Policy};

/// The heads the vtium on one VT builds, and the VT it places devices for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VtHeads {
    /// The VT vtium runs on. Without a kernel VT it is `None`, and every
    /// head runs, as before heads had a `vt`.
    vt: Option<u16>,
    /// The place in the file's heads of each head vtium builds, in the
    /// file's order. It is empty when vtium builds the default head alone,
    /// or no head at all.
    file: Vec<usize>,
    /// vtium builds the default head, `[head console]` of the compiled-in
    /// configuration, because no head of the file runs on this VT and the
    /// policy is open (ADR-1401 D9).
    default: bool,
}

impl VtHeads {
    /// The heads of `rules` that the vtium on VT `vt` builds. Under the
    /// open policy, where no head of the file runs on `vt`, vtium builds
    /// the default head in their place, which the display guard lights.
    pub(crate) fn new(rules: &Config, vt: Option<u16>) -> VtHeads {
        let file: Vec<usize> = rules
            .heads
            .iter()
            .enumerate()
            .filter(|(_, h)| vt.is_none_or(|vt| h.runs_on(vt)))
            .map(|(i, _)| i)
            .collect();
        let default = file.is_empty() && rules.policy == Policy::Open;
        VtHeads { vt, file, default }
    }

    /// The VT vtium places devices for, `None` without a kernel VT.
    pub(crate) fn vt(&self) -> Option<u16> {
        self.vt
    }

    /// The configuration of each head vtium builds, from `rules`, in the
    /// file's order: the heads of this VT, or the default head.
    pub(crate) fn heads(&self, rules: &Config) -> Vec<vtium_settings::Head> {
        if self.default {
            return Config::builtin().heads;
        }
        self.file
            .iter()
            .filter_map(|&i| rules.heads.get(i).cloned())
            .collect()
    }

    /// The head vtium built for `place`, by its index among the heads it
    /// builds. A device placed nowhere on this VT has none.
    pub(crate) fn head(&self, place: Place) -> Option<usize> {
        match place {
            Place::Head(i) => self.file.iter().position(|&f| f == i),
            Place::DefaultHead => self.default.then_some(0),
            Place::Roaming | Place::Ignore | Place::Off | Place::Notice | Place::Elsewhere(_) => {
                None
            }
        }
    }

    /// The place vtium-settings gives the head vtium built at `h`.
    pub(crate) fn place(&self, h: usize) -> Option<Place> {
        if self.default {
            return (h == 0).then_some(Place::DefaultHead);
        }
        self.file.get(h).map(|&i| Place::Head(i))
    }

    /// Whether the head vtium built at `h` is isolated (ADR-1401 D5). The
    /// default head never is.
    pub(crate) fn isolated(&self, rules: &Config, h: usize) -> bool {
        match self.place(h) {
            Some(Place::Head(i)) => rules.isolated(i),
            _ => false,
        }
    }

    /// Where the device `id` lands on this VT, by the first rule that
    /// places it (ADR-1401 D3).
    pub(crate) fn walk(&self, rules: &Config, id: &Identity) -> Placed {
        match self.vt {
            Some(vt) => rules.walk_on(id, vt),
            None => rules.walk(id),
        }
    }

    /// Every device of `devices` placed on this VT, and the guards run
    /// over them (ADR-1401 D9), `rescued` being the keyboards the rescue
    /// key moved and `focus` the active head, by vtium's own index.
    pub(crate) fn placement(
        &self,
        rules: &Config,
        devices: &[Identity],
        rescued: &[Identity],
        focus: Option<usize>,
    ) -> Placement {
        let focus = focus.and_then(|h| self.place(h));
        match self.vt {
            Some(vt) => rules.place_rescued_on(devices, rescued, focus, vt),
            None => rules.place_rescued(devices, rescued, focus),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Config {
        vtium_settings::parse(text)
            .map(|p| p.config)
            .unwrap_or_default()
    }

    const THREE: &str = "version = 1
[head a]
vt = 3
[head every]
[head b]
vt = 4 5
[rules]
display * -> a
";

    #[test]
    fn the_vtium_on_a_vt_builds_the_heads_that_name_it_and_those_with_no_vt() {
        let r = rules(THREE);
        let names = |vt| -> Vec<String> {
            VtHeads::new(&r, vt)
                .heads(&r)
                .into_iter()
                .map(|h| h.name)
                .collect()
        };
        assert_eq!(names(Some(3)), ["a", "every"]);
        assert_eq!(names(Some(4)), ["every", "b"]);
        assert_eq!(names(Some(5)), ["every", "b"]);
        assert_eq!(names(Some(6)), ["every"]);
        assert_eq!(names(None), ["a", "every", "b"], "no VT: every head");
    }

    #[test]
    fn a_head_of_the_file_maps_to_the_head_vtium_built_and_back() {
        let r = rules(THREE);
        let on4 = VtHeads::new(&r, Some(4));
        assert_eq!(on4.head(Place::Head(0)), None, "a runs on VT 3 only");
        assert_eq!(on4.head(Place::Head(1)), Some(0));
        assert_eq!(on4.head(Place::Head(2)), Some(1));
        assert_eq!(on4.head(Place::Elsewhere(0)), None);
        assert_eq!(on4.head(Place::DefaultHead), None);
        assert_eq!(on4.place(1), Some(Place::Head(2)));
        assert_eq!(on4.place(2), None);
    }

    #[test]
    fn with_no_head_on_its_vt_an_open_file_builds_the_default_head_and_a_closed_one_none() {
        let open = rules("version = 1\n[head a]\nvt = 3\n");
        let on4 = VtHeads::new(&open, Some(4));
        let names: Vec<String> = on4.heads(&open).into_iter().map(|h| h.name).collect();
        assert_eq!(names, ["console"]);
        assert_eq!(on4.head(Place::DefaultHead), Some(0));
        assert_eq!(on4.place(0), Some(Place::DefaultHead));
        assert!(
            !on4.isolated(&open, 0),
            "the default head is never isolated"
        );
        let closed = rules("version = 1\npolicy = closed\n[head a]\nvt = 3\n");
        let on4 = VtHeads::new(&closed, Some(4));
        assert_eq!(on4.heads(&closed), []);
        assert_eq!(on4.head(Place::DefaultHead), None);
    }
}
