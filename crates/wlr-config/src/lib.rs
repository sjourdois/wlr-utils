//! The configuration of the wlr-utils tools: one `config.toml` for all of them.
//!
//! [`load`] finds the file and reads it, resolving its `[theme]` against the installed
//! themes; each tool then takes its own section with [`Config::section`]. Nothing here
//! fails: a setting that cannot be read keeps its default, and why becomes a localised
//! warning, which the tool prints ([`Config::report`]) or hands back to whoever asked.
//!
//! The first file found is the only one read:
//!
//! 1. `$XDG_CONFIG_HOME/wlr-utils/config.toml` (`~/.config/wlr-utils/config.toml`);
//! 2. `~/.wlr-utils.toml`;
//! 3. the old `wlr-chooser/theme.toml` and `wlr-draw/keys.toml` under `$XDG_CONFIG_HOME`,
//!    until 2.0 — [`migrate`] turns them into the first;
//! 4. `wlr-utils/config.toml` under each of `$XDG_CONFIG_DIRS` (`/etc/xdg`).
//!
//! ```toml
//! [theme]
//! name = "nord"          # a theme file, by name or by path
//! accent = "#88c0d0"     # …and any of its keys, set over it
//!
//! [draw]
//! dwell-ms = 650
//!
//! [draw.keys]
//! pen = "p"
//! ```

mod i18n;
mod legacy;
pub mod migrate;
pub mod paths;
mod theme;

pub use paths::Dirs;
pub use theme::Theme;

use i18n::tr;
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use theme::Problem;

/// The sections of `config.toml`: any other top-level key is a mistake.
const SECTIONS: [&str; 3] = ["theme", "chooser", "draw"];

/// Where the settings came from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// No file: every setting has its default.
    Defaults,
    /// A `config.toml`, or `~/.wlr-utils.toml`.
    File(PathBuf),
    /// The old files, read until 2.0.
    Legacy {
        /// `wlr-chooser/theme.toml`, if there is one.
        theme: Option<PathBuf>,
        /// `wlr-draw/keys.toml`, if there is one.
        keys: Option<PathBuf>,
    },
}

/// The settings of every tool, read from the first file found.
#[derive(Debug)]
pub struct Config {
    source: Source,
    table: toml::Table,
    theme: Theme,
    warnings: Vec<String>,
}

/// How the running tool spells the migration command, for the warning that names it.
static MIGRATE_COMMAND: OnceLock<&'static str> = OnceLock::new();

/// Name the command that migrates the old files, as the running tool spells it
/// (`wlr-draw migrate-config`, `wlr-chooser --migrate-config`): the warning that the old
/// files are still read tells the user to run it. Call once, at startup.
pub fn set_migrate_command(command: &'static str) {
    let _ = MIGRATE_COMMAND.set(command);
}

/// Read the configuration this process's environment points at.
pub fn load() -> Config {
    Config::load(&Dirs::from_env())
}

impl Theme {
    /// The theme of the configuration, after printing what was wrong with it.
    pub fn load() -> Self {
        let config = load();
        config.report();
        config.theme
    }
}

impl Config {
    /// Read the first configuration file `dirs` leads to.
    pub fn load(dirs: &Dirs) -> Self {
        let present = |path: Option<PathBuf>| path.filter(|p| p.symlink_metadata().is_ok());
        let user: Vec<PathBuf> = [dirs.config_file(), dirs.home_file()]
            .into_iter()
            .filter_map(present)
            .collect();
        let legacy_theme = present(dirs.legacy_theme());
        let legacy_keys = present(dirs.legacy_keys());

        let mut warnings = Vec::new();
        let (source, table) = if let Some(file) = user.first() {
            for other in &user[1..] {
                warnings.push(tr!(
                    "config-shadowed",
                    file = other.display(),
                    used = file.display()
                ));
            }
            for old in legacy_theme.iter().chain(&legacy_keys) {
                warnings.push(tr!(
                    "config-legacy-ignored",
                    file = old.display(),
                    used = file.display()
                ));
            }
            (Source::File(file.clone()), read_table(file, &mut warnings))
        } else if legacy_theme.is_some() || legacy_keys.is_some() {
            let target = dirs.config_file().unwrap_or_default();
            let command = MIGRATE_COMMAND.get().copied().unwrap_or("migrate-config");
            let mut table = toml::Table::new();
            if let Some(file) = &legacy_theme {
                let theme = read_table(file, &mut warnings);
                table.insert("theme".into(), toml::Value::Table(theme));
            }
            if let Some(file) = &legacy_keys {
                let draw = legacy::draw_section(read_table(file, &mut warnings));
                table.insert("draw".into(), toml::Value::Table(draw));
            }
            for old in legacy_theme.iter().chain(&legacy_keys) {
                warnings.push(tr!(
                    "config-legacy",
                    file = old.display(),
                    command = command,
                    target = target.display()
                ));
            }
            let source = Source::Legacy {
                theme: legacy_theme,
                keys: legacy_keys,
            };
            (source, table)
        } else if let Some(file) = dirs.system_files().find(|p| p.symlink_metadata().is_ok()) {
            let table = read_table(&file, &mut warnings);
            (Source::File(file), table)
        } else {
            (Source::Defaults, toml::Table::new())
        };

        let mut config = Config {
            source,
            table,
            theme: Theme::default(),
            warnings,
        };
        let unknown: Vec<String> = config
            .table
            .keys()
            .filter(|key| !SECTIONS.contains(&key.as_str()))
            .cloned()
            .collect();
        for key in unknown {
            let message = config.unknown_key(&key);
            config.warn(message);
        }
        config.theme = config.resolve_theme(dirs);
        config
    }

    /// Where the settings came from.
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// The theme, with the defaults under what the configuration sets.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// A tool's section (`chooser`, `draw`): what it sets, the defaults for the rest. A
    /// key the schema does not know becomes a warning; a section of the wrong shape is
    /// ignored whole, with a warning.
    pub fn section<T: DeserializeOwned + Default>(&mut self, name: &str) -> T {
        let Some(value) = self.table.get(name).cloned() else {
            return T::default();
        };
        if !value.is_table() {
            let message = self.not_a_section(name);
            self.warn(message);
            return T::default();
        }
        let mut unknown = Vec::new();
        let result = serde_ignored::deserialize(value, |path| unknown.push(path.to_string()));
        match result {
            Ok(section) => {
                for key in unknown {
                    let message = self.unknown_key(&format!("{name}.{key}"));
                    self.warn(message);
                }
                section
            }
            Err(e) => {
                let (file, _) = self.locate(name);
                self.warn(tr!("config-bad-value", file = file.display(), error = e));
                T::default()
            }
        }
    }

    /// Add a warning about a setting a tool found wrong in its own section: `path` is its
    /// key in `config.toml` (`draw.keys.pen`), or the section's (`draw.keys`) for what
    /// concerns it whole. The warning names the file and the key as the user wrote them.
    pub fn warn_at(&mut self, path: &str, message: String) {
        let (file, key) = self.locate(path);
        let message = if key.is_empty() {
            tr!(
                "config-file-warning",
                file = file.display(),
                message = message
            )
        } else {
            tr!(
                "config-key-warning",
                file = file.display(),
                key = key,
                message = message
            )
        };
        self.warn(message);
    }

    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    /// What was wrong with the configuration, localised.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Print the warnings on stderr, each once per process: a daemon re-reads the
    /// configuration at every overlay it shows, and repeating itself would bury the
    /// rest of its log.
    pub fn report(&self) {
        static SEEN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        let program = program();
        for warning in &self.warnings {
            if seen.insert(warning.clone()) {
                eprintln!("{program}: {warning}");
            }
        }
    }

    /// The theme: the defaults, then the theme file `name` designates, then the other
    /// keys of `[theme]`.
    fn resolve_theme(&mut self, dirs: &Dirs) -> Theme {
        let mut theme = Theme::default();
        let Some(value) = self.table.get("theme").cloned() else {
            return theme;
        };
        let toml::Value::Table(mut section) = value else {
            let message = self.not_a_section("theme");
            self.warn(message);
            return theme;
        };
        let (file, _) = self.locate("theme.name");
        match section.remove("name") {
            None => {}
            Some(toml::Value::String(name)) => {
                let base = file.parent().unwrap_or(Path::new(""));
                match theme::find(dirs, base, &name) {
                    Ok(path) => {
                        let mut warnings = Vec::new();
                        let table = read_table(&path, &mut warnings);
                        for problem in theme.merge(table) {
                            warnings.push(describe(&path, problem));
                        }
                        self.warnings.extend(warnings);
                    }
                    Err(searched) => {
                        let searched: Vec<String> =
                            searched.iter().map(|d| d.display().to_string()).collect();
                        self.warn(tr!(
                            "config-theme-not-found",
                            file = file.display(),
                            name = name,
                            dirs = searched.join(", ")
                        ));
                    }
                }
            }
            Some(_) => self.warn(tr!("config-bad-theme-name", file = file.display())),
        }
        for problem in theme.merge(section) {
            let message = match problem {
                Problem::Unknown(key) => self.unknown_key(&format!("theme.{key}")),
                Problem::BadColour { key, value } => {
                    let (file, key) = self.locate(&format!("theme.{key}"));
                    tr!(
                        "config-bad-colour",
                        file = file.display(),
                        key = key,
                        value = value
                    )
                }
                Problem::Negative(key) => {
                    let (file, key) = self.locate(&format!("theme.{key}"));
                    tr!("config-negative", file = file.display(), key = key)
                }
                Problem::BadValue(error) => {
                    let (file, _) = self.locate("theme");
                    tr!("config-bad-value", file = file.display(), error = error)
                }
            };
            self.warn(message);
        }
        theme
    }

    /// The file a key of the configuration comes from, and how that file writes it:
    /// `draw.keys.pen` is `pen` in the old `keys.toml`.
    fn locate(&self, path: &str) -> (PathBuf, String) {
        match &self.source {
            Source::Legacy { theme, keys } => {
                let (section, rest) = path.split_once('.').unwrap_or((path, ""));
                if section == "theme" {
                    (theme.clone().unwrap_or_default(), rest.into())
                } else {
                    let key = legacy::keys_file_key(rest);
                    (keys.clone().unwrap_or_default(), key.into())
                }
            }
            Source::File(file) => (file.clone(), path.into()),
            Source::Defaults => (PathBuf::new(), path.into()),
        }
    }

    fn unknown_key(&self, path: &str) -> String {
        let (file, key) = self.locate(path);
        tr!("config-unknown-key", file = file.display(), key = key)
    }

    fn not_a_section(&self, name: &str) -> String {
        let (file, _) = self.locate(name);
        tr!("config-not-a-section", file = file.display(), key = name)
    }
}

/// A theme file's problem, as a warning.
fn describe(file: &Path, problem: Problem) -> String {
    let file = file.display();
    match problem {
        Problem::Unknown(key) => tr!("config-unknown-key", file = file, key = key),
        Problem::BadColour { key, value } => {
            tr!("config-bad-colour", file = file, key = key, value = value)
        }
        Problem::Negative(key) => tr!("config-negative", file = file, key = key),
        Problem::BadValue(error) => tr!("config-bad-value", file = file, error = error),
    }
}

/// `path`'s table, or an empty one and a warning when it cannot be read or parsed.
pub(crate) fn read_table(path: &Path, warnings: &mut Vec<String>) -> toml::Table {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            warnings.push(tr!("config-unreadable", file = path.display(), error = e));
            return toml::Table::new();
        }
    };
    match text.parse::<toml::Table>() {
        Ok(table) => table,
        Err(e) => {
            let offset = e.span().map_or(0, |span| span.start);
            let before = text.get(..offset).unwrap_or(&text);
            let line = before.matches('\n').count() + 1;
            warnings.push(tr!(
                "config-invalid",
                file = path.display(),
                line = line,
                error = e.message()
            ));
            toml::Table::new()
        }
    }
}

/// The running binary's name, to head its warnings with.
fn program() -> String {
    std::env::args_os()
        .next()
        .and_then(|arg0| {
            Path::new(&arg0)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "wlr-utils".into())
}

#[cfg(test)]
mod tests;
