//! Move the old files into `config.toml`: `wlr-chooser/theme.toml` becomes `[theme]`,
//! `wlr-draw/keys.toml` becomes `[draw]` and `[draw.keys]`, comments included.
//!
//! [`migrate`] writes `config.toml` only where there is none, then deletes the old
//! files, and their directories once empty. [`document`] only says what it would write,
//! for a configuration kept in a dotfile manager. A theme that was a link to a theme
//! file stays one: `name = "nord"` when an installed theme has that name and content,
//! else `name` with the path the link pointed to.

use crate::i18n::tr;
use crate::legacy::DRAW_SETTINGS;
use crate::paths::Dirs;
use crate::theme;
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

/// Where the migrated configuration goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// `config.toml`, after which the old files are deleted.
    File,
    /// Standard output, touching no file.
    Stdout,
}

impl Output {
    /// The output a command line names: `-` for stdout, nothing for the file.
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            Some("-") => Output::Stdout,
            _ => Output::File,
        }
    }
}

/// What a migration did.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// The `config.toml` written.
    pub written: PathBuf,
    /// The old files, and their directories when that left them empty.
    pub removed: Vec<PathBuf>,
    /// The old directories that still hold other files.
    pub kept: Vec<PathBuf>,
    /// The old files that could not be deleted, and why.
    pub failed: Vec<(PathBuf, String)>,
}

/// Why nothing was migrated. Every case leaves every file as it was.
#[derive(Debug)]
pub enum Error {
    /// Neither old file exists.
    Nothing,
    /// `config.toml` already exists.
    Exists(PathBuf),
    /// An old file cannot be read.
    Unreadable { file: PathBuf, error: String },
    /// An old file is not valid TOML.
    Invalid { file: PathBuf, error: String },
    /// An old file has a section, which the old format never had: it would not survive
    /// the move into one.
    Section { file: PathBuf, key: String },
    /// `theme.toml` links to a file that is gone, and no installed theme has its name.
    BrokenLink { file: PathBuf, target: PathBuf },
    /// `config.toml` could not be written.
    Write { file: PathBuf, error: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Error::Nothing => tr!("migrate-nothing"),
            Error::Exists(file) => tr!("migrate-exists", file = file.display()),
            Error::Unreadable { file, error } => {
                tr!("config-unreadable", file = file.display(), error = error)
            }
            Error::Invalid { file, error } => {
                tr!("migrate-invalid", file = file.display(), error = error)
            }
            Error::Section { file, key } => {
                tr!("migrate-section", file = file.display(), key = key)
            }
            Error::BrokenLink { file, target } => tr!(
                "migrate-broken-link",
                file = file.display(),
                target = target.display()
            ),
            Error::Write { file, error } => {
                tr!("migrate-write-failed", file = file.display(), error = error)
            }
        };
        f.write_str(&message)
    }
}

impl std::error::Error for Error {}

/// The command itself: migrate the configuration the environment points at, and say what
/// was done on stdout, or print the document there. With no old file left, it says so
/// and succeeds: the configuration is already where it belongs, and running it again in
/// a provisioning script must not fail.
pub fn run(output: Output) -> Result<(), Error> {
    let dirs = Dirs::from_env();
    let result = match output {
        Output::Stdout => document(&dirs).map(|text| print!("{text}")),
        Output::File => migrate(&dirs).map(|report| print_report(&report)),
    };
    match result {
        Err(Error::Nothing) => {
            eprintln!("{}", tr!("migrate-nothing"));
            Ok(())
        }
        result => result,
    }
}

fn print_report(report: &Report) {
    println!("{}", tr!("migrate-wrote", file = report.written.display()));
    for path in &report.removed {
        println!("{}", tr!("migrate-removed", file = path.display()));
    }
    for dir in &report.kept {
        println!("{}", tr!("migrate-kept", dir = dir.display()));
    }
    for (file, error) in &report.failed {
        eprintln!(
            "{}",
            tr!(
                "migrate-remove-failed",
                file = file.display(),
                error = error
            )
        );
    }
}

/// The `config.toml` the old files make.
pub fn document(dirs: &Dirs) -> Result<String, Error> {
    let (theme_file, keys_file) = old_files(dirs);
    if theme_file.is_none() && keys_file.is_none() {
        return Err(Error::Nothing);
    }
    let mut doc = DocumentMut::new();
    // Comments after a file's last key belong to it: carried over in front of what
    // follows it, the next section or the end of the document.
    let mut trailing = String::new();
    if let Some(file) = &theme_file {
        let (table, rest) = theme_table(dirs, file)?;
        doc.insert("theme", Item::Table(table));
        trailing = rest;
    }
    if let Some(file) = &keys_file {
        let (mut draw, mut keys, rest) = draw_tables(file)?;
        if draw.is_empty() {
            // Bindings alone: `[draw]` gets no header, and `[draw.keys]` comes first.
            draw.set_implicit(true);
            prepend(keys.decor_mut(), &trailing);
        } else {
            prepend(draw.decor_mut(), &trailing);
        }
        draw.insert("keys", Item::Table(keys));
        doc.insert("draw", Item::Table(draw));
        trailing = rest;
    }
    doc.set_trailing(trailing);
    Ok(doc.to_string())
}

/// Write `config.toml` from the old files, then delete them, and their directories once
/// empty.
pub fn migrate(dirs: &Dirs) -> Result<Report, Error> {
    let text = document(dirs)?;
    let target = dirs.config_file().ok_or(Error::Nothing)?;
    if target.symlink_metadata().is_ok() {
        return Err(Error::Exists(target));
    }
    write_new(&target, &text).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => Error::Exists(target.clone()),
        _ => Error::Write {
            file: target.clone(),
            error: e.to_string(),
        },
    })?;

    let mut report = Report {
        written: target,
        ..Report::default()
    };
    let (theme_file, keys_file) = old_files(dirs);
    for old in theme_file.into_iter().chain(keys_file) {
        if let Err(e) = std::fs::remove_file(&old) {
            report.failed.push((old, e.to_string()));
            continue;
        }
        report.removed.push(old.clone());
        let Some(dir) = old.parent() else { continue };
        match std::fs::remove_dir(dir) {
            Ok(()) => report.removed.push(dir.to_path_buf()),
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                report.kept.push(dir.to_path_buf())
            }
            Err(_) => {}
        }
    }
    Ok(report)
}

/// The old files that exist, a dangling link included.
fn old_files(dirs: &Dirs) -> (Option<PathBuf>, Option<PathBuf>) {
    let present = |path: Option<PathBuf>| path.filter(|p| p.symlink_metadata().is_ok());
    (present(dirs.legacy_theme()), present(dirs.legacy_keys()))
}

/// `theme.toml` as the `[theme]` section, with the comments after its last key.
fn theme_table(dirs: &Dirs, file: &Path) -> Result<(Table, String), Error> {
    let unreadable = |e: std::io::Error| Error::Unreadable {
        file: file.to_path_buf(),
        error: e.to_string(),
    };
    if !file.symlink_metadata().map_err(unreadable)?.is_symlink() {
        let doc = read_doc(file)?;
        return flat_table(&doc, file, |_| true).map(|(table, _)| (table, trailing(&doc)));
    }
    let target = file
        .parent()
        .unwrap_or(Path::new(""))
        .join(std::fs::read_link(file).map_err(unreadable)?);
    let installed = target
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Some((stem, theme::find(dirs, Path::new(""), stem).ok()?)));
    let name = match installed {
        Some((stem, found)) if !target.exists() || same_content(&target, &found) => {
            stem.to_string()
        }
        _ if target.exists() => target.to_string_lossy().into_owned(),
        _ => {
            return Err(Error::BrokenLink {
                file: file.to_path_buf(),
                target,
            });
        }
    };
    let mut table = Table::new();
    table.insert("name", toml_edit::value(name));
    Ok((table, String::new()))
}

/// `keys.toml` as the `[draw]` settings and the `[draw.keys]` bindings, with the comments
/// after its last key.
fn draw_tables(file: &Path) -> Result<(Table, Table, String), Error> {
    let doc = read_doc(file)?;
    let (draw, keys) = flat_table(&doc, file, |key| DRAW_SETTINGS.contains(&key))?;
    Ok((draw, keys, trailing(&doc)))
}

/// The top-level keys of `doc`, with their comments, split by `first`: the keys it
/// accepts, then the others.
fn flat_table(
    doc: &DocumentMut,
    file: &Path,
    first: impl Fn(&str) -> bool,
) -> Result<(Table, Table), Error> {
    let (mut chosen, mut others) = (Table::new(), Table::new());
    for (key, item) in doc.as_table().iter() {
        if !item.is_value() {
            return Err(Error::Section {
                file: file.to_path_buf(),
                key: key.to_string(),
            });
        }
        let Some((formatted, item)) = doc.as_table().get_key_value(key) else {
            continue;
        };
        let into = if first(key) { &mut chosen } else { &mut others };
        into.insert_formatted(formatted, item.clone());
    }
    Ok((chosen, others))
}

fn read_doc(file: &Path) -> Result<DocumentMut, Error> {
    let text = std::fs::read_to_string(file).map_err(|e| Error::Unreadable {
        file: file.to_path_buf(),
        error: e.to_string(),
    })?;
    text.parse()
        .map_err(|e: toml_edit::TomlError| Error::Invalid {
            file: file.to_path_buf(),
            error: e.message().to_string(),
        })
}

/// The comments and blank lines after a document's last key.
fn trailing(doc: &DocumentMut) -> String {
    doc.trailing().as_str().unwrap_or_default().to_string()
}

/// Put `text` before what `decor` already holds in front of its table's header.
fn prepend(decor: &mut toml_edit::Decor, text: &str) {
    if text.is_empty() {
        return;
    }
    let before = decor
        .prefix()
        .and_then(|p| p.as_str())
        .unwrap_or_default()
        .to_string();
    decor.set_prefix(format!("{text}{before}"));
}

fn same_content(a: &Path, b: &Path) -> bool {
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// Write `text` to `target` atomically, and only if `target` does not exist: a link from
/// a complete temporary file, which fails if a file took the name in the meantime.
fn write_new(target: &Path, text: &str) -> std::io::Result<()> {
    let dir = target.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".config.toml.{}", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create_new(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::hard_link(&tmp, target)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

#[cfg(test)]
mod tests;
