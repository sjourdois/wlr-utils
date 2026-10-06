//! The files before `config.toml`, read until 2.0: `wlr-chooser/theme.toml`, which is a
//! theme file, and `wlr-draw/keys.toml`, which holds the `[draw]` settings and the
//! `[draw.keys]` bindings side by side.

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
