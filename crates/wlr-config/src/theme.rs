//! The theme: the colours and fonts every overlay draws with.
//!
//! A theme file holds the keys of the `[theme]` section at its top level, without `name`:
//!
//! ```toml
//! accent = "#89b4fa"
//! screen-accent = "#89b4fa"
//! window-accent = "#cba6f7"
//! font = "JetBrains Mono"
//! font-size = 15.0
//! ```
//!
//! Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`.

use crate::paths::Dirs;
use ecolor::Color32;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Colours and fonts for the overlays, with generic-dark defaults (see [`Default`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Dimmed overlay drawn behind the card (lock mode).
    pub backdrop: Color32,
    /// Window background (no-lock mode).
    pub bg: Color32,
    /// The centred card.
    pub card: Color32,
    /// Tile background.
    pub tile: Color32,
    /// Tile background when hovered.
    pub tile_hover: Color32,
    /// Tile background when selected.
    pub tile_selected: Color32,
    /// Thumbnail letterbox area.
    pub thumb: Color32,
    /// Labels.
    pub text: Color32,
    /// Placeholders and secondary text.
    pub text_dim: Color32,
    /// General accent (selection, focus).
    pub accent: Color32,
    /// Outline and glyph for OUTPUT tiles.
    pub screen_accent: Color32,
    /// Outline for WINDOW tiles.
    pub window_accent: Color32,

    /// UI font family (resolved via fontconfig).
    pub font: Option<String>,
    /// …or a direct font file.
    pub font_path: Option<String>,
    /// CJK fallback family (else auto-detected).
    pub cjk_font: Option<String>,
    /// Base UI text size in points.
    pub font_size: Option<f32>,
    /// How round the corners are, in logical pixels: the card's radius, every other
    /// corner keeping its proportion to it. 0 squares them all.
    pub corner_radius: f32,
}

/// The card's corner radius by default, which every other corner is drawn in proportion to.
const DEFAULT_CORNER_RADIUS: f32 = 12.0;

impl Default for Theme {
    fn default() -> Self {
        let c = |r, g, b| Color32::from_rgb(r, g, b);
        Self {
            backdrop: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            bg: c(0x1e, 0x21, 0x27),
            card: c(0x21, 0x25, 0x2d),
            tile: c(0x18, 0x1b, 0x22),
            tile_hover: c(0x26, 0x2b, 0x33),
            tile_selected: c(0x3b, 0x42, 0x52),
            thumb: c(0x12, 0x14, 0x1a),
            text: c(0xd8, 0xde, 0xe9),
            text_dim: c(0x7a, 0x82, 0x90),
            accent: c(0x88, 0xc0, 0xd0),
            screen_accent: c(0x81, 0xa1, 0xc1), // blue — screens
            window_accent: c(0xb4, 0x8e, 0xad), // purple — windows
            font: None,
            font_path: None,
            cjk_font: None,
            font_size: None,
            corner_radius: DEFAULT_CORNER_RADIUS,
        }
    }
}

/// What was wrong with one key of a theme, named relative to the table it was read from.
#[derive(Debug, PartialEq)]
pub(crate) enum Problem {
    /// A key no theme has.
    Unknown(String),
    /// A colour key whose value is no colour.
    BadColour { key: String, value: String },
    /// A size that is negative, or not a number.
    Negative(String),
    /// The table does not have the theme's shape (a value of the wrong type).
    BadValue(String),
}

/// The keys a theme sets, all optional: the ones it leaves out keep what is below.
#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
struct Keys {
    backdrop: Option<String>,
    bg: Option<String>,
    card: Option<String>,
    tile: Option<String>,
    tile_hover: Option<String>,
    tile_selected: Option<String>,
    thumb: Option<String>,
    text: Option<String>,
    text_dim: Option<String>,
    accent: Option<String>,
    screen_accent: Option<String>,
    window_accent: Option<String>,
    font: Option<String>,
    font_path: Option<String>,
    cjk_font: Option<String>,
    font_size: Option<f32>,
    corner_radius: Option<f32>,
}

impl Theme {
    /// Set what `table` sets, keeping the rest. A colour that does not parse keeps its
    /// value; a table of the wrong shape changes nothing.
    pub(crate) fn merge(&mut self, table: toml::Table) -> Vec<Problem> {
        let mut problems = Vec::new();
        let keys: Keys = match serde_ignored::deserialize(toml::Value::Table(table), |path| {
            problems.push(Problem::Unknown(path.to_string()))
        }) {
            Ok(keys) => keys,
            Err(e) => return vec![Problem::BadValue(e.to_string())],
        };
        let mut colour = |dst: &mut Color32, key: &str, value: Option<String>| {
            let Some(value) = value else { return };
            match parse_hex(&value) {
                Some(c) => *dst = c,
                None => problems.push(Problem::BadColour {
                    key: key.into(),
                    value,
                }),
            }
        };
        colour(&mut self.backdrop, "backdrop", keys.backdrop);
        colour(&mut self.bg, "bg", keys.bg);
        colour(&mut self.card, "card", keys.card);
        colour(&mut self.tile, "tile", keys.tile);
        colour(&mut self.tile_hover, "tile-hover", keys.tile_hover);
        colour(&mut self.tile_selected, "tile-selected", keys.tile_selected);
        colour(&mut self.thumb, "thumb", keys.thumb);
        colour(&mut self.text, "text", keys.text);
        colour(&mut self.text_dim, "text-dim", keys.text_dim);
        colour(&mut self.accent, "accent", keys.accent);
        colour(&mut self.screen_accent, "screen-accent", keys.screen_accent);
        colour(&mut self.window_accent, "window-accent", keys.window_accent);
        if keys.font.is_some() {
            self.font = keys.font;
        }
        if keys.font_path.is_some() {
            self.font_path = keys.font_path;
        }
        if keys.cjk_font.is_some() {
            self.cjk_font = keys.cjk_font;
        }
        if keys.font_size.is_some() {
            self.font_size = keys.font_size;
        }
        if let Some(radius) = keys.corner_radius {
            if radius.is_finite() && radius >= 0.0 {
                self.corner_radius = radius;
            } else {
                problems.push(Problem::Negative("corner-radius".into()));
            }
        }
        problems
    }

    /// The radius of a corner drawn `at_default` with the default rounding: every corner
    /// keeps its proportion to the card's.
    pub fn radius(&self, at_default: f32) -> f32 {
        at_default * self.corner_radius / DEFAULT_CORNER_RADIUS
    }
}

/// The keys of a theme that `table` leaves out, to check that the example sets them all.
#[cfg(test)]
pub(crate) fn unset_keys(table: toml::Table) -> Vec<&'static str> {
    let k: Keys = toml::Value::Table(table).try_into().unwrap();
    [
        ("backdrop", k.backdrop.is_none()),
        ("bg", k.bg.is_none()),
        ("card", k.card.is_none()),
        ("tile", k.tile.is_none()),
        ("tile-hover", k.tile_hover.is_none()),
        ("tile-selected", k.tile_selected.is_none()),
        ("thumb", k.thumb.is_none()),
        ("text", k.text.is_none()),
        ("text-dim", k.text_dim.is_none()),
        ("accent", k.accent.is_none()),
        ("screen-accent", k.screen_accent.is_none()),
        ("window-accent", k.window_accent.is_none()),
        ("font", k.font.is_none()),
        ("font-path", k.font_path.is_none()),
        ("cjk-font", k.cjk_font.is_none()),
        ("font-size", k.font_size.is_none()),
        ("corner-radius", k.corner_radius.is_none()),
    ]
    .into_iter()
    .filter(|&(_, unset)| unset)
    .map(|(key, _)| key)
    .collect()
}

/// The file `name` designates: a path when it holds a `/` (relative to `base`, the
/// directory of the file that names it), else `<name>.toml` in the first theme directory
/// that has it. `Err` carries the directories searched.
pub(crate) fn find(dirs: &Dirs, base: &Path, name: &str) -> Result<PathBuf, Vec<PathBuf>> {
    if name.contains('/') {
        return Ok(base.join(name));
    }
    let searched = dirs.theme_dirs();
    searched
        .iter()
        .map(|dir| dir.join(format!("{name}.toml")))
        .find(|file| file.is_file())
        .ok_or(searched)
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa`; anything else is `None`.
fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().strip_prefix('#')?;
    // Hex digits only: the length and slices below count bytes, so a slice could end
    // inside a multi-byte character (a panic), and `from_str_radix` would take a `+`.
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(n(0)?, n(2)?, n(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(n(0)?, n(2)?, n(4)?, n(6)?)),
        3 => {
            let d = |i: usize| {
                let v = u8::from_str_radix(&h[i..i + 1], 16).ok()?;
                Some(v * 17)
            };
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(s: &str) -> toml::Table {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn reads_the_three_hex_forms() {
        assert_eq!(parse_hex("#fff"), Some(Color32::WHITE));
        // Each digit of the short form is doubled.
        assert_eq!(parse_hex("#1a3"), Some(Color32::from_rgb(0x11, 0xaa, 0x33)));
        assert_eq!(parse_hex("#ffffff"), Some(Color32::WHITE));
        // Either case.
        assert_eq!(
            parse_hex("#89B4fa"),
            Some(Color32::from_rgb(0x89, 0xb4, 0xfa))
        );
        let c = parse_hex("#ffffff80").unwrap();
        assert_eq!(c, Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 0x80));
        assert_eq!(c.a(), 0x80);
    }

    #[test]
    fn ignores_surrounding_whitespace() {
        assert_eq!(
            parse_hex("  #89b4fa\n"),
            Some(Color32::from_rgb(0x89, 0xb4, 0xfa))
        );
    }

    #[test]
    fn rejects_what_is_not_a_hex_colour() {
        for s in [
            "ffffff", // no `#`
            "#",      // wrong length
            "#ff",
            "#ffff",
            "#fffffff",
            "#fffffffff",
            "#ggg", // not hex digits
            "#12345z",
            "# fff",
            "#+f+f+f", // `from_str_radix` alone would take the sign
        ] {
            assert_eq!(parse_hex(s), None, "{s:?}");
        }
    }

    #[test]
    fn rejects_non_ascii_without_panicking() {
        // Each is 3, 6 or 8 bytes long, the lengths that get sliced, with a slice
        // boundary inside its multi-byte character: all of these used to panic.
        for s in ["#éa", "#€", "#aébcd", "#aébcdef"] {
            assert_eq!(parse_hex(s), None, "{s:?}");
        }
    }

    #[test]
    fn a_merge_keeps_what_the_table_leaves_out() {
        let mut theme = Theme::default();
        let problems = theme.merge(table("accent = \"#ff0000\"\nfont = \"Inter\""));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(theme.accent, Color32::from_rgb(0xff, 0, 0));
        assert_eq!(theme.font.as_deref(), Some("Inter"));
        assert_eq!(theme.bg, Theme::default().bg);

        // A second layer only overrides what it names.
        theme.merge(table("font-size = 15.0"));
        assert_eq!(theme.font.as_deref(), Some("Inter"));
        assert_eq!(theme.font_size, Some(15.0));
    }

    #[test]
    fn a_bad_colour_and_an_unknown_key_are_named() {
        let mut theme = Theme::default();
        let problems = theme.merge(table("accent = \"blue\"\naccnet = \"#fff\""));
        assert_eq!(theme.accent, Theme::default().accent);
        assert_eq!(
            problems,
            [
                Problem::Unknown("accnet".into()),
                Problem::BadColour {
                    key: "accent".into(),
                    value: "blue".into()
                },
            ]
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_changes_nothing() {
        let mut theme = Theme::default();
        let problems = theme.merge(table("accent = \"#ff0000\"\nfont-size = \"big\""));
        assert_eq!(theme, Theme::default());
        assert!(
            matches!(problems[..], [Problem::BadValue(_)]),
            "{problems:?}"
        );
    }

    /// Every corner keeps its proportion to the card's radius, so 0 squares them all; a
    /// whole number reads as well as a fraction, and a negative one is refused.
    #[test]
    fn the_corner_radius_scales_every_corner() {
        let mut theme = Theme::default();
        assert_eq!(theme.radius(8.0), 8.0);
        assert!(theme.merge(table("corner-radius = 0")).is_empty());
        assert_eq!(theme.radius(8.0), 0.0);
        assert!(theme.merge(table("corner-radius = 18.0")).is_empty());
        assert_eq!(theme.radius(8.0), 12.0);
        let problems = theme.merge(table("corner-radius = -4"));
        assert_eq!(problems, [Problem::Negative("corner-radius".into())]);
        assert_eq!(theme.corner_radius, 18.0);
    }

    #[test]
    fn a_name_with_a_slash_is_a_path() {
        let dirs = Dirs::default();
        assert_eq!(
            find(&dirs, Path::new("/cfg"), "themes/mine.toml"),
            Ok(PathBuf::from("/cfg/themes/mine.toml"))
        );
        assert_eq!(
            find(&dirs, Path::new("/cfg"), "/abs/mine.toml"),
            Ok(PathBuf::from("/abs/mine.toml"))
        );
    }

    #[test]
    fn a_user_theme_wins_over_an_installed_one() {
        let tmp = tempfile::tempdir().unwrap();
        let config_home = tmp.path().join("config");
        let data = tmp.path().join("data");
        for dir in [
            config_home.join("wlr-utils/themes"),
            data.join("wlr-utils/themes"),
        ] {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("nord.toml"), "").unwrap();
        }
        std::fs::write(data.join("wlr-utils/themes/dracula.toml"), "").unwrap();
        let dirs = Dirs {
            config_home: Some(config_home.clone()),
            data_dirs: vec![data.clone()],
            ..Dirs::default()
        };
        let base = Path::new("/unused");
        assert_eq!(
            find(&dirs, base, "nord"),
            Ok(config_home.join("wlr-utils/themes/nord.toml"))
        );
        assert_eq!(
            find(&dirs, base, "dracula"),
            Ok(data.join("wlr-utils/themes/dracula.toml"))
        );
        assert_eq!(
            find(&dirs, base, "solarized"),
            Err(vec![
                config_home.join("wlr-utils/themes"),
                data.join("wlr-utils/themes")
            ])
        );
    }
}
