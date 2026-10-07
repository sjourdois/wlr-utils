//! The files before `config.toml`, read until 2.0: `wlr-chooser/theme.toml`, which is a
//! theme file, and `wlr-draw/keys.toml`, which holds the `[draw]` settings and the
//! `[draw.keys]` bindings side by side.

use crate::i18n::tr;
use crate::paths::Dirs;
use crate::theme;
use std::path::{Path, PathBuf};

/// What `theme.toml` stands for. Before `config.toml`, a theme was picked by linking
/// `theme.toml` to it, and such a link goes on naming its theme once the target has
/// moved — a source tree renamed, a theme reinstalled elsewhere.
pub(crate) enum ThemeLink {
    /// A file of its own, whose keys are the theme.
    File,
    /// A theme by name or path: the installed theme the target is named after, when the
    /// target is gone or holds that theme; the target itself otherwise.
    Name(String),
    /// A link to nothing, named after no installed theme.
    Broken(PathBuf),
}

/// Read what `theme.toml` stands for. The reading and the migration both go through
/// here, so the theme an old file applies is the one the migration writes.
pub(crate) fn theme_link(dirs: &Dirs, file: &Path) -> std::io::Result<ThemeLink> {
    if !file.symlink_metadata()?.is_symlink() {
        return Ok(ThemeLink::File);
    }
    let target = file
        .parent()
        .unwrap_or(Path::new(""))
        .join(std::fs::read_link(file)?);
    let installed = target
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Some((stem, theme::find(dirs, Path::new(""), stem).ok()?)));
    Ok(match installed {
        Some((stem, found)) if !target.exists() || same_content(&target, &found) => {
            ThemeLink::Name(stem.to_string())
        }
        _ if target.exists() => ThemeLink::Name(target.to_string_lossy().into_owned()),
        _ => ThemeLink::Broken(target),
    })
}

/// `theme.toml` as the `[theme]` section, or `None` for a link to nothing, which is
/// said in `warnings`: the migration would refuse it, so it is not offered.
pub(crate) fn theme_section(
    dirs: &Dirs,
    file: &Path,
    warnings: &mut Vec<String>,
) -> Option<toml::Table> {
    match theme_link(dirs, file) {
        Ok(ThemeLink::File) => Some(crate::read_table(file, warnings)),
        Ok(ThemeLink::Name(name)) => Some(toml::Table::from_iter([(
            "name".to_string(),
            toml::Value::String(name),
        )])),
        Ok(ThemeLink::Broken(target)) => {
            warnings.push(tr!(
                "config-broken-link",
                file = file.display(),
                target = target.display()
            ));
            None
        }
        Err(error) => {
            warnings.push(tr!(
                "config-unreadable",
                file = file.display(),
                error = error
            ));
            Some(toml::Table::new())
        }
    }
}

fn same_content(a: &Path, b: &Path) -> bool {
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// The `[draw]` settings `keys.toml` held next to the bindings.
pub(crate) const DRAW_SETTINGS: [&str; 2] = ["dwell", "dwell-ms"];

/// `keys.toml`'s table, as the `[draw]` section holding the same settings.
pub(crate) fn draw_section(mut keys: toml::Table) -> toml::Table {
    let mut draw = toml::Table::new();
    for setting in DRAW_SETTINGS {
        if let Some(value) = keys.remove(setting) {
            draw.insert(setting.into(), value);
        }
    }
    draw.insert("keys".into(), toml::Value::Table(keys));
    draw
}

/// How a key of the synthesised `[draw]` section is written in `keys.toml`: the bindings
/// sit at its top level, so `[draw.keys]` itself is the whole file.
pub(crate) fn keys_file_key(key: &str) -> &str {
    match key {
        "keys" => "",
        _ => key.strip_prefix("keys.").unwrap_or(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_toml_splits_into_settings_and_bindings() {
        let keys: toml::Table =
            toml::from_str("pen = \"p\"\ndwell = false\ndwell-ms = 400\n").unwrap();
        let draw = draw_section(keys);
        assert_eq!(draw["dwell"], toml::Value::Boolean(false));
        assert_eq!(draw["dwell-ms"], toml::Value::Integer(400));
        assert_eq!(draw["keys"]["pen"], toml::Value::String("p".into()));
        assert_eq!(draw["keys"].as_table().unwrap().len(), 1);
        assert_eq!(keys_file_key("keys.pen"), "pen");
        assert_eq!(keys_file_key("dwell-ms"), "dwell-ms");
        assert_eq!(keys_file_key("keys"), "");
    }
}
