//! Build script: when built from the project's own git checkout, set `WLR_GIT_VERSION`
//! to the version the tools report (see `GIT_VERSION` in `lib.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let Some(state) = git_state(Path::new(env!("CARGO_MANIFEST_DIR"))) else {
        return;
    };
    for path in &state.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let version = version_from_describe(&state.describe, env!("CARGO_PKG_VERSION"));
    println!("cargo:rustc-env=WLR_GIT_VERSION={version}");
}

/// What `git` says about the checkout the crate is built from.
struct GitState {
    /// `git describe` against the release tags, or the bare commit hash if none is
    /// reachable.
    describe: String,
    /// Files whose change moves `HEAD` or the tags, so the build script re-runs.
    watched: Vec<PathBuf>,
}

/// Describe the checkout `dir` belongs to, or `None` when git is missing or `dir` is not
/// tracked there (an unpacked tarball inside some other repository — the AUR build
/// directory, `cargo package`'s — must not take that repository's version).
fn git_state(dir: &Path) -> Option<GitState> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()?;
        out.status.success().then_some(())?;
        String::from_utf8(out.stdout).ok()
    };
    git(&["ls-files", "--error-unmatch", "Cargo.toml"])?;
    let describe = git(&["describe", "--tags", "--always", "--match", "v[0-9]*"])?;
    let dirs = git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-dir",
        "--git-common-dir",
    ])?;
    let mut dirs = dirs.lines().map(PathBuf::from);
    let (git_dir, common_dir) = (dirs.next()?, dirs.next()?);
    let watched = [
        git_dir.join("HEAD"),
        common_dir.join("packed-refs"),
        common_dir.join("refs/heads"),
        common_dir.join("refs/tags"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .collect();
    Some(GitState {
        describe: describe.trim().to_string(),
        watched,
    })
}

/// Turn `git describe` output into the reported version: a release tag loses its `v`
/// (`v1.9.0-17-g7a434c1` → `1.9.0-17-g7a434c1`), and a bare hash — no tag reachable, as
/// in a shallow clone — is appended to `pkg_version` (`7a434c1` → `1.9.0-g7a434c1`).
fn version_from_describe(describe: &str, pkg_version: &str) -> String {
    match describe.strip_prefix('v') {
        Some(version) => version.to_string(),
        None => format!("{pkg_version}-g{describe}"),
    }
}
