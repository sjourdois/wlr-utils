//! Keyboard pieces the wlr-utils overlays can share.
//!
//! Each tool runs its own windowing host and owns its own key handling; this module
//! holds what more than one of them would otherwise spell out for itself.

use egui::Key;
use smithay_client_toolkit::seat::keyboard::Keysym;
use std::fmt;
use std::str::FromStr;

const SHIFT: &str = "Shift+";

/// True for the keystrokes that mean "cancel".
///
/// That is `Esc`, or the terminal-style `Ctrl+[` — the chord that has sent the same
/// byte (`0x1b`) since the ASCII days. `ctrl` is the Control state at the time of the
/// press: xkb leaves the keysym as `bracketleft`, so only the modifier tells the two
/// apart.
pub fn is_cancel(keysym: Keysym, ctrl: bool) -> bool {
    keysym == Keysym::Escape || (ctrl && keysym == Keysym::bracketleft)
}

/// A key, held with Shift or without.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyPress {
    /// Named unshifted, the way egui names keys: `Shift+Minus`, never `Underscore`.
    pub key: Key,
    /// Whether Shift is held.
    pub shifted: bool,
}

/// Writes what [`FromStr`] reads: `Tab`, `Shift+Tab`, `Down`, `J`.
impl fmt::Display for KeyPress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.shifted {
            f.write_str(SHIFT)?;
        }
        f.write_str(self.key.name())
    }
}

/// Reads an optional `Shift+` prefix, in any case, then a key name.
///
/// The names are egui's, so the variants (`Tab`, `ArrowDown`, `Digit1`), their aliases
/// (`Down`, `Esc`) and bare characters (`j`, `1`, `-`) all work.
impl FromStr for KeyPress {
    type Err = UnknownKey;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        // `get`, because an arbitrary string may not split at that byte.
        let (shifted, name) = match s.get(..SHIFT.len()) {
            Some(p) if p.eq_ignore_ascii_case(SHIFT) => (true, &s[SHIFT.len()..]),
            _ => (false, s),
        };
        Key::from_name(name)
            .map(|key| Self { key, shifted })
            .ok_or(UnknownKey)
    }
}

/// A [`KeyPress`] that named no key egui knows.
///
/// It leaves out the offending value, which a caller reporting this already has, and
/// says what an accepted one looks like instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[error("unknown key (try Tab, Shift+Tab, Down, j, F5)")]
pub struct UnknownKey;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_press_reads_back_as_what_it_printed() {
        for k in [
            KeyPress {
                key: Key::Tab,
                shifted: false,
            },
            KeyPress {
                key: Key::Tab,
                shifted: true,
            },
            // `ArrowDown` prints as `Down`, so what comes out is not what one would
            // most likely put in.
            KeyPress {
                key: Key::ArrowDown,
                shifted: false,
            },
            KeyPress {
                key: Key::Minus,
                shifted: true,
            },
        ] {
            assert_eq!(k.to_string().parse(), Ok(k));
        }
    }
}
