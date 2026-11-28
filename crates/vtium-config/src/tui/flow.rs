//! The flow as plain data, which [`crate::flow`] holds, at the path it had
//! in the TUI, and the flow's tests, which draw the TUI's fixture.

#[doc(inline)]
pub use crate::flow::*;

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn a_rule_s_target_resolves_by_name_and_kind() {
        let f = fixture::flow();
        assert_eq!(f.target(&f.rules[0]), Target::Head(0));
        assert_eq!(f.target(&f.rules[4]), Target::Special(Special::Roaming));
        // a display rule may not send to roaming, nor to a head that is gone
        let mut r = f.rules[0].clone();
        r.target = "roaming".into();
        assert_eq!(f.target(&r), Target::Missing);
        r.target = "tv".into();
        assert_eq!(f.target(&r), Target::Missing);
    }

    #[test]
    fn the_columns_stack_as_the_figure_draws_them() {
        let f = fixture::flow();
        let [displays, drules, heads, looks, irules, inputs] = f.columns();
        assert_eq!(displays, [Item::Base, Item::Display(0), Item::Display(1)]);
        assert_eq!(
            drules,
            [
                Item::Rule(0),
                Item::Rule(1),
                Item::Rule(2),
                Item::New(New::DisplayRule)
            ]
        );
        assert_eq!(
            heads,
            [
                Item::Head(0),
                Item::Head(1),
                Item::New(New::Head),
                Item::Special(Special::Roaming),
                Item::Special(Special::Ignore),
                Item::Special(Special::Off),
                Item::Policy,
            ]
        );
        assert_eq!(
            looks,
            [
                Item::HeadLook(0),
                Item::HeadLook(1),
                Item::Look(2),
                Item::New(New::Look)
            ]
        );
        assert_eq!(
            irules,
            [
                Item::Rule(3),
                Item::Rule(4),
                Item::Rule(5),
                Item::New(New::InputRule)
            ]
        );
        assert_eq!(inputs, [Item::Input(0), Item::Input(1), Item::Input(2)]);
        for (c, items) in f.columns().iter().enumerate() {
            for item in items {
                assert_eq!(f.column_of(*item), c, "{item:?}");
            }
        }
    }

    #[test]
    fn follow_goes_toward_the_heads() {
        let f = fixture::flow();
        assert_eq!(f.follows(Item::Display(0)), [Item::Rule(0)]);
        assert_eq!(f.follows(Item::Rule(0)), [Item::Head(0)]);
        assert_eq!(f.follows(Item::Input(1)), [Item::Rule(4)]);
        assert_eq!(f.follows(Item::Rule(4)), [Item::Special(Special::Roaming)]);
        assert_eq!(f.follows(Item::Rule(3)), [Item::HeadLook(0)]);
        // a head steps through the display rules that send to it
        assert_eq!(f.follows(Item::Head(0)), [Item::Rule(0), Item::Rule(2)]);
        assert_eq!(f.follows(Item::Base), []);
    }

    #[test]
    fn a_selector_names_one_device_or_a_kind() {
        for s in [
            "edid:X",
            "path:pci-0",
            "serial:1",
            "name:Power Button",
            "bluetooth:aa",
        ] {
            assert!(!general(s), "{s}");
        }
        for s in [
            "*",
            "class:keyboard",
            "id:046d:c31c",
            "card:card1",
            "udev:ID_X=1",
        ] {
            assert!(general(s), "{s}");
        }
    }
}
