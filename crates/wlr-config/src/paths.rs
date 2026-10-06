//! Where the configuration and the themes are looked for.

use std::path::PathBuf;

/// The configuration file, under `$XDG_CONFIG_HOME` or each of `$XDG_CONFIG_DIRS`.
const CONFIG: &str = "wlr-utils/config.toml";
/// The themes directory, under the config home and each data directory.
const THEMES: &str = "wlr-utils/themes";

/// The XDG base directories, read once from the environment, or set by a test.
#[derive(Clone, Debug, Default)]
pub struct Dirs {
    /// `$HOME`.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config_home: Option<PathBuf>,
    /// `$XDG_CONFIG_DIRS`, or `/etc/xdg`.
    pub config_dirs: Vec<PathBuf>,
    /// `$XDG_DATA_HOME`, or `~/.local/share`.
    pub data_home: Option<PathBuf>,
    /// `$XDG_DATA_DIRS`, or `/usr/local/share:/usr/share`.
    pub data_dirs: Vec<PathBuf>,
    /// `share` beside the `bin` the running binary sits in, symlinks resolved: where a
    /// Nix store path, or any install under its own prefix, keeps its data.
    pub exe_share: Option<PathBuf>,
}

/// A directory from the environment; unset and empty are the same.
fn var(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A `:`-separated list of directories from the environment, or `default`.
fn list(key: &str, default: &str) -> Vec<PathBuf> {
    let value = std::env::var(key).ok().filter(|v| !v.is_empty());
    value
        .as_deref()
        .unwrap_or(default)
        .split(':')
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn data_home(home: Option<&PathBuf>) -> Option<PathBuf> {
    var("XDG_DATA_HOME").or_else(|| Some(home?.join(".local/share")))
}

fn system_data_dirs() -> Vec<PathBuf> {
    list("XDG_DATA_DIRS", "/usr/local/share:/usr/share")
}

impl Dirs {
    /// The directories this process's environment names.
    pub fn from_env() -> Self {
        let home = var("HOME");
        Dirs {
            config_home: var("XDG_CONFIG_HOME").or_else(|| Some(home.as_ref()?.join(".config"))),
            config_dirs: list("XDG_CONFIG_DIRS", "/etc/xdg"),
            data_home: data_home(home.as_ref()),
            data_dirs: system_data_dirs(),
            exe_share: std::env::current_exe()
                .and_then(|exe| exe.canonicalize())
                .ok()
                .and_then(|exe| Some(exe.parent()?.parent()?.join("share"))),
            home,
        }
    }

    /// `$XDG_CONFIG_HOME/wlr-utils/config.toml`: the file the migration writes.
    pub fn config_file(&self) -> Option<PathBuf> {
        Some(self.config_home.as_ref()?.join(CONFIG))
    }

    /// `~/.wlr-utils.toml`, for those who keep their configuration out of `~/.config`.
    pub fn home_file(&self) -> Option<PathBuf> {
        Some(self.home.as_ref()?.join(".wlr-utils.toml"))
    }

    /// `wlr-chooser/theme.toml`, the theme before `config.toml`.
    pub fn legacy_theme(&self) -> Option<PathBuf> {
        Some(self.config_home.as_ref()?.join("wlr-chooser/theme.toml"))
    }

    /// `wlr-draw/keys.toml`, wlr-draw's settings before `config.toml`.
    pub fn legacy_keys(&self) -> Option<PathBuf> {
        Some(self.config_home.as_ref()?.join("wlr-draw/keys.toml"))
    }

    /// `wlr-utils/config.toml` under each of `$XDG_CONFIG_DIRS`, most important first.
    pub fn system_files(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.config_dirs.iter().map(|d| d.join(CONFIG))
    }

    /// Where a theme is looked for by name, most important first: the user's own, then
    /// the installed ones — next to the binary before `$XDG_DATA_DIRS`, so a store path
    /// finds its own themes before another install's.
    pub fn theme_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let bases = [&self.config_home, &self.data_home, &self.exe_share]
            .into_iter()
            .flatten()
            .chain(&self.data_dirs);
        for dir in bases.map(|base| base.join(THEMES)) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        dirs
    }
}

/// The base data directories, `$XDG_DATA_HOME` first: a user's copy of a file wins over
/// the system's.
pub fn data_dirs() -> Vec<PathBuf> {
    data_home(var("HOME").as_ref())
        .into_iter()
        .chain(system_data_dirs())
        .collect()
}
