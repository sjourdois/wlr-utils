//! Moving the old files into `config.toml`.

use super::*;
use std::fs;
use std::os::unix::fs::symlink;

struct Home {
    tmp: tempfile::TempDir,
    dirs: Dirs,
}

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dirs = Dirs {
            home: Some(root.join("home")),
            config_home: Some(root.join("home/.config")),
            data_dirs: vec![root.join("usr/share")],
            ..Dirs::default()
        };
        Home { tmp, dirs }
    }

    fn path(&self, path: &str) -> PathBuf {
        self.tmp.path().join(path)
    }

    fn write(&self, path: &str, text: &str) -> PathBuf {
        let file = self.path(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, text).unwrap();
        file
    }

    fn link(&self, path: &str, target: &Path) -> PathBuf {
        let link = self.path(path);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(target, &link).unwrap();
        link
    }
}

const THEME: &str = "home/.config/wlr-chooser/theme.toml";
const KEYS: &str = "home/.config/wlr-draw/keys.toml";
const CONFIG: &str = "home/.config/wlr-utils/config.toml";

#[test]
fn the_old_files_become_their_sections_comments_included() {
    let home = Home::new();
    home.write(
        THEME,
        "# my colours\nbg = \"#010203\"\naccent = \"#ff0000\" # red\n",
    );
    home.write(
        KEYS,
        "# my keys\npen = \"q\"\n\n# snapping\ndwell = false\ndwell-ms = 400\n\
         undo = [\"u\", \"z\"]\n# the end\n",
    );
    let text = document(&home.dirs).unwrap();
    assert_eq!(
        text,
        "[theme]\n# my colours\nbg = \"#010203\"\naccent = \"#ff0000\" # red\n\n\
         [draw]\n\n# snapping\ndwell = false\ndwell-ms = 400\n\n\
         [draw.keys]\n# my keys\npen = \"q\"\nundo = [\"u\", \"z\"]\n# the end\n",
    );
    // What it writes reads back as the old files did.
    let table: toml::Table = text.parse().unwrap();
    assert_eq!(table["draw"]["dwell-ms"].as_integer(), Some(400));
    assert_eq!(table["draw"]["keys"]["pen"].as_str(), Some("q"));
}

#[test]
fn bindings_alone_make_no_draw_header() {
    let home = Home::new();
    home.write(KEYS, "pen = \"q\"\n");
    assert_eq!(document(&home.dirs).unwrap(), "[draw.keys]\npen = \"q\"\n");
}

#[test]
fn a_link_to_an_installed_theme_becomes_its_name() {
    let home = Home::new();
    let installed = home.write("usr/share/wlr-utils/themes/nord.toml", "bg = \"#2e3440\"\n");
    home.link(THEME, &installed);
    assert_eq!(document(&home.dirs).unwrap(), "[theme]\nname = \"nord\"\n");
}

#[test]
fn a_dangling_link_named_after_an_installed_theme_becomes_its_name() {
    let home = Home::new();
    home.write("usr/share/wlr-utils/themes/catppuccin-mocha.toml", "");
    home.link(
        THEME,
        &home.path("src/gone/docs/themes/catppuccin-mocha.toml"),
    );
    assert_eq!(
        document(&home.dirs).unwrap(),
        "[theme]\nname = \"catppuccin-mocha\"\n"
    );
}

#[test]
fn a_link_to_a_theme_of_ones_own_keeps_its_path() {
    let home = Home::new();
    home.write("usr/share/wlr-utils/themes/nord.toml", "bg = \"#2e3440\"\n");
    let mine = home.write("dotfiles/nord.toml", "bg = \"#000000\"\n");
    home.link(THEME, &mine);
    assert_eq!(
        document(&home.dirs).unwrap(),
        format!("[theme]\nname = \"{}\"\n", mine.display())
    );
}

#[test]
fn a_dangling_link_to_an_unknown_theme_stops_the_migration() {
    let home = Home::new();
    home.link(THEME, &home.path("gone/solarized.toml"));
    home.write(KEYS, "pen = \"q\"\n");
    assert!(matches!(migrate(&home.dirs), Err(Error::BrokenLink { .. })));
    assert!(!home.path(CONFIG).exists());
    assert!(home.path(KEYS).exists());
}

#[test]
fn a_file_with_a_section_stops_the_migration() {
    let home = Home::new();
    home.write(KEYS, "pen = \"q\"\n[extra]\nx = 1\n");
    assert!(matches!(
        migrate(&home.dirs),
        Err(Error::Section { key, .. }) if key == "extra"
    ));
    assert!(home.path(KEYS).exists());
}

#[test]
fn nothing_to_migrate_is_said() {
    let home = Home::new();
    assert!(matches!(migrate(&home.dirs), Err(Error::Nothing)));
}

#[test]
fn an_existing_config_is_never_overwritten() {
    let home = Home::new();
    home.write(KEYS, "pen = \"q\"\n");
    home.write(CONFIG, "# mine\n");
    assert!(matches!(migrate(&home.dirs), Err(Error::Exists(_))));
    assert_eq!(fs::read_to_string(home.path(CONFIG)).unwrap(), "# mine\n");
    assert!(home.path(KEYS).exists());
}

#[test]
fn the_migration_writes_then_deletes_the_old_files_and_empty_directories() {
    let home = Home::new();
    let installed = home.write("usr/share/wlr-utils/themes/nord.toml", "");
    let theme = home.link(THEME, &installed);
    let keys = home.write(KEYS, "pen = \"q\"\n");
    let other = home.write("home/.config/wlr-draw/notes.txt", "");
    let report = migrate(&home.dirs).unwrap();
    assert_eq!(report.written, home.path(CONFIG));
    assert_eq!(
        fs::read_to_string(home.path(CONFIG)).unwrap(),
        "[theme]\nname = \"nord\"\n\n[draw.keys]\npen = \"q\"\n"
    );
    let theme_dir = theme.parent().unwrap().to_path_buf();
    let keys_dir = keys.parent().unwrap().to_path_buf();
    assert_eq!(report.removed, [theme, theme_dir.clone(), keys]);
    assert_eq!(report.kept, [keys_dir]);
    assert!(report.failed.is_empty());
    // The link went, not what it pointed to; the stranger stays.
    assert!(installed.exists());
    assert!(!theme_dir.exists());
    assert!(other.exists());
    // No temporary file is left behind.
    let names: Vec<_> = fs::read_dir(home.path("home/.config/wlr-utils"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["config.toml"]);
}
