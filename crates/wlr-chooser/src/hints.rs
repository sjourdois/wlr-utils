//! Tile hints: one key per tile, taken from a physical keyboard row and labelled
//! with the character the *active* layout prints on it.
//!
//! A fixed alphabet would be a trap on anything but the layout it was written for: on
//! AZERTY a tile labelled `a` is reached by pressing the key QWERTY calls `q`. So a
//! hint is a physical position (an evdev code) first, and its label is read out of the
//! compositor's keymap — `a` on QWERTY, `q` on AZERTY, `ф` on ЙЦУКЕН, all on the same
//! key under the same finger. Matching is on the position too, so what is displayed
//! and what is pressed cannot drift apart.

use xkbcommon::xkb;

/// Wayland reports a key by its evdev code; xkb numbers the same key eight higher.
pub(crate) const EVDEV_OFFSET: u32 = 8;

/// The home row, left to right, without the two keys under the little fingers'
/// stretch: `asdfghjkl` on QWERTY, `qsdfghjkl` on AZERTY.
const HOME_ROW: [u32; 9] = [30, 31, 32, 33, 34, 35, 36, 37, 38];

/// The row above the letters: `1234567890` on QWERTY, `&é"'(-è_çà` on AZERTY.
const TOP_ROW: [u32; 10] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// Which physical row the hints are taken from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HintRow {
    /// The home row, under the resting fingers. Prints a letter on every common
    /// layout, which is what makes it the default.
    Home,
    /// The row above the letters. One more key than the home row, and a natural
    /// order wherever it prints digits — which is not everywhere.
    Top,
}

impl HintRow {
    /// The physical keys of this row, in reading order.
    fn positions(self) -> &'static [u32] {
        match self {
            Self::Home => &HOME_ROW,
            Self::Top => &TOP_ROW,
        }
    }
}

/// One tile's hint: the key to press, and what the active layout prints on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    /// The physical key, as an evdev code — what a Wayland key event carries.
    pub code: u32,
    /// The character to display, as produced by the active layout with no modifier.
    pub label: String,
}

/// The hints `row` offers under `keymap` (the compositor's keymap, in xkb text
/// format), in tile order.
///
/// A keymap that does not compile, or a row whose keys print nothing usable, yields
/// fewer hints — or none. Callers hand them out to the first tiles and leave the rest
/// without a shortcut, so a hint is never shown that cannot be typed.
pub fn hints(keymap: &str, row: HintRow) -> Vec<Hint> {
    hints_at(keymap, row.positions())
}

/// The hints `positions` offers under `keymap`, skipping the positions that print
/// nothing displayable.
fn hints_at(keymap: &str, positions: &[u32]) -> Vec<Hint> {
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let Some(keymap) = xkb::Keymap::new_from_string(
        &ctx,
        keymap.to_string(),
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    ) else {
        return Vec::new();
    };
    let state = xkb::State::new(&keymap);
    positions
        .iter()
        .filter_map(|&code| label(&state, code).map(|label| Hint { code, label }))
        .collect()
}

/// What `code` prints with no modifier held, when that is a single character a tile
/// can carry: one glyph, standing on its own.
///
/// A position bound to a modifier or a function prints nothing, and a dead key prints
/// nothing until it is composed with the next one — neither can label a tile, and the
/// position is passed over rather than shown blank.
fn label(state: &xkb::State, code: u32) -> Option<String> {
    let text = state.key_get_utf8(xkb::Keycode::new(code + EVDEV_OFFSET));
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() || c.is_control() || c.is_whitespace() {
        return None;
    }
    Some(c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compositor keymap, as the one arriving over `wl_keyboard` would read.
    fn keymap(layout: &str, variant: &str) -> String {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        xkb::Keymap::new_from_names(
            &ctx,
            "",
            "",
            layout,
            variant,
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("the layouts under test come with xkeyboard-config")
        .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
    }

    /// The labels of a row, as a tile would show them.
    fn labels(layout: &str, variant: &str, row: HintRow) -> String {
        hints(&keymap(layout, variant), row)
            .iter()
            .map(|h| h.label.as_str())
            .collect()
    }

    #[test]
    fn a_hint_shows_what_the_active_layout_prints_on_that_key() {
        // The same nine physical keys, under three layouts. QWERTY's `a` and AZERTY's
        // `q` are one key: a tile labelled `a` on a French keyboard would be a trap.
        assert_eq!(labels("us", "", HintRow::Home), "asdfghjkl");
        assert_eq!(labels("fr", "", HintRow::Home), "qsdfghjkl");
        assert_eq!(labels("ru", "", HintRow::Home), "фывапролд");
        // And the same ten keys of the row above, where French prints no digit at all.
        assert_eq!(labels("us", "", HintRow::Top), "1234567890");
        assert_eq!(labels("fr", "", HintRow::Top), "&é\"'(-è_çà");
    }

    #[test]
    fn the_home_row_prints_a_letter_on_every_layout_the_top_row_does_not() {
        // Why the home row is the default: it carries letters where the row above
        // carries punctuation, and punctuation is what a tile has to display.
        for (layout, variant) in [("us", "dvorak"), ("us", "colemak"), ("de", "")] {
            assert!(
                labels(layout, variant, HintRow::Home)
                    .chars()
                    .all(char::is_alphabetic),
                "{layout} {variant}"
            );
        }
        // Bépo is the exception both ways: a comma sits in its home row, and its top
        // row is punctuation throughout.
        assert_eq!(labels("fr", "bepo", HintRow::Home), "auie,ctsr");
        assert_eq!(labels("fr", "bepo", HintRow::Top), "\"«»()@+-/*");
    }

    #[test]
    fn a_key_that_prints_nothing_is_passed_over() {
        // Left Shift (evdev 42) sits between two letters here. It labels no tile, so
        // the tile that would have had it takes the next usable key instead.
        let km = keymap("us", "");
        assert_eq!(
            hints_at(&km, &[30, 42, 31]),
            vec![
                Hint {
                    code: 30,
                    label: "a".into()
                },
                Hint {
                    code: 31,
                    label: "s".into()
                },
            ]
        );
    }

    #[test]
    fn a_keymap_that_does_not_compile_offers_no_hint() {
        // Nothing to display and nothing to press, rather than a guess at an alphabet.
        assert!(hints("this is not a keymap", HintRow::Home).is_empty());
        assert!(hints("", HintRow::Top).is_empty());
    }

    #[test]
    fn hints_come_back_in_reading_order_on_their_own_keys() {
        // The order is the row's, left to right, and each hint carries the evdev code
        // a key event will be matched against.
        let km = keymap("fr", "");
        let home = hints(&km, HintRow::Home);
        assert_eq!(
            home.iter().map(|h| h.code).collect::<Vec<_>>(),
            HOME_ROW.to_vec()
        );
        assert_eq!(hints(&km, HintRow::Top).len(), TOP_ROW.len());
    }
}
