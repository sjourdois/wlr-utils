//! The sizes of the overlays: the `[chooser]` section of `config.toml`, in logical pixels.
//!
//! ```toml
//! [chooser]
//! card-width = 1000      # the card, when --grid does not size it
//! card-height = 760
//! tile-width = 280       # the narrowest a card tile gets
//! spacing = 10           # between tiles
//! switcher-size = 96     # the largest preview or icon in the Alt-Tab row
//! ```

use crate::tr;
use serde::Deserialize;
use wlr_config::Config;

/// The sizes the overlays are drawn at.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The card's size, when `--grid` does not size it to its tiles.
    pub card_width: f32,
    pub card_height: f32,
    /// The narrowest a card tile gets: a row holds as many as fit, widened to fill it.
    pub tile_width: f32,
    /// The space between the card's tiles; the exposé and the Alt-Tab row scale theirs
    /// from it.
    pub spacing: f32,
    /// The largest preview, or icon, in the Alt-Tab row, which shrinks them to fit the
    /// screen.
    pub switcher_size: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            card_width: 1000.0,
            card_height: 760.0,
            tile_width: 280.0,
            spacing: 10.0,
            switcher_size: 96.0,
        }
    }
}

/// The section as written, every key optional.
#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
struct Section {
    card_width: Option<u16>,
    card_height: Option<u16>,
    tile_width: Option<u16>,
    spacing: Option<u16>,
    switcher_size: Option<u16>,
}

impl Layout {
    /// The `[chooser]` section of `config`, over the defaults. A size below what the
    /// overlay can be drawn at is raised to it, and becomes one of `config`'s warnings.
    pub fn from_config(config: &mut Config) -> Self {
        let section: Section = config.section("chooser");
        let mut layout = Layout::default();
        for (value, size, key, min) in [
            (
                section.card_width,
                &mut layout.card_width,
                "card-width",
                320,
            ),
            (
                section.card_height,
                &mut layout.card_height,
                "card-height",
                240,
            ),
            (section.tile_width, &mut layout.tile_width, "tile-width", 64),
            (section.spacing, &mut layout.spacing, "spacing", 0),
            (
                section.switcher_size,
                &mut layout.switcher_size,
                "switcher-size",
                24,
            ),
        ] {
            let Some(value) = value else { continue };
            if value < min {
                config.warn_at(
                    &format!("chooser.{key}"),
                    tr!("chooser-config-min", min = min),
                );
            }
            *size = f32::from(value.max(min));
        }
        layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wlr_config::Dirs;

    /// The layout and warnings of a `config.toml` holding `text`.
    fn load(text: &str) -> (Layout, Vec<String>) {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(".config/wlr-utils/config.toml");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, text).unwrap();
        let dirs = Dirs {
            home: Some(tmp.path().into()),
            config_home: Some(tmp.path().join(".config")),
            ..Dirs::default()
        };
        let mut config = Config::load(&dirs);
        let layout = Layout::from_config(&mut config);
        (layout, config.warnings().to_vec())
    }

    #[test]
    fn the_section_sets_what_it_names() {
        let (layout, warnings) = load("[chooser]\ntile-width = 200\nspacing = 0\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(layout.tile_width, 200.0);
        assert_eq!(layout.spacing, 0.0);
        assert_eq!(layout.card_width, Layout::default().card_width);
    }

    #[test]
    fn a_size_too_small_is_raised_and_named() {
        let (layout, warnings) = load("[chooser]\ntile-width = 10\n");
        assert_eq!(layout.tile_width, 64.0);
        assert!(
            matches!(&warnings[..], [w] if w.contains("`chooser.tile-width`") && w.contains("64")),
            "{warnings:?}"
        );
    }

    /// The example `config.toml`, its `[chooser]` settings uncommented, sets every size,
    /// each to its default.
    #[test]
    fn the_example_holds_the_defaults() {
        let mut current = "";
        let example: String = wlr_config::EXAMPLE
            .lines()
            .map(|line| {
                if line.starts_with('[') {
                    current = line;
                }
                let setting = line
                    .strip_prefix("# ")
                    .filter(|l| l.contains(" = ") && current == "[chooser]");
                format!("{}\n", setting.unwrap_or(line))
            })
            .collect();
        let example = example.as_str();
        let table: toml::Table = example.parse().unwrap();
        let section: Section = table["chooser"].clone().try_into().unwrap();
        assert!(
            section.card_width.is_some()
                && section.card_height.is_some()
                && section.tile_width.is_some()
                && section.spacing.is_some()
                && section.switcher_size.is_some(),
            "the example leaves a [chooser] key out"
        );
        let (layout, warnings) = load(example);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(layout, Layout::default());
    }
}
