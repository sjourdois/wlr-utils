//! Finding, reading and checking the configuration. The warnings are localised, so the
//! tests look for the file and the key they name, not for their wording.

use super::*;
use ecolor::Color32;
use serde::Deserialize;
use std::fs;

/// A home with its own XDG directories, all under a temporary directory.
struct Home {
    _tmp: tempfile::TempDir,
    dirs: Dirs,
}

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dirs = Dirs {
            home: Some(root.join("home")),
            config_home: Some(root.join("home/.config")),
            config_dirs: vec![root.join("etc/xdg")],
            data_home: Some(root.join("home/.local/share")),
            data_dirs: vec![root.join("usr/share")],
            exe_share: None,
        };
        Home { _tmp: tmp, dirs }
    }

    /// Write `text` at `path`, relative to the temporary root.
    fn write(&self, path: &str, text: &str) -> PathBuf {
        let root = self.dirs.home.as_ref().unwrap().parent().unwrap();
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        file
    }

    fn load(&self) -> Config {
        Config::load(&self.dirs)
    }
}

/// A stand-in for a tool's section.
#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(rename_all = "kebab-case", default)]
struct Draw {
    dwell_ms: Option<u64>,
    keys: Keys,
}

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
struct Keys {
    pen: Option<String>,
}

fn assert_warns(config: &Config, needles: &[&str]) {
    assert!(
        config
            .warnings()
            .iter()
            .any(|w| needles.iter().all(|n| w.contains(n))),
        "no warning with {needles:?} in {:?}",
        config.warnings()
    );
}

#[test]
fn no_file_means_the_defaults() {
    let home = Home::new();
    let config = home.load();
    assert_eq!(config.source(), &Source::Defaults);
    assert_eq!(config.theme(), &Theme::default());
    assert!(config.warnings().is_empty());
}

#[test]
fn a_named_theme_is_found_and_the_section_sets_over_it() {
    let home = Home::new();
    home.write(
        "usr/share/wlr-utils/themes/nord.toml",
        "accent = \"#88c0d0\"\nbg = \"#2e3440\"\n",
    );
    let file = home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\nname = \"nord\"\naccent = \"#ff0000\"\n",
    );
    let config = home.load();
    assert_eq!(config.source(), &Source::File(file));
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
    assert_eq!(config.theme().bg, Color32::from_rgb(0x2e, 0x34, 0x40));
    assert_eq!(config.theme().accent, Color32::from_rgb(0xff, 0, 0));
}

#[test]
fn a_theme_path_is_relative_to_the_file_that_names_it() {
    let home = Home::new();
    home.write("home/.config/wlr-utils/mine.toml", "bg = \"#010203\"\n");
    home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\nname = \"./mine.toml\"\n",
    );
    let config = home.load();
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
    assert_eq!(config.theme().bg, Color32::from_rgb(1, 2, 3));
}

#[test]
fn a_missing_theme_names_the_directories_searched() {
    let home = Home::new();
    home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\nname = \"solarized\"\n",
    );
    let config = home.load();
    assert_eq!(config.theme(), &Theme::default());
    assert_warns(&config, &["solarized", "usr/share/wlr-utils/themes"]);
}

#[test]
fn the_first_file_found_is_the_only_one_read() {
    let home = Home::new();
    let used = home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\nbg = \"#010101\"\n",
    );
    home.write("home/.wlr-utils.toml", "[theme]\nbg = \"#020202\"\n");
    home.write("home/.config/wlr-chooser/theme.toml", "bg = \"#030303\"\n");
    home.write("home/.config/wlr-draw/keys.toml", "pen = \"q\"\n");
    home.write(
        "etc/xdg/wlr-utils/config.toml",
        "[theme]\nbg = \"#040404\"\n",
    );
    let mut config = home.load();
    assert_eq!(config.source(), &Source::File(used));
    assert_eq!(config.theme().bg, Color32::from_rgb(1, 1, 1));
    assert_eq!(config.section::<Draw>("draw"), Draw::default());
    // The other user files are named, so a forgotten one is found; the system file is
    // the administrator's default, not a mistake.
    assert_eq!(config.warnings().len(), 3, "{:?}", config.warnings());
    assert_warns(&config, &[".wlr-utils.toml"]);
    assert_warns(&config, &["wlr-chooser/theme.toml"]);
    assert_warns(&config, &["wlr-draw/keys.toml"]);
}

#[test]
fn the_home_file_comes_before_the_system_one() {
    let home = Home::new();
    let used = home.write("home/.wlr-utils.toml", "[theme]\nbg = \"#020202\"\n");
    home.write(
        "etc/xdg/wlr-utils/config.toml",
        "[theme]\nbg = \"#040404\"\n",
    );
    let config = home.load();
    assert_eq!(config.source(), &Source::File(used));
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
}

#[test]
fn the_system_file_is_read_when_the_user_has_none() {
    let home = Home::new();
    let used = home.write(
        "etc/xdg/wlr-utils/config.toml",
        "[theme]\nbg = \"#040404\"\n",
    );
    let config = home.load();
    assert_eq!(config.source(), &Source::File(used));
    assert_eq!(config.theme().bg, Color32::from_rgb(4, 4, 4));
}

#[test]
fn the_old_files_are_read_as_their_sections() {
    let home = Home::new();
    let theme = home.write(
        "home/.config/wlr-chooser/theme.toml",
        "bg = \"#030303\"\nbogus = 1\n",
    );
    let keys = home.write(
        "home/.config/wlr-draw/keys.toml",
        "pen = \"q\"\ndwell-ms = 400\nrubber = \"e\"\n",
    );
    let mut config = home.load();
    assert_eq!(
        config.source(),
        &Source::Legacy {
            theme: Some(theme),
            keys: Some(keys)
        }
    );
    assert_eq!(config.theme().bg, Color32::from_rgb(3, 3, 3));
    let draw: Draw = config.section("draw");
    assert_eq!(draw.dwell_ms, Some(400));
    assert_eq!(draw.keys.pen.as_deref(), Some("q"));
    // Each old file says it is old, and a mistake in one is named as that file writes
    // it: `rubber`, not `draw.keys.rubber`.
    assert_warns(
        &config,
        &["wlr-chooser/theme.toml", "wlr-utils/config.toml"],
    );
    assert_warns(&config, &["wlr-draw/keys.toml", "wlr-utils/config.toml"]);
    assert_warns(&config, &["wlr-chooser/theme.toml", "`bogus`"]);
    assert_warns(&config, &["wlr-draw/keys.toml", "`rubber`"]);
}

#[test]
fn mistakes_are_named_with_their_file_and_key() {
    let home = Home::new();
    home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\naccent = \"blue\"\naccnet = \"#fff\"\n\n[draw]\ndwell-ms = 400\n\n\
         [draw.keys]\npne = \"p\"\n\n[drwa]\n",
    );
    let mut config = home.load();
    let _: Draw = config.section("draw");
    assert_warns(&config, &["config.toml", "`theme.accnet`"]);
    assert_warns(&config, &["config.toml", "`theme.accent`", "blue"]);
    assert_warns(&config, &["config.toml", "`draw.keys.pne`"]);
    assert_warns(&config, &["config.toml", "`drwa`"]);
    assert_eq!(config.warnings().len(), 4, "{:?}", config.warnings());
    // Plain text, in every language: no bidi isolation marks around the values.
    for warning in config.warnings() {
        assert!(!warning.contains(['\u{2068}', '\u{2069}']), "{warning:?}");
    }
}

#[test]
fn a_key_where_a_section_belongs_is_named() {
    let home = Home::new();
    home.write(
        "home/.config/wlr-utils/config.toml",
        "theme = \"nord\"\ndraw = 3\n",
    );
    let mut config = home.load();
    assert_eq!(config.section::<Draw>("draw"), Draw::default());
    assert_warns(&config, &["[theme]"]);
    assert_warns(&config, &["[draw]"]);
}

#[test]
fn a_section_of_the_wrong_shape_keeps_its_defaults() {
    let home = Home::new();
    home.write(
        "home/.config/wlr-utils/config.toml",
        "[draw]\ndwell-ms = \"long\"\n",
    );
    let mut config = home.load();
    assert_eq!(config.section::<Draw>("draw"), Draw::default());
    assert_warns(&config, &["config.toml"]);
}

#[test]
fn invalid_toml_names_its_line() {
    let home = Home::new();
    home.write(
        "home/.config/wlr-utils/config.toml",
        "[theme]\nbg = \"#000000\"\naccent = \n",
    );
    let config = home.load();
    assert_eq!(config.theme(), &Theme::default());
    assert_warns(&config, &["config.toml", "3"]);
}

/// The themes shipped in `docs/themes` set only keys a theme has, with valid colours.
#[test]
fn the_shipped_themes_are_clean() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/themes");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let mut warnings = Vec::new();
        let table = read_table(&path, &mut warnings);
        let problems = Theme::default().merge(table);
        assert!(
            warnings.is_empty() && problems.is_empty(),
            "{path:?}: {warnings:?} {problems:?}"
        );
        count += 1;
    }
    assert_eq!(count, 8);
}

/// The example `config.toml` lists every setting at its default: read as it is, it
/// changes nothing; with the settings that have no default uncommented, every key is
/// one the tools know.
#[test]
fn the_example_holds_the_defaults() {
    let example = include_str!("../../../docs/config.toml");
    let home = Home::new();
    home.write("home/.config/wlr-utils/config.toml", example);
    home.write("usr/share/wlr-utils/themes/nord.toml", "");
    let config = home.load();
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
    assert_eq!(config.theme(), &Theme::default());

    let uncommented: String = example
        .lines()
        .map(|line| {
            line.strip_prefix("# ")
                .filter(|l| l.contains(" = "))
                .unwrap_or(line)
        })
        .map(|line| format!("{line}\n"))
        .collect();
    home.write("home/.config/wlr-utils/config.toml", &uncommented);
    let config = home.load();
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
    let table: toml::Table = uncommented.parse().unwrap();
    let mut section = table["theme"].as_table().unwrap().clone();
    assert!(
        section.remove("name").is_some(),
        "the example leaves `name` out"
    );
    assert_eq!(theme::unset_keys(section), Vec::<&str>::new());
}
