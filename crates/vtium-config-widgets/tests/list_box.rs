//! The list box: where it opens, how it scrolls, the search it offers
//! where its line has many choices, its keys and clicks, the places it
//! returns against the cells it draws, and its screens at 100×30 and
//! 80×25 with the screen map's choices and with five hundred.
//!
//! References:
//! - ADR-1206 D8: one widget, under the line or above it, scrolling, its
//!   places, its state in the tab's state, the choices from the tab, a
//!   kept pick as one change, and the search where the choices are many.
//! - ADR-1403 D11, "The list box": its keys and clicks.

mod screens;

use std::borrow::Cow;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::list_box::{ListBox, Outcome};
use vtium_config_widgets::tab::Choices;

use screens::rows_of;

/// The change keeping a choice makes, as a tab names it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Set(String);

/// A line's choices.
struct Values {
    items: Vec<String>,
    current: Option<usize>,
}

impl Choices<Set> for Values {
    fn count(&self) -> usize {
        self.items.len()
    }
    fn text(&self, choice: usize) -> Cow<'_, str> {
        Cow::Borrowed(self.items.get(choice).map_or("", String::as_str))
    }
    fn current(&self) -> Option<usize> {
        self.current
    }
    fn keep(&self, choice: usize) -> Set {
        Set(self.items[choice].clone())
    }
}

/// The font sets a look's card offers, as the screen map draws them.
fn font_sets() -> Values {
    Values {
        items: [
            "the font set vtium.conf sets (unifont)",
            "cullet-mono",
            "jetbrains",
            "terminus",
            "unifont",
        ]
        .map(String::from)
        .to_vec(),
        current: Some(1),
    }
}

/// Five hundred choices, as a role of a new font set offers on a machine
/// with many fonts.
fn many() -> Values {
    Values {
        items: (0..500).map(|i| format!("choice {i:03}")).collect(),
        current: Some(250),
    }
}

fn press<C>(b: &mut ListBox, choices: &dyn Choices<C>, rect: Rect, code: Code) -> Outcome<C> {
    b.key(&Key::plain(code), choices, rect)
}

fn typed(b: &mut ListBox, choices: &dyn Choices<Set>, rect: Rect, text: &str) {
    for c in text.chars() {
        b.key(&Key::char(c), choices, rect);
    }
}

const SCREEN: Rect = Rect::new(0, 2, 100, 25);

#[test]
fn it_opens_under_its_line_with_the_highlight_on_the_line_s_choice() {
    let sets = font_sets();
    let b = ListBox::open("looks.look.list", &sets);
    assert_eq!(b.highlight(), 1);
    let rect = b.place(&sets, SCREEN, 5, 17);
    // under the line, wide enough for the longest choice and its mark
    assert_eq!(rect, Rect::new(17, 6, 44, 7));
    assert!(!b.offers_search(&sets, rect));
    // a line with no choice of the list: the first is highlighted
    let other = Values {
        current: None,
        ..font_sets()
    };
    assert_eq!(ListBox::open("looks.look.list", &other).highlight(), 0);
}

#[test]
fn at_the_screen_s_last_rows_it_opens_above_its_line() {
    let sets = font_sets();
    let b = ListBox::open("looks.look.list", &sets);
    // the line is the area's second row from the bottom: one row below it
    let rect = b.place(&sets, SCREEN, 25, 17);
    assert_eq!(rect, Rect::new(17, 18, 44, 7));
    // at the right edge, it moves left to stay inside the area
    let rect = b.place(&sets, SCREEN, 5, 90);
    assert_eq!(rect.right(), SCREEN.right());
    // in a narrow area, it takes the area's width
    let rect = b.place(&sets, Rect::new(0, 0, 30, 25), 5, 0);
    assert_eq!(rect.width, 30);
    // with room on neither side for every choice, the larger side scrolls
    let choices = many();
    let b = ListBox::open("looks.new-frit.list", &choices);
    let below = b.place(&choices, SCREEN, 8, 0);
    assert_eq!((below.y, below.bottom()), (9, SCREEN.bottom()));
    let above = b.place(&choices, SCREEN, 20, 0);
    assert_eq!((above.y, above.bottom()), (SCREEN.y, 20));
}

#[test]
fn five_hundred_choices_scroll_and_offer_the_search() {
    let choices = many();
    let mut b = ListBox::open("looks.new-frit.list", &choices);
    let rect = b.place(&choices, SCREEN, 2, 10);
    let rows = usize::from(rect.height - 2);
    assert!(b.offers_search(&choices, rect));
    assert_eq!(b.highlight(), 250);
    // the first draw shows the line's choice
    let places = b.places(&choices, rect);
    assert!(places.rows.iter().any(|(_, c)| *c == 250));
    assert!(places.search.is_some());
    assert_eq!(press(&mut b, &choices, rect, Code::Home), Outcome::Moved);
    assert_eq!((b.highlight(), b.offset()), (0, 0));
    assert_eq!(
        press(&mut b, &choices, rect, Code::PageDown),
        Outcome::Moved
    );
    assert_eq!(b.highlight(), rows);
    assert_eq!(b.offset(), 1);
    assert_eq!(press(&mut b, &choices, rect, Code::End), Outcome::Moved);
    assert_eq!((b.highlight(), b.offset()), (499, 500 - rows));
    assert_eq!(press(&mut b, &choices, rect, Code::Down), Outcome::Moved);
    assert_eq!(b.highlight(), 499);
    assert_eq!(press(&mut b, &choices, rect, Code::PageUp), Outcome::Moved);
    assert_eq!(b.highlight(), 499 - rows);
    // `?` opens the search on the choices' text
    assert_eq!(b.key(&Key::char('?'), &choices, rect), Outcome::Searched);
    assert!(b.filter().is_some_and(|s| s.typing()));
    typed(&mut b, &choices, rect, "$49");
    let shown: Vec<usize> = b.places(&choices, rect).rows.iter().map(|r| r.1).collect();
    assert_eq!(shown, [49, 149, 249, 349, 449]);
    assert_eq!(b.highlight(), 49);
    assert_eq!(press(&mut b, &choices, rect, Code::Down), Outcome::Moved);
    assert_eq!(b.highlight(), 149);
    // Enter keeps the filter, and Enter again keeps the pick
    assert_eq!(
        press(&mut b, &choices, rect, Code::Enter),
        Outcome::Searched
    );
    assert!(b.filter().is_some_and(|s| !s.typing()));
    assert_eq!(press(&mut b, &choices, rect, Code::Down), Outcome::Moved);
    assert_eq!(b.highlight(), 249);
    assert_eq!(
        press(&mut b, &choices, rect, Code::Enter),
        Outcome::Kept(Set("choice 249".into()))
    );
}

#[test]
fn a_line_with_few_choices_offers_no_search() {
    let sets = font_sets();
    let mut b = ListBox::open("looks.look.list", &sets);
    let rect = b.place(&sets, SCREEN, 5, 17);
    assert_eq!(b.key(&Key::char('?'), &sets, rect), Outcome::Ignored);
    assert!(b.filter().is_none());
    assert!(b.places(&sets, rect).search.is_none());
    // a list box on a node that names no search field offers none either
    let choices = many();
    let b = ListBox::open("fonts.filter", &choices);
    let rect = b.place(&choices, SCREEN, 5, 17);
    assert!(!b.offers_search(&choices, rect));
}

#[test]
fn enter_keeps_the_pick_as_the_tab_s_change_and_esc_changes_nothing() {
    let sets = font_sets();
    let mut b = ListBox::open("looks.look.list", &sets);
    let rect = b.place(&sets, SCREEN, 5, 17);
    assert_eq!(press(&mut b, &sets, rect, Code::Down), Outcome::Moved);
    assert_eq!(press(&mut b, &sets, rect, Code::Down), Outcome::Moved);
    assert_eq!(
        press(&mut b, &sets, rect, Code::Enter),
        Outcome::Kept(Set("terminus".into()))
    );
    let mut b = ListBox::open("looks.look.list", &sets);
    press(&mut b, &sets, rect, Code::Down);
    assert_eq!(press(&mut b, &sets, rect, Code::Escape), Outcome::Closed);
    // with a filter kept, the first Esc clears it, the second closes
    let choices = many();
    let mut b = ListBox::open("looks.new-frit.list", &choices);
    let rect = b.place(&choices, SCREEN, 2, 10);
    b.key(&Key::char('?'), &choices, rect);
    typed(&mut b, &choices, rect, "^choice 1");
    press(&mut b, &choices, rect, Code::Enter);
    assert_eq!(
        press(&mut b, &choices, rect, Code::Escape),
        Outcome::Searched
    );
    assert!(b.filter().is_none());
    assert_eq!(press(&mut b, &choices, rect, Code::Escape), Outcome::Closed);
    // nothing to pick: Enter does nothing
    let none = Values {
        items: Vec::new(),
        current: None,
    };
    let mut b = ListBox::open("looks.look.list", &none);
    let rect = b.place(&none, SCREEN, 5, 17);
    assert_eq!(press(&mut b, &none, rect, Code::Enter), Outcome::Ignored);
}

#[test]
fn the_places_are_where_the_choices_are_drawn_and_a_click_lands_on_what_it_hits() {
    for (node, choices, line) in [
        ("looks.look.list", font_sets(), 5),
        ("looks.new-frit.list", many(), 24),
    ] {
        let mut b = ListBox::open(node, &choices);
        let rect = b.place(&choices, SCREEN, line, 17);
        let mut buf = Buffer::empty(Rect::new(0, 0, 100, 30));
        let places = b.draw(&choices, &mut buf, rect, true);
        assert_eq!(places.rect, rect);
        assert_eq!(places, b.places(&choices, rect));
        let text = rows_of(&buf);
        for (place, choice) in &places.rows {
            assert_eq!(place.height, 1);
            // inside the border, on the rows and columns between its lines
            assert!(
                place.y > rect.y && place.y < rect.bottom() - 1,
                "{place:?} in {rect:?}"
            );
            assert_eq!((place.x, place.right()), (rect.x + 1, rect.right() - 1));
            let row: String = text[usize::from(place.y)]
                .chars()
                .skip(usize::from(place.x))
                .take(usize::from(place.width))
                .collect();
            assert!(row.contains(&*choices.text(*choice)), "{choice}: {row:?}");
        }
        if let Some(search) = places.search {
            let row: String = text[usize::from(search.y)]
                .chars()
                .skip(usize::from(search.x))
                .take(usize::from(search.width))
                .collect();
            assert_eq!(row, "[?] Search");
        }
        // a click on another choice picks it, and a click on the picked
        // choice keeps it
        let (place, choice) = places.rows[0];
        let at = Position::new(place.x + 2, place.y);
        assert_eq!(b.click(at, &choices, rect), Outcome::Moved);
        assert_eq!(b.highlight(), choice);
        assert_eq!(
            b.click(at, &choices, rect),
            Outcome::Kept(Set(choices.text(choice).into_owned()))
        );
        // on the border nothing happens, and outside the box the tab
        // decides
        assert_eq!(
            b.click(rect.as_position(), &choices, rect),
            Outcome::Ignored
        );
        assert_eq!(
            b.click(Position::new(0, 0), &choices, rect),
            Outcome::Outside
        );
    }
    // a click on the foot's [?] Search opens the search
    let choices = many();
    let mut b = ListBox::open("looks.new-frit.list", &choices);
    let rect = b.place(&choices, SCREEN, 2, 10);
    let search = b.places(&choices, rect).search.expect("the search");
    assert_eq!(
        b.click(search.as_position(), &choices, rect),
        Outcome::Searched
    );
    assert!(b.filter().is_some());
}

#[test]
fn drawing_in_a_smaller_place_keeps_the_highlight_in_sight() {
    let choices = many();
    let mut b = ListBox::open("looks.new-frit.list", &choices);
    let rect = b.place(&choices, SCREEN, 2, 10);
    press(&mut b, &choices, rect, Code::End);
    let small = Rect::new(rect.x, rect.y, rect.width, 6);
    let places = b.places(&choices, small);
    assert_eq!(places.rows.last().map(|r| r.1), Some(499));
    assert_eq!(places.rows.len(), 4);
}

/// The list box over a stand-in card: the screen map's font sets under
/// the card's Font set line, and five hundred choices under a line at the
/// screen's last rows, so the box opens above it.
#[test]
fn the_list_box_screens_match_their_snapshots() {
    for (cols, rows) in [(100u16, 30u16), (80, 25)] {
        for (name, node, choices, line) in [
            ("list-box-font-sets", "looks.look.list", font_sets(), 5),
            ("list-box-many", "looks.new-frit.list", many(), rows - 5),
        ] {
            let b = ListBox::open(node, &choices);
            let screen = card_screen(&b, &choices, cols, rows, line, true);
            screens::check_rows(&screen, cols, rows);
            let mono = card_screen(&b, &choices, cols, rows, line, false);
            assert_eq!(rows_of(&mono), rows_of(&screen));
            screens::check_highlight_without_color(&mono);
            screens::snapshot(&format!("{name}-{cols}x{rows}"), &screen);
        }
    }
}

fn card_screen(
    b: &ListBox,
    choices: &Values,
    cols: u16,
    rows: u16,
    line: u16,
    color: bool,
) -> Buffer {
    use vtium_config_widgets::roles::Role;
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows));
    let plain = Role::Row.style(color).cell();
    let rule = "─".repeat(usize::from(cols));
    screens::put(&mut buf, 0, 0, " Looks", plain);
    screens::put(&mut buf, 0, 1, &rule, plain);
    for y in 2..rows - 2 {
        let text = if y == line {
            format!(
                " Font set       {} ▾",
                choices.text(choices.current.unwrap_or(0))
            )
        } else {
            format!(" card line {y}")
        };
        screens::put(&mut buf, 0, y, &text, plain);
    }
    let area = Rect::new(0, 2, cols, rows - 4);
    let rect = b.place(choices, area, line, 16);
    b.draw(choices, &mut buf, rect, color);
    screens::put(&mut buf, 0, rows - 2, &rule, plain);
    screens::put(&mut buf, 0, rows - 1, " [Enter] Keep  [Esc] Close", plain);
    buf
}
