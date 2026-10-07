//! Self-managed XDG autostart entry for the daemon (the `tray` feature).
//!
//! wlr-draw can launch itself on login by writing a desktop entry to
//! `~/.config/autostart/wlr-draw.desktop` — picked up by the systemd xdg-autostart
//! generator (and any XDG-compliant session). The tray's "Start on login" checkbox
//! toggles it.
//!
//! [`ensure_initialized`] performs a **one-time** auto-register on the very first run,
//! tracked by a sentinel under `$XDG_STATE_HOME`. After that the desktop file's
//! presence is the sole source of truth, so a later manual launch never resurrects an
//! entry the user deliberately removed from the tray.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME` or `~/.config`.
fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(d));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".config"))
}

/// `$XDG_STATE_HOME` or `~/.local/state`.
fn state_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(d));
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state"))
}

/// `~/.config/autostart/wlr-draw.desktop`.
fn desktop_path() -> Option<PathBuf> {
    Some(config_dir()?.join("autostart").join("wlr-draw.desktop"))
}

/// `~/.local/state/wlr-draw/autostart-initialized` — its presence means the one-time
/// auto-register has already run.
fn sentinel_path() -> Option<PathBuf> {
    Some(state_dir()?.join("wlr-draw").join("autostart-initialized"))
}

/// Whether the autostart entry currently exists.
pub fn is_enabled() -> bool {
    desktop_path().is_some_and(|p| p.exists())
}

/// Create or remove the autostart entry to match `on`.
pub fn set(on: bool) -> io::Result<()> {
    if on { enable() } else { disable() }
}

fn enable() -> io::Result<()> {
    let path = desktop_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config dir (HOME unset)"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, desktop_entry())
}

fn disable() -> io::Result<()> {
    let Some(path) = desktop_path() else {
        return Ok(());
    };
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// One-time auto-register on first ever run: if the sentinel is absent, create the
/// autostart entry and write the sentinel. After that the tray checkbox is the sole
/// source of truth — a later manual launch will not recreate a removed entry.
pub fn ensure_initialized() {
    let Some(sentinel) = sentinel_path() else {
        return;
    };
    if sentinel.exists() {
        return;
    }
    if let Err(e) = enable() {
        eprintln!("wlr-draw: could not register autostart: {e}");
        return;
    }
    if let Some(dir) = sentinel.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&sentinel, b"1") {
        eprintln!("wlr-draw: could not write autostart sentinel: {e}");
    }
}

/// The desktop entry written to `autostart/`. `Exec` names the running binary by
/// [`launcher`], so it works whether installed in `~/.local/bin`, `/usr/bin` or a Nix
/// profile.
fn desktop_entry() -> String {
    let exec = std::env::current_exe()
        .ok()
        .map(|exe| launcher(&exe, &std::env::var_os("PATH").unwrap_or_default()))
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_else(|| "wlr-draw".to_string());
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=wlr-draw\n\
         Comment=Live on-screen annotation overlay\n\
         Exec={exec}\n\
         Icon=wlr-draw\n\
         Terminal=false\n\
         Categories=Utility;\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// The path that launches `exe`: the `wlr-draw` on `path` that leads to it, so the entry
/// follows an upgrade that moves the binary — a Nix profile points at a new store path
/// on each one, and the old one is gone once collected — or `exe` itself, off `path`.
/// A `wlr-draw` leads to `exe` when, links followed, it sits in the same directory: a
/// wrapper script next to the binary it wraps is the one to run.
fn launcher(exe: &Path, path: &OsStr) -> PathBuf {
    let home = exe
        .canonicalize()
        .ok()
        .and_then(|exe| Some(exe.parent()?.to_path_buf()));
    std::env::split_paths(path)
        .map(|dir| dir.join("wlr-draw"))
        .find(|candidate| {
            let real = candidate.canonicalize().ok();
            home.is_some() && real.as_deref().and_then(Path::parent) == home.as_deref()
        })
        .unwrap_or_else(|| exe.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::launcher;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    fn file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn link(path: &Path, target: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }

    #[test]
    fn a_profile_link_is_preferred_to_the_store_path() {
        // A Nix profile: the store path changes with every upgrade, the profile's
        // link does not.
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store/abc-wlr-utils/bin/wlr-draw");
        file(&store);
        let profile = tmp.path().join("profile/bin/wlr-draw");
        link(&profile, &store);
        let path =
            std::env::join_paths([tmp.path().join("elsewhere"), tmp.path().join("profile/bin")])
                .unwrap();
        assert_eq!(launcher(&store, &path), profile);
    }

    #[test]
    fn a_wrapper_next_to_the_binary_is_the_launcher() {
        let tmp = tempfile::tempdir().unwrap();
        let wrapped = tmp.path().join("store/abc-wlr-utils/bin/.wlr-draw-wrapped");
        file(&wrapped);
        let wrapper = tmp.path().join("store/abc-wlr-utils/bin/wlr-draw");
        file(&wrapper);
        let profile = tmp.path().join("profile/bin/wlr-draw");
        link(&profile, &wrapper);
        let path = OsString::from(tmp.path().join("profile/bin"));
        assert_eq!(launcher(&wrapped, &path), profile);
    }

    #[test]
    fn another_install_on_the_path_is_not_taken() {
        // The one running is not the one PATH would start: name the running one.
        let tmp = tempfile::tempdir().unwrap();
        let running = tmp.path().join("build/wlr-draw");
        file(&running);
        file(&tmp.path().join("usr/bin/wlr-draw"));
        let path = OsString::from(tmp.path().join("usr/bin"));
        assert_eq!(launcher(&running, &path), running);
    }
}
