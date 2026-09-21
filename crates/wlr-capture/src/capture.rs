//! High-level capture: resolve a *source* (output, window, logical region) to a
//! ready-to-use [`CapturedImage`], compositing across outputs when a region spans
//! several (possibly mixed-scale) monitors.
//!
//! This sits above the [`wl`](crate::wl) engine and is shared by the capture tools
//! (`wlr-shot` screenshots, `wlr-peek` colour/OCR). It is gated behind the `compose`
//! feature because the multi-output path resamples with `image`; a headless recorder
//! that only streams frames through [`sink`](crate::sink) doesn't pull it in.

use crate::error::{CaptureError, Context, Result};
use crate::wl::{CapturedImage, Client, Frame, Output, Region, Toplevel};
use std::time::Duration;

/// Default time to wait for a one-shot frame from a source.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(2);

/// Extract CPU pixels from a one-shot frame. shm frames are already CPU pixels;
/// dma-buf frames (only produced by the `gpu` build) are read back via an offscreen
/// GL context. The readback context is built per call — fine for a one-shot tool;
/// a streaming consumer would reuse one (see [`sink::pump`](crate::sink::pump)).
pub fn frame_to_image(frame: Frame) -> Result<CapturedImage, CaptureError> {
    match frame {
        Frame::Shm(img) => Ok(img),
        Frame::Dmabuf(d) => crate::gl::GpuReadback::new().and_then(|mut rb| rb.readback(d)),
    }
}

/// Capture once via `once`, and convert the frame to CPU pixels.
///
/// A dma-buf the GPU refuses to import is not a fatal condition: the same capture
/// works through shm, which every driver can do. On import failure the client drops
/// to shm ([`Client::disable_gpu`]) and the capture is retried once, so a driver we
/// can't hand buffers to costs a round trip rather than the whole command.
fn capture_to_image(
    client: &mut Client,
    budget: Duration,
    mut once: impl FnMut(&mut Client, Duration) -> Result<Frame, CaptureError>,
) -> Result<CapturedImage, CaptureError> {
    let frame = once(client, budget)?;
    let was_gpu = matches!(frame, Frame::Dmabuf(_));
    match frame_to_image(frame) {
        Ok(img) => Ok(img),
        Err(e) if was_gpu && !client.gpu_disabled() => {
            eprintln!("wlr-capture: dma-buf import failed ({e}); falling back to shm");
            client.disable_gpu();
            frame_to_image(once(client, budget)?)
        }
        Err(e) => Err(e),
    }
}

/// Capture a whole output: the named one, or the sole output if unnamed.
pub fn capture_output(
    client: &mut Client,
    name: Option<&str>,
    budget: Duration,
) -> Result<CapturedImage, CaptureError> {
    let outputs = client.outputs().to_vec();
    let output = match name {
        Some(n) => outputs
            .iter()
            .find(|o| o.name == n)
            .ok_or_else(|| CaptureError::OutputNotFound(n.to_string()))?,
        None => match outputs.as_slice() {
            [single] => single,
            [] => return Err(CaptureError::NoOutputs),
            many => {
                let names: Vec<&str> = many.iter().map(|o| o.name.as_str()).collect();
                return Err(CaptureError::msg(format!(
                    "multiple outputs; specify -o NAME among: {}",
                    names.join(", ")
                )));
            }
        },
    };
    capture_to_image(client, budget, |c, b| c.capture_output_once(output, b))
}

/// Capture a window by its foreign-toplevel identifier.
pub fn capture_window(
    client: &mut Client,
    id: &str,
    budget: Duration,
) -> Result<CapturedImage, CaptureError> {
    let tl = client
        .toplevels()
        .iter()
        .find(|t| t.identifier == id)
        .cloned()
        .ok_or_else(|| CaptureError::WindowNotFound(id.to_string()))?;
    capture_to_image(client, budget, |c, b| c.capture_toplevel_once(&tl, b))
}

/// Which window to act on, described the way a user thinks of it rather than by the
/// opaque identifier the compositor assigns. At least one field must be set; both
/// together narrow the match.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowFilter<'a> {
    /// Exact application id, compared case-insensitively (e.g. `firefox`).
    pub app_id: Option<&'a str>,
    /// Substring of the window title, compared case-insensitively.
    pub title: Option<&'a str>,
}

impl<'a> WindowFilter<'a> {
    /// Build a filter from a pair of optional CLI flags, or `None` when neither was given
    /// (the caller then falls back to its own default source).
    pub fn from_flags(app_id: Option<&'a str>, title: Option<&'a str>) -> Option<Self> {
        (app_id.is_some() || title.is_some()).then_some(Self { app_id, title })
    }
}

impl WindowFilter<'_> {
    /// Whether this filter selects a window with these attributes. An unset field matches
    /// anything, so an empty filter matches every window — callers reject that case before
    /// resolving.
    fn matches(&self, app_id: &str, title: &str) -> bool {
        let app_id_ok = self.app_id.is_none_or(|a| app_id.eq_ignore_ascii_case(a));
        let title_ok = self
            .title
            .is_none_or(|s| title.to_lowercase().contains(&s.to_lowercase()));
        app_id_ok && title_ok
    }

    /// How to name this filter in an error message.
    fn describe(&self) -> String {
        match (self.app_id, self.title) {
            (Some(a), Some(t)) => format!("app-id '{a}' with title containing '{t}'"),
            (Some(a), None) => format!("app-id '{a}'"),
            (None, Some(t)) => format!("title containing '{t}'"),
            (None, None) => "no window filter".to_string(),
        }
    }
}

/// Resolve a [`WindowFilter`] to exactly one window.
///
/// Ambiguity is an error rather than an arbitrary pick: the toplevel list is ordered by
/// the order the compositor advertised the windows, which is not something a script
/// should silently depend on. The error lists the candidates so the caller can re-run
/// with a tighter filter or the identifier.
pub fn resolve_window<'a>(
    toplevels: &'a [Toplevel],
    filter: WindowFilter<'_>,
) -> Result<&'a Toplevel, CaptureError> {
    let matches: Vec<&Toplevel> = toplevels
        .iter()
        .filter(|t| filter.matches(&t.app_id, &t.title))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(CaptureError::NoWindowMatches(filter.describe())),
        many => Err(CaptureError::AmbiguousWindow {
            selector: filter.describe(),
            candidates: many
                .iter()
                .map(|t| format!("  -w {}\t{}\t{}", t.identifier, t.app_id, t.title))
                .collect(),
        }),
    }
}

/// Capture the single window matching `filter` (see [`resolve_window`]).
pub fn capture_matching_window(
    client: &mut Client,
    filter: WindowFilter<'_>,
    budget: Duration,
) -> Result<CapturedImage, CaptureError> {
    let tl = resolve_window(client.toplevels(), filter)?.clone();
    capture_to_image(client, budget, |c, b| c.capture_toplevel_once(&tl, b))
}

/// A captured output paired with its geometry, for compositing a region.
pub struct OutputCapture {
    /// The output, including its placement in the global logical space.
    pub output: Output,
    /// The captured pixels of that output.
    pub image: CapturedImage,
}

/// Capture every output once — used as the interactive overlay's frozen backdrop,
/// and then to composite the chosen region from the very same pixels.
pub fn capture_all(
    client: &mut Client,
    budget: Duration,
) -> Result<Vec<OutputCapture>, CaptureError> {
    let outputs = client.outputs().to_vec();
    if outputs.is_empty() {
        return Err(CaptureError::NoOutputs);
    }
    let mut caps = Vec::with_capacity(outputs.len());
    for output in outputs {
        let image = capture_to_image(client, budget, |c, b| c.capture_output_once(&output, b))?;
        caps.push(OutputCapture { output, image });
    }
    Ok(caps)
}

/// Composite `region` from already-captured outputs. A region within a single
/// output is returned at that output's native pixel resolution; a region spanning
/// several (possibly mixed-scale) outputs is composited at logical resolution.
pub fn composite(caps: &[OutputCapture], region: Region) -> Result<CapturedImage, CaptureError> {
    if region.is_empty() {
        return Err(CaptureError::EmptyRegion);
    }
    let covering: Vec<&OutputCapture> = caps
        .iter()
        .filter(|c| region.intersect(&c.output.logical_rect()).is_some())
        .collect();

    match covering.as_slice() {
        [] => Err(CaptureError::RegionOffscreen),
        // Fast path: a single output → crop its native capture, no resampling.
        [c] => {
            let inter = region.intersect(&c.output.logical_rect()).unwrap();
            Ok(c.image.crop(logical_to_physical(&c.output, inter)))
        }
        // Multi-output: composite at logical resolution.
        many => {
            let (dw, dh) = (region.w, region.h);
            let mut dst = vec![0u8; (dw as usize) * (dh as usize) * 4];
            for c in many {
                let inter = region.intersect(&c.output.logical_rect()).unwrap();
                let phys = logical_to_physical(&c.output, inter);
                let logical = resize(c.image.crop(phys), inter.w, inter.h);
                logical.blit_into(&mut dst, dw, dh, inter.x - region.x, inter.y - region.y);
            }
            Ok(CapturedImage {
                width: dw,
                height: dh,
                rgba: dst,
            })
        }
    }
}

/// Capture a logical region live (capture only the outputs it covers, then
/// composite).
pub fn capture_region(
    client: &mut Client,
    region: Region,
    budget: Duration,
) -> Result<CapturedImage, CaptureError> {
    if region.is_empty() {
        return Err(CaptureError::EmptyRegion);
    }
    let outputs: Vec<Output> = client
        .outputs()
        .iter()
        .filter(|o| region.intersect(&o.logical_rect()).is_some())
        .cloned()
        .collect();
    if outputs.is_empty() {
        return Err(CaptureError::RegionOffscreen);
    }
    let mut caps = Vec::with_capacity(outputs.len());
    for output in outputs {
        let image = capture_to_image(client, budget, |c, b| c.capture_output_once(&output, b))?;
        caps.push(OutputCapture { output, image });
    }
    composite(&caps, region)
}

/// The bounding box of every output, in logical coordinates (the whole desktop).
pub fn whole_layout(client: &Client) -> Result<Region, CaptureError> {
    let mut it = client.outputs().iter().map(Output::logical_rect);
    let first = it.next().ok_or(CaptureError::NoOutputs)?;
    let (mut x0, mut y0) = (first.x, first.y);
    let (mut x1, mut y1) = (first.x + first.w as i32, first.y + first.h as i32);
    for r in it {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w as i32);
        y1 = y1.max(r.y + r.h as i32);
    }
    Ok(Region {
        x: x0,
        y: y0,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    })
}

/// Map a logical sub-rectangle of `output` to pixels within its capture (handles
/// fractional scale via the pixel/logical ratio).
///
/// The ratio is taken against [`Output::capture_size`], not the mode: a capture of a
/// rotated output comes back already turned into the layout's orientation, so on a
/// `90`/`270` output the mode's width is the capture's *height*.
pub fn logical_to_physical(output: &Output, logical: Region) -> Region {
    scale_into_capture(output.logical_rect(), output.capture_size(), logical)
}

/// The arithmetic behind [`logical_to_physical`]: `logical` expressed relative to
/// the output rectangle `lr`, scaled by the capture's pixels-per-point on each
/// axis. Free function so the scaling is unit-testable without a live `WlOutput`.
fn scale_into_capture(lr: Region, capture: (i32, i32), logical: Region) -> Region {
    let (cw, ch) = capture;
    let sx = cw as f64 / lr.w.max(1) as f64;
    let sy = ch as f64 / lr.h.max(1) as f64;
    Region {
        x: (((logical.x - lr.x) as f64) * sx).round() as i32,
        y: (((logical.y - lr.y) as f64) * sy).round() as i32,
        w: ((logical.w as f64) * sx).round() as u32,
        h: ((logical.h as f64) * sy).round() as u32,
    }
}

/// Resize a capture to `nw × nh` (Triangle filter); identity if already that size.
pub fn resize(img: CapturedImage, nw: u32, nh: u32) -> CapturedImage {
    if (img.width, img.height) == (nw, nh) || nw == 0 || nh == 0 {
        return img;
    }
    let Some(buf) = image::RgbaImage::from_raw(img.width, img.height, img.rgba) else {
        return CapturedImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        };
    };
    let small = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Triangle);
    CapturedImage {
        width: small.width(),
        height: small.height(),
        rgba: small.into_raw(),
    }
}

/// Encode a captured image as PNG bytes (for tools that save a still without pulling in
/// `image` themselves).
pub fn encode_png(img: &CapturedImage) -> Result<Vec<u8>, CaptureError> {
    let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
        .ok_or_else(|| CaptureError::msg("image dimensions don't match the buffer"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .context("PNG encode")?;
    Ok(out.into_inner())
}

/// Parse a slurp-style geometry: `"X,Y WxH"` (X/Y may be negative).
pub fn parse_geometry(s: &str) -> Result<Region, CaptureError> {
    let err = || CaptureError::InvalidGeometry(s.to_string());
    let (pos, size) = s.trim().split_once(' ').ok_or_else(err)?;
    let (x, y) = pos.split_once(',').ok_or_else(err)?;
    let (w, h) = size.split_once(['x', 'X', '×']).ok_or_else(err)?;
    Ok(Region {
        x: x.trim().parse().map_err(|_| err())?,
        y: y.trim().parse().map_err(|_| err())?,
        w: w.trim().parse().map_err(|_| err())?,
        h: h.trim().parse().map_err(|_| err())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_geometry_ok() {
        let r = parse_geometry("10,20 300x400").unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 300, 400));
        let r = parse_geometry("-5,-6 7x8").unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (-5, -6, 7, 8));
    }

    #[test]
    fn logical_maps_into_the_capture_of_an_untransformed_output() {
        // 3840×2160 panel shown as 1920×1080 logical: two pixels per point.
        let lr = Region {
            x: 100,
            y: 50,
            w: 1920,
            h: 1080,
        };
        let r = scale_into_capture(
            lr,
            (3840, 2160),
            Region {
                x: 100 + 10,
                y: 50 + 20,
                w: 300,
                h: 400,
            },
        );
        assert_eq!((r.x, r.y, r.w, r.h), (20, 40, 600, 800));
    }

    #[test]
    fn logical_maps_into_the_capture_of_a_rotated_output() {
        // The same panel stood on its side: 1080×1920 logical, and a capture that
        // comes back already turned, so 2160×3840 — not the mode's 3840×2160. Both
        // axes keep the same two pixels per point; taking the mode here would scale
        // x by 3.55 and y by 1.125 instead.
        let lr = Region {
            x: 0,
            y: 0,
            w: 1080,
            h: 1920,
        };
        let r = scale_into_capture(
            lr,
            (2160, 3840),
            Region {
                x: 10,
                y: 20,
                w: 300,
                h: 400,
            },
        );
        assert_eq!((r.x, r.y, r.w, r.h), (20, 40, 600, 800));
    }

    #[test]
    fn parse_geometry_bad() {
        assert!(parse_geometry("nonsense").is_err());
        assert!(parse_geometry("1,2 3").is_err());
    }

    #[test]
    fn app_id_matches_exactly_and_ignores_case() {
        let f = WindowFilter {
            app_id: Some("Firefox"),
            title: None,
        };
        assert!(f.matches("firefox", "Inbox"));
        assert!(!f.matches("firefox-esr", "Inbox")); // not a substring match
    }

    #[test]
    fn title_matches_as_a_case_insensitive_substring() {
        let f = WindowFilter {
            app_id: None,
            title: Some("inbox"),
        };
        assert!(f.matches("firefox", "Inbox — Mail"));
        assert!(!f.matches("firefox", "Drafts"));
    }

    #[test]
    fn both_fields_narrow_the_match() {
        let f = WindowFilter {
            app_id: Some("firefox"),
            title: Some("inbox"),
        };
        assert!(f.matches("firefox", "Inbox"));
        assert!(!f.matches("chromium", "Inbox"));
        assert!(!f.matches("firefox", "Drafts"));
    }

    #[test]
    fn describe_names_whichever_fields_are_set() {
        let app = WindowFilter {
            app_id: Some("firefox"),
            title: None,
        };
        assert_eq!(app.describe(), "app-id 'firefox'");
        let title = WindowFilter {
            app_id: None,
            title: Some("inbox"),
        };
        assert_eq!(title.describe(), "title containing 'inbox'");
        let both = WindowFilter {
            app_id: Some("firefox"),
            title: Some("inbox"),
        };
        assert_eq!(
            both.describe(),
            "app-id 'firefox' with title containing 'inbox'"
        );
    }
}
