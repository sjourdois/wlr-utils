//! Resolve an application icon from its app-id and rasterize it to RGBA8
//! (unmultiplied alpha), for display next to window names. PNG/JPEG via `image`,
//! SVG via `resvg`.

use std::path::{Path, PathBuf};

/// Base data directories, in XDG precedence order (`XDG_DATA_HOME` first, then
/// `XDG_DATA_DIRS`): a user copy of an entry must win over the system one.
fn data_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    match std::env::var("XDG_DATA_HOME") {
        Ok(d) if !d.is_empty() => v.push(PathBuf::from(d)),
        _ => {
            if let Ok(home) = std::env::var("HOME") {
                v.push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let dirs = if dirs.is_empty() {
        "/usr/local/share:/usr/share".to_string()
    } else {
        dirs
    };
    v.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    v
}

fn app_dirs() -> Vec<PathBuf> {
    data_dirs()
        .into_iter()
        .map(|d| d.join("applications"))
        .collect()
}

fn icon_dirs() -> Vec<PathBuf> {
    // Largest first, and `scalable` ahead of the rasters: callers rasterize at
    // 64–128 px, so a vector source beats upscaling a 32×32 PNG.
    let sizes = [
        "scalable", "512x512", "256x256", "128x128", "64x64", "48x48", "32x32",
    ];
    let mut v = Vec::new();
    for base in data_dirs() {
        for s in sizes {
            v.push(base.join(format!("icons/hicolor/{s}/apps")));
        }
    }
    v.push(PathBuf::from("/usr/share/pixmaps"));
    v
}

/// The keys of a `[Desktop Entry]` group this module cares about.
struct DesktopEntry {
    icon: Option<String>,
    wm_class: Option<String>,
}

/// Read `Icon=` and `StartupWMClass=` from the `[Desktop Entry]` group only:
/// `[Desktop Action …]` groups repeat `Icon=` for their own menu item, which
/// describes the action, not the application.
fn read_entry(path: &Path) -> Option<DesktopEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut entry = DesktopEntry {
        icon: None,
        wm_class: None,
    };
    let mut in_main_group = false;
    for line in content.lines() {
        let line = line.trim();
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_main_group = group == "Desktop Entry";
            continue;
        }
        if !in_main_group {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // Only the unlocalized keys: `Icon[fr]=` names a locale-specific variant.
        let slot = match key.trim_end() {
            "Icon" => &mut entry.icon,
            "StartupWMClass" => &mut entry.wm_class,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(value.trim().to_string());
        }
    }
    Some(entry)
}

/// A `.desktop` file found while scanning the application directories.
struct DesktopFile {
    /// File name minus the `.desktop` suffix, lowercased for matching.
    stem: String,
    path: PathBuf,
}

fn desktop_files(dirs: &[PathBuf]) -> Vec<DesktopFile> {
    let mut all = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut in_dir: Vec<DesktopFile> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                Some(DesktopFile {
                    stem: name.strip_suffix(".desktop")?.to_string(),
                    path: e.path(),
                })
            })
            .collect();
        // `read_dir` yields filesystem order, so two equally good candidates would
        // otherwise resolve to a different icon from one machine (or boot) to the next.
        in_dir.sort_by(|a, b| a.stem.cmp(&b.stem));
        all.extend(in_dir);
    }
    all
}

/// Last component of a reverse-DNS name: `org.xfce.Thunar` → `Thunar`.
fn tail(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn icon_of(f: &DesktopFile) -> Option<String> {
    read_entry(&f.path)?.icon.filter(|i| !i.is_empty())
}

/// Find the icon name a `.desktop` file declares for `app_id`, searching `dirs`
/// in precedence order.
///
/// Matching follows the desktop-entry spec the way taskbars do, strongest first:
/// the app-id names the desktop file, then it matches a declared `StartupWMClass`,
/// and only then a heuristic. Substring matching is deliberately absent — it made
/// `thunar` pick up `thunar-volman-settings.desktop`, and an empty app-id match
/// every file on disk.
fn desktop_icon_in(dirs: &[PathBuf], app_id: &str) -> Option<String> {
    let id = app_id.trim().to_lowercase();
    if id.is_empty() {
        return None;
    }
    let files = desktop_files(dirs);

    // 1. The file name, exactly, then modulo a reverse-DNS prefix on either side:
    //    `thunar` ↔ `org.xfce.Thunar.desktop`, `com.anthropic.Claude` ↔ `claude.desktop`.
    if let Some(icon) = files.iter().filter(|f| f.stem == id).find_map(icon_of) {
        return Some(icon);
    }
    if let Some(icon) = files
        .iter()
        .filter(|f| tail(&f.stem) == tail(&id))
        .find_map(icon_of)
    {
        return Some(icon);
    }

    // 2. `StartupWMClass`: what an app declares precisely because its app-id and
    //    its desktop file name differ.
    if let Some(icon) = files
        .iter()
        .filter_map(|f| read_entry(&f.path))
        .find_map(|e| {
            let wm = e.wm_class?;
            (wm.to_lowercase() == id)
                .then_some(e.icon)
                .flatten()
                .filter(|i| !i.is_empty())
        })
    {
        return Some(icon);
    }

    // 3. Heuristic, last: wrapper launchers (`firefox-perso`, `steam-native`) that
    //    ship no entry of their own still deserve the base app's icon. Trailing
    //    `-segments` are dropped one at a time, never enough to match on a stub.
    let mut base = id.as_str();
    while let Some((head, _)) = base.rsplit_once('-') {
        if head.len() < 3 {
            break;
        }
        base = head;
        if let Some(icon) = files
            .iter()
            .filter(|f| f.stem == base || tail(&f.stem) == tail(base))
            .find_map(icon_of)
        {
            return Some(icon);
        }
    }
    None
}

/// Find the file backing an icon name (or an absolute path, which `Icon=` allows).
fn icon_file_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    let name = name.trim();
    // An empty name would otherwise probe for dotfiles like `<dir>/.png`.
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    // A theme icon is a bare name; a relative path would resolve against whatever
    // directory the tool happens to run from.
    if name.contains('/') {
        return None;
    }
    for dir in dirs {
        for ext in ["png", "svg", "svgz"] {
            let p = dir.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Resolve an app-id to an icon file on disk. A window with no app-id gets no
/// icon rather than an arbitrary one.
pub fn resolve(app_id: &str) -> Option<PathBuf> {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return None;
    }
    let dirs = icon_dirs();
    if let Some(name) = desktop_icon_in(&app_dirs(), app_id)
        && let Some(path) = icon_file_in(&dirs, &name)
    {
        return Some(path);
    }
    // No entry, or one pointing at a missing icon: many apps still install a theme
    // icon named after their app-id.
    icon_file_in(&dirs, app_id)
}

/// Rasterize an icon to at most `size`×`size`, RGBA8 unmultiplied.
pub fn load(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("svg") | Some("svgz") => load_svg(path, size),
        _ => load_raster(path, size),
    }
}

fn load_raster(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::open(path).ok()?.to_rgba8();
    let small = image::imageops::thumbnail(&img, size, size);
    Some((small.width(), small.height(), small.into_raw()))
}

fn load_svg(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    use resvg::{tiny_skia, usvg};
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let ts = tree.size();
    let scale = (size as f32 / ts.width()).min(size as f32 / ts.height());
    let w = ((ts.width() * scale).round() as u32).max(1);
    let h = ((ts.height() * scale).round() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia is premultiplied; un-premultiply for egui's unmultiplied textures.
    let mut rgba = pixmap.take();
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a > 0 {
            let af = a as f32;
            for c in &mut px[..3] {
                *c = (*c as f32 * 255.0 / af).round().min(255.0) as u8;
            }
        }
    }
    Some((w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Write `<dir>/<name>.desktop` with the given `[Desktop Entry]` body.
    fn desktop(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(format!("{name}.desktop")), body).unwrap();
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, []).unwrap();
    }

    fn apps() -> (tempfile::TempDir, Vec<PathBuf>) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        desktop(
            &dir,
            "org.xfce.Thunar",
            "[Desktop Entry]\nIcon=org.xfce.thunar\n",
        );
        desktop(
            &dir,
            "thunar-volman-settings",
            "[Desktop Entry]\nIcon=org.xfce.volman\n",
        );
        desktop(&dir, "discord", "[Desktop Entry]\nIcon=discord\n");
        desktop(
            &dir,
            "com.anthropic.Claude",
            "[Desktop Entry]\nIcon=claude-desktop\nStartupWMClass=com.anthropic.Claude\n",
        );
        desktop(
            &dir,
            "firefox",
            "[Desktop Entry]\nIcon=firefox\nStartupWMClass=firefox-esr\n",
        );
        let dirs = vec![dir];
        (tmp, dirs)
    }

    #[test]
    fn empty_app_id_matches_nothing() {
        let (_tmp, dirs) = apps();
        assert_eq!(desktop_icon_in(&dirs, ""), None);
        assert_eq!(desktop_icon_in(&dirs, "   "), None);
    }

    #[test]
    fn unknown_app_id_matches_nothing() {
        let (_tmp, dirs) = apps();
        assert_eq!(desktop_icon_in(&dirs, "no-such-app"), None);
    }

    #[test]
    fn app_id_matches_file_name_across_reverse_dns_prefix() {
        let (_tmp, dirs) = apps();
        // Not `org.xfce.volman`: a longer file name containing the app-id is not a match.
        assert_eq!(
            desktop_icon_in(&dirs, "thunar").as_deref(),
            Some("org.xfce.thunar")
        );
        assert_eq!(
            desktop_icon_in(&dirs, "org.xfce.Thunar").as_deref(),
            Some("org.xfce.thunar")
        );
        assert_eq!(
            desktop_icon_in(&dirs, "com.anthropic.Claude").as_deref(),
            Some("claude-desktop")
        );
    }

    #[test]
    fn file_name_match_ignores_case() {
        let (_tmp, dirs) = apps();
        assert_eq!(
            desktop_icon_in(&dirs, "DisCord").as_deref(),
            Some("discord")
        );
    }

    #[test]
    fn startup_wm_class_matches_when_the_file_name_does_not() {
        let (_tmp, dirs) = apps();
        assert_eq!(
            desktop_icon_in(&dirs, "firefox-esr").as_deref(),
            Some("firefox")
        );
    }

    #[test]
    fn file_name_wins_over_startup_wm_class() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        desktop(
            &dir,
            "aaa-impostor",
            "[Desktop Entry]\nIcon=impostor\nStartupWMClass=foot\n",
        );
        desktop(&dir, "foot", "[Desktop Entry]\nIcon=foot\n");
        assert_eq!(desktop_icon_in(&[dir], "foot").as_deref(), Some("foot"));
    }

    #[test]
    fn suffixed_launcher_falls_back_to_the_base_app() {
        let (_tmp, dirs) = apps();
        assert_eq!(
            desktop_icon_in(&dirs, "firefox-perso").as_deref(),
            Some("firefox")
        );
        // The fallback never strips down to a stub that would match anything.
        assert_eq!(desktop_icon_in(&dirs, "xy-zzy"), None);
    }

    #[test]
    fn a_desktop_action_icon_is_not_the_app_icon() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        desktop(
            &dir,
            "foot",
            "[Desktop Entry]\nName=Foot\n\n[Desktop Action New]\nIcon=foot-action\n",
        );
        desktop(&dir, "footclient", "[Desktop Entry]\nIcon=foot\n");
        // `foot.desktop` declares no app icon, so the next candidate answers.
        assert_eq!(
            desktop_icon_in(&[dir], "footclient").as_deref(),
            Some("foot")
        );
    }

    #[test]
    fn earlier_directories_win() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("user");
        let system = tmp.path().join("system");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&system).unwrap();
        desktop(&user, "steam", "[Desktop Entry]\nIcon=steam-user\n");
        desktop(&system, "steam", "[Desktop Entry]\nIcon=steam\n");
        assert_eq!(
            desktop_icon_in(&[user, system], "steam").as_deref(),
            Some("steam-user")
        );
    }

    #[test]
    fn icon_lookup_rejects_empty_and_relative_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        touch(&dir.join(".png"));
        touch(&dir.join("sub/foot.png"));
        let dirs = vec![dir];
        assert_eq!(icon_file_in(&dirs, ""), None);
        assert_eq!(icon_file_in(&dirs, "  "), None);
        assert_eq!(icon_file_in(&dirs, "sub/foot"), None);
    }

    #[test]
    fn icon_lookup_takes_an_absolute_path_as_is() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("elsewhere.png");
        touch(&png);
        let dirs = vec![tmp.path().to_path_buf()];
        assert_eq!(icon_file_in(&dirs, png.to_str().unwrap()), Some(png));
        assert_eq!(icon_file_in(&dirs, "/nope/missing.png"), None);
    }

    #[test]
    fn icon_lookup_follows_directory_order() {
        let tmp = tempfile::tempdir().unwrap();
        let scalable = tmp.path().join("scalable");
        let small = tmp.path().join("32x32");
        touch(&scalable.join("foot.svg"));
        touch(&small.join("foot.png"));
        assert_eq!(
            icon_file_in(&[scalable.clone(), small], "foot"),
            Some(scalable.join("foot.svg"))
        );
    }
}
