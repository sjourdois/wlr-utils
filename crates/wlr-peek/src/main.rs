//! wlr-peek — inspect the screen on wlroots compositors.
//!
//! The "look at the screen" companion to `wlr-shot` (which produces image artifacts).
//! Built on the shared `wlr-capture` engine: `color` (pixel colour picker / pipette),
//! `loupe` (frozen full-screen magnifier), `mirror` (live floating window/region
//! mirror, the former `wlr-pip`), and `ocr` (Tesseract text recognition).

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use wlr_capture::capture::{self, DEFAULT_BUDGET};
use wlr_capture::overlay;
use wlr_capture::wl::{self, Region};

#[derive(Parser)]
#[command(
    name = "wlr-peek",
    version,
    about = "Inspect the screen on wlroots (colour picker, OCR)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pick a colour from anywhere on screen (pipette), with a magnifying loupe.
    Color(ColorArgs),
    /// Magnify the screen around the cursor; scroll to zoom, Esc to quit.
    Loupe,
    /// Mirror a window live in a floating, always-on-top window (picture-in-picture).
    Mirror(MirrorArgs),
    /// Recognise text in a screen region (OCR, via Tesseract).
    #[cfg(feature = "ocr")]
    Ocr(OcrArgs),
    /// Internal: serve a clipboard selection read from stdin. Spawned detached by
    /// `color --clipboard`; not meant to be run by hand.
    #[command(hide = true)]
    ClipboardServe {
        /// MIME type to advertise for the bytes on stdin.
        #[arg(long)]
        mime: String,
    },
}

#[derive(Args)]
struct ColorArgs {
    /// How to format the picked colour.
    #[arg(short = 'f', long, value_enum, default_value_t = Format::Hex)]
    format: Format,
    /// Copy the colour to the Wayland clipboard instead of printing it. Runs a small
    /// background daemon that serves the selection until another client replaces it.
    #[arg(short = 'c', long)]
    clipboard: bool,
    /// With `--clipboard`: serve the selection in the foreground (don't detach).
    /// Mainly for scripts and debugging; the process blocks until replaced.
    #[arg(long, requires = "clipboard")]
    clipboard_foreground: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// `#RRGGBB`
    Hex,
    /// `rgb(R, G, B)`
    Rgb,
    /// `R G B`
    Plain,
}

fn main() {
    wlr_capture::i18n::init();
    let cli = Cli::parse();
    let res = match cli.cmd {
        Cmd::Color(args) => color(args),
        Cmd::Loupe => loupe(),
        Cmd::Mirror(args) => mirror(args),
        #[cfg(feature = "ocr")]
        Cmd::Ocr(args) => ocr(args),
        Cmd::ClipboardServe { mime } => clipboard_serve(&mime),
    };
    if let Err(e) = res {
        eprintln!("wlr-peek: {e:#}");
        std::process::exit(1);
    }
}

fn color(args: ColorArgs) -> Result<()> {
    let mut client = wl::Client::connect().context("Wayland connection")?;
    client.refresh().ok();

    // Freeze every output, let the user aim and click a pixel on the loupe overlay,
    // then read that pixel back from the very same frozen capture.
    let caps = capture::capture_all(&mut client, DEFAULT_BUDGET)?;
    let Some((x, y)) = overlay::pick_point(&caps)? else {
        std::process::exit(1); // cancelled
    };

    let px = capture::composite(&caps, Region { x, y, w: 1, h: 1 }).context("reading pixel")?;
    let [r, g, b, _] = px.pixel(0, 0).context("pixel off-screen")?;
    let text = format_color(args.format, r, g, b);

    if args.clipboard {
        copy_text(text, args.clipboard_foreground).context("copying to clipboard")?;
    } else {
        println!("{text}");
    }
    Ok(())
}

/// Freeze the screen and show a full-screen magnifier that follows the cursor (scroll
/// to zoom, Esc to quit). Frozen rather than live: a full-screen live magnifier would
/// capture its own output. For a live zoom of a fixed region, use `mirror -g`.
fn loupe() -> Result<()> {
    let mut client = wl::Client::connect().context("Wayland connection")?;
    client.refresh().ok();
    let caps = capture::capture_all(&mut client, DEFAULT_BUDGET)?;
    overlay::magnify(&caps)
}

#[derive(Args)]
struct MirrorArgs {
    /// The `ext-foreign-toplevel` identifier of the window to mirror (as printed by
    /// `wlr-chooser`). With no ID and no `-g`, the chooser is launched to pick one.
    #[arg(group = "source")]
    id: Option<String>,
    /// Mirror (and magnify) this logical region instead of a window, `"X,Y WxH"`
    /// (the format slurp prints) — a live, always-on-top loupe of a fixed area.
    #[arg(short = 'g', long, value_name = "GEOM", group = "source")]
    geometry: Option<String>,
    /// With `-g`: magnification factor (window starts at region × zoom).
    #[arg(long, default_value_t = 2.0, requires = "geometry")]
    zoom: f32,
}

/// Mirror a window (or, with `-g`, a region) live in a floating, always-on-top
/// window. One mirror per window (an advisory lock makes a second launch for the
/// same window a no-op).
fn mirror(args: MirrorArgs) -> Result<()> {
    if let Some(geo) = args.geometry {
        return mirror_region(&geo, args.zoom);
    }
    let id = match args.id {
        Some(id) => id,
        None => match pick_via_chooser() {
            Some(id) => id,
            None => std::process::exit(1), // cancelled or chooser unavailable
        },
    };
    let _lock = match mirror_lock(&id) {
        Some(lock) => lock,
        None => return Ok(()), // already mirroring this window
    };
    let (label, icon) = resolve_window(&id);
    wlr_capture::mirror::run(
        wlr_capture::mirror::Source::Toplevel(id),
        wlr_capture::mirror::Config {
            app_id: "wlr-peek-mirror".to_string(),
            label,
            icon,
            relaunch: vec!["mirror".to_string()],
        },
    )
}

/// Mirror a fixed logical region live, magnified by `zoom`. Resolves the output the
/// region's top-left corner sits on (mono-output for now), clips the region to it,
/// and streams that output, showing only the region's sub-rectangle.
fn mirror_region(geo: &str, zoom: f32) -> Result<()> {
    let region = capture::parse_geometry(geo)?;
    if region.is_empty() {
        anyhow::bail!("empty region");
    }
    let mut client = wl::Client::connect().context("Wayland connection")?;
    client.refresh().ok();

    // The output under the region's top-left corner; clip the region to it.
    let corner = Region {
        x: region.x,
        y: region.y,
        w: 1,
        h: 1,
    };
    let output = client
        .outputs()
        .iter()
        .find(|o| o.logical_rect().intersect(&corner).is_some())
        .cloned()
        .context("the region's top-left corner is on no output")?;
    let lr = output.logical_rect();
    let region = region
        .intersect(&lr)
        .context("region does not overlap its output")?;

    // Normalized sub-rectangle of the region within the output (scale-independent).
    let crop = [
        (region.x - lr.x) as f32 / lr.w.max(1) as f32,
        (region.y - lr.y) as f32 / lr.h.max(1) as f32,
        (region.x + region.w as i32 - lr.x) as f32 / lr.w.max(1) as f32,
        (region.y + region.h as i32 - lr.y) as f32 / lr.h.max(1) as f32,
    ];

    wlr_capture::mirror::run(
        wlr_capture::mirror::Source::Region {
            output: output.name.clone(),
            crop,
            region_w: region.w,
            region_h: region.h,
            zoom,
        },
        wlr_capture::mirror::Config {
            app_id: "wlr-peek-mirror".to_string(),
            label: format!("{},{} {}×{}", region.x, region.y, region.w, region.h),
            icon: None,
            relaunch: vec![], // no re-pick in region mode
        },
    )
}

/// Launch `wlr-chooser --windows` to pick a window; parse its `Window: <id>` stdout
/// contract. Prefers a `wlr-chooser` next to our own binary, else one on `PATH`.
fn pick_via_chooser() -> Option<String> {
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("wlr-chooser")))
        .filter(|p| p.exists());
    let mut cmd = match sibling {
        Some(p) => std::process::Command::new(p),
        None => std::process::Command::new("wlr-chooser"),
    };
    let out = cmd.arg("--windows").output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Window: ").map(|id| id.trim().to_string()))
        .filter(|id| !id.is_empty())
}

/// Acquire the single-instance advisory lock for this window's mirror; the held file
/// (keep it alive) or `None` if another mirror already owns it.
fn mirror_lock(identifier: &str) -> Option<std::fs::File> {
    use rustix::fs::{FlockOperation, flock};
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let safe: String = identifier
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(format!("wlr-peek-mirror-{safe}.lock")))
        .ok()?;
    flock(&f, FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(f)
}

/// Look up the target window's app-id/title (label) and app icon. Falls back to a
/// generic label if it isn't found right now (it may map shortly).
fn resolve_window(identifier: &str) -> (String, Option<(u32, u32, Vec<u8>)>) {
    let mut client = match wl::Client::connect() {
        Ok(c) => c,
        Err(_) => return ("wlr-peek".to_string(), None),
    };
    let _ = client.refresh();
    let Some(t) = client
        .toplevels()
        .iter()
        .find(|t| t.identifier == identifier)
    else {
        return ("wlr-peek".to_string(), None);
    };
    let label = if t.app_id.is_empty() {
        t.title.clone()
    } else if t.title.is_empty() {
        t.app_id.clone()
    } else {
        format!("{} — {}", t.app_id, t.title)
    };
    let icon =
        wlr_capture::icons::resolve(&t.app_id).and_then(|p| wlr_capture::icons::load(&p, 64));
    (label, icon)
}

/// Render a colour in the requested format.
fn format_color(fmt: Format, r: u8, g: u8, b: u8) -> String {
    match fmt {
        Format::Hex => format!("#{r:02X}{g:02X}{b:02X}"),
        Format::Rgb => format!("rgb({r}, {g}, {b})"),
        Format::Plain => format!("{r} {g} {b}"),
    }
}

/// Put `text` on the Wayland clipboard (as UTF-8 text). Unless `foreground`, detach a
/// background daemon (a re-exec in `clipboard-serve` mode) that serves the selection
/// until replaced — the clipboard is pull-based, so it must outlive this process.
fn copy_text(text: String, foreground: bool) -> Result<()> {
    const MIME: &str = "text/plain;charset=utf-8";
    if foreground {
        return wlr_capture::clipboard::serve(MIME, text.into_bytes());
    }
    wlr_capture::clipboard::spawn_detached(text.as_bytes(), &["clipboard-serve", "--mime", MIME])
}

#[cfg(feature = "ocr")]
#[derive(Args)]
struct OcrArgs {
    /// OCR this logical region, `"X,Y WxH"` (the format slurp prints), stitched
    /// across the outputs it covers.
    #[arg(short = 'g', long, value_name = "GEOM", group = "source")]
    geometry: Option<String>,
    /// OCR this whole named output (e.g. `DP-4`).
    #[arg(short = 'o', long, value_name = "NAME", group = "source")]
    output: Option<String>,
    /// OCR the active (focused) window — needs compositor focus info.
    #[arg(short = 'a', long, group = "source")]
    active_window: bool,
    /// OCR the focused output — needs compositor focus info.
    #[arg(long, group = "source")]
    current_output: bool,
    /// Tesseract language(s), e.g. `eng`, `fra`, `fra+eng` (needs the tessdata pack).
    #[arg(short = 'l', long, default_value = "eng")]
    lang: String,
    /// Copy the recognised text to the Wayland clipboard instead of printing it.
    #[arg(short = 'c', long)]
    clipboard: bool,
    /// With `--clipboard`: serve the selection in the foreground (don't detach).
    #[arg(long, requires = "clipboard")]
    clipboard_foreground: bool,
}

#[cfg(feature = "ocr")]
fn ocr(args: OcrArgs) -> Result<()> {
    let mut client = wl::Client::connect().context("Wayland connection")?;
    client.refresh().ok();

    let img = ocr_source(&mut client, &args)?;
    let text = clean_ocr(&run_ocr(&img, &args.lang)?);

    if args.clipboard {
        copy_text(text, args.clipboard_foreground).context("copying to clipboard")?;
    } else {
        println!("{text}");
    }
    Ok(())
}

/// Resolve the OCR source (the flags form an exclusive group; with none, the default
/// is the interactive region selector) to a captured image.
#[cfg(feature = "ocr")]
fn ocr_source(client: &mut wl::Client, args: &OcrArgs) -> Result<wl::CapturedImage> {
    if let Some(geo) = &args.geometry {
        capture::capture_region(client, capture::parse_geometry(geo)?, DEFAULT_BUDGET)
    } else if let Some(name) = &args.output {
        capture::capture_output(client, Some(name), DEFAULT_BUDGET)
    } else if args.active_window {
        capture::capture_region(client, active_window_rect()?, DEFAULT_BUDGET)
    } else if args.current_output {
        capture::capture_output(client, Some(&focused_output()?), DEFAULT_BUDGET)
    } else {
        // Default (no source flag): freeze, let the user drag a region.
        let caps = capture::capture_all(client, DEFAULT_BUDGET)?;
        match overlay::select_region(&caps)? {
            Some(region) => capture::composite(&caps, region),
            None => std::process::exit(1), // cancelled
        }
    }
}

/// Tidy Tesseract output: drop the form-feed page separators it appends, trim
/// trailing whitespace from each line, collapse runs of blank lines to at most one,
/// and trim the ends. Tesseract is liberal with blank lines and a trailing `\x0c`.
#[cfg(feature = "ocr")]
fn clean_ocr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        let line = line.trim_end_matches([' ', '\t', '\r', '\u{0c}']);
        if line.is_empty() {
            blank_run += 1;
            if blank_run == 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out.truncate(out.trim_end().len());
    out
}

/// Run Tesseract over a captured image and return the recognised text.
#[cfg(feature = "ocr")]
fn run_ocr(img: &wl::CapturedImage, lang: &str) -> Result<String> {
    use std::io::Cursor;
    if img.width == 0 || img.height == 0 {
        anyhow::bail!("empty image (region off-screen?)");
    }
    // Tesseract reads an encoded image from memory; PNG is lossless and cheap here.
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
        .context("inconsistent dimensions/buffer")?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .context("encoding image for OCR")?;

    let mut lt = leptess::LepTess::new(None, lang)
        .map_err(|e| anyhow::anyhow!("Tesseract init (language '{lang}'): {e:?}"))?;
    lt.set_image_from_mem(&png)
        .map_err(|e| anyhow::anyhow!("loading image into Tesseract: {e:?}"))?;
    // Screen captures carry no DPI metadata; set one (after the image) so Tesseract
    // skips guessing it. Must follow set_image, or libtesseract warns and ignores it.
    lt.set_source_resolution(96);
    lt.get_utf8_text().context("extracting text")
}

/// The active window's logical rectangle, via compositor focus IPC.
#[cfg(feature = "ocr")]
fn active_window_rect() -> Result<Region> {
    let backend = wlr_capture::focus::detect()
        .context("focus info unavailable (unsupported compositor); try -s or -g")?;
    backend
        .active_window_rect()
        .with_context(|| format!("no active window detected (via {})", backend.name()))
}

/// The focused output's name, via compositor focus IPC.
#[cfg(feature = "ocr")]
fn focused_output() -> Result<String> {
    let backend = wlr_capture::focus::detect()
        .context("focus info unavailable (unsupported compositor); specify -o NAME")?;
    backend
        .focused_output()
        .with_context(|| format!("no focused output detected (via {})", backend.name()))
}

/// The `clipboard-serve` daemon body: read the blob from stdin, then serve it.
fn clipboard_serve(mime: &str) -> Result<()> {
    use std::io::Read;
    let mut data = Vec::new();
    std::io::stdin()
        .read_to_end(&mut data)
        .context("reading clipboard data")?;
    wlr_capture::clipboard::serve(mime, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_color_variants() {
        assert_eq!(format_color(Format::Hex, 0x4d, 0x9a, 0xff), "#4D9AFF");
        assert_eq!(format_color(Format::Rgb, 77, 154, 255), "rgb(77, 154, 255)");
        assert_eq!(format_color(Format::Plain, 0, 128, 255), "0 128 255");
    }

    #[cfg(feature = "ocr")]
    #[test]
    fn clean_ocr_tidies_tesseract_output() {
        // Trailing form-feed + blank lines, double blank lines, trailing spaces.
        let raw = "line one  \n\n\nline two   \n\u{0c}\n";
        assert_eq!(clean_ocr(raw), "line one\n\nline two");
        assert_eq!(clean_ocr("solo\n\u{0c}"), "solo");
        assert_eq!(clean_ocr("   \n\n"), "");
    }
}
