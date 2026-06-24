//! The annotation document and the value types behind it.
//!
//! Pure logic — no Wayland, no egui — so it is unit-tested directly. The overlay host
//! ([`crate::overlay`]) turns pointer gestures into [`Document`] mutations and paints
//! the resulting [`Element`]s; the control protocol ([`crate::proto`]) parses [`Tool`]
//! and colours from socket commands.
//!
//! Undo/redo is snapshot-based: each gesture (a finished stroke/shape/text, an eraser
//! drag, or a clear) pushes one snapshot of the element list, so undo restores the
//! whole previous state uniformly — including erases, which delete elements rather than
//! add them. Snapshots are cheap at presenter scale (a few hundred elements) and the
//! stack is capped.

/// A drawing tool. Freehand and eraser are gesture tools (the eraser deletes elements
/// under the cursor); the rest place a single [`Element`] on release.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Freehand polyline. Holding still mid-stroke snaps it to a clean line or ellipse
    /// (so there are no separate line/ellipse tools).
    Pen,
    /// Axis-aligned rectangle outline.
    Rect,
    /// Mask: a solid filled rectangle that hides/redacts the area under it.
    Mask,
    /// Straight line with an arrowhead at the end.
    Arrow,
    /// A typed text label placed where you click.
    Text,
    /// Delete whole elements the cursor passes over.
    Eraser,
    /// Select an element by clicking it, then drag (or arrow-nudge) to move it.
    Move,
}

impl Tool {
    /// Parse a tool name (as accepted by `wlr-draw tool <name>`).
    pub fn from_name(s: &str) -> Option<Tool> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "pen" | "freehand" | "draw" => Tool::Pen,
            "rect" | "rectangle" | "box" => Tool::Rect,
            "arrow" => Tool::Arrow,
            "mask" | "filled-rect" | "fill" | "redact" => Tool::Mask,
            "text" => Tool::Text,
            "eraser" | "erase" | "rubber" => Tool::Eraser,
            "move" | "select" | "grab" => Tool::Move,
            _ => return None,
        })
    }

    /// The canonical name (round-trips with [`Tool::from_name`]).
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Rect => "rect",
            Tool::Mask => "mask",
            Tool::Arrow => "arrow",
            Tool::Text => "text",
            Tool::Eraser => "eraser",
            Tool::Move => "move",
        }
    }

    /// The two-corner shape kind this tool draws, if any.
    pub fn shape_kind(self) -> Option<ShapeKind> {
        Some(match self {
            Tool::Rect => ShapeKind::Rect,
            Tool::Mask => ShapeKind::FilledRect,
            Tool::Arrow => ShapeKind::Arrow,
            _ => return None,
        })
    }
}

/// RGBA, straight (non-premultiplied) alpha.
pub type Color = [u8; 4];

/// A two-corner shape. `Line`/`Ellipse` have no dedicated tool — they are produced by
/// the pen's dwell-to-snap — but remain as kinds so those snapped shapes can be drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    Line,
    Rect,
    Ellipse,
    Arrow,
    /// A solid filled rectangle (the redaction/mask tool).
    FilledRect,
    /// A spotlight rectangle: the screen is dimmed *outside* this box (the inverse of a
    /// mask). Produced by holding Shift while dragging a rectangle.
    SpotlightRect,
    /// A spotlight ellipse: the screen is dimmed *outside* this ellipse. Produced by
    /// holding Shift while the pen dwell-snaps a loop.
    SpotlightEllipse,
}

impl ShapeKind {
    /// True for the spotlight kinds, whose veil dims everything around their (bright)
    /// hole rather than drawing an outline or fill.
    pub fn is_spotlight(self) -> bool {
        matches!(self, ShapeKind::SpotlightRect | ShapeKind::SpotlightEllipse)
    }

    /// The spotlight (inverse-mask) counterpart of a closed shape, if it has one. Holding
    /// Shift at release turns a rectangle/ellipse gesture into its spotlight.
    pub fn spotlight(self) -> Option<ShapeKind> {
        Some(match self {
            ShapeKind::Rect | ShapeKind::FilledRect => ShapeKind::SpotlightRect,
            ShapeKind::Ellipse => ShapeKind::SpotlightEllipse,
            _ => return None,
        })
    }
}

/// One drawn item, in global logical coordinates (so an element can span outputs; each
/// surface paints the portion that falls in its area).
#[derive(Clone, PartialEq, Debug)]
pub enum Element {
    /// Freehand polyline (the pen).
    Stroke {
        points: Vec<(f32, f32)>,
        color: Color,
        width: f32,
    },
    /// A line / rectangle / ellipse / arrow defined by two corners `a` and `b`.
    Shape {
        kind: ShapeKind,
        a: (f32, f32),
        b: (f32, f32),
        color: Color,
        width: f32,
    },
    /// A text label anchored at its top-left.
    Text {
        pos: (f32, f32),
        text: String,
        color: Color,
        size: f32,
    },
}

impl Element {
    /// Translate the whole element by `(dx, dy)` (arrow-key nudge of a selection).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Element::Stroke { points, .. } => {
                for p in points {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
            Element::Shape { a, b, .. } => {
                a.0 += dx;
                a.1 += dy;
                b.0 += dx;
                b.1 += dy;
            }
            Element::Text { pos, .. } => {
                pos.0 += dx;
                pos.1 += dy;
            }
        }
    }

    /// Axis-aligned bounds `((min_x, min_y), (max_x, max_y))` in global logical coords
    /// (for the selection outline). Strokes fold in half their width.
    pub fn bounds(&self) -> ((f32, f32), (f32, f32)) {
        match self {
            Element::Stroke { points, width, .. } => {
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for &(x, y) in points {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
                let h = width * 0.5;
                ((x0 - h, y0 - h), (x1 + h, y1 + h))
            }
            Element::Shape { a, b, .. } => {
                ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
            }
            Element::Text {
                pos, text, size, ..
            } => {
                let w = (text.chars().count() as f32 * size * 0.6).max(*size);
                ((pos.0, pos.1), (pos.0 + w, pos.1 + *size))
            }
        }
    }

    /// True if the element passes within `r` of point `p` (used by the eraser). The
    /// distance threshold folds in half the stroke width so thick strokes are easier
    /// to hit.
    pub fn hit(&self, p: (f32, f32), r: f32) -> bool {
        match self {
            Element::Stroke { points, width, .. } => {
                let tol = r + width * 0.5;
                points
                    .windows(2)
                    .any(|seg| dist_point_seg(p, seg[0], seg[1]) <= tol)
                    || points.first().is_some_and(|&q| dist(p, q) <= tol)
            }
            Element::Shape {
                kind, a, b, width, ..
            } => {
                let tol = r + width * 0.5;
                match kind {
                    ShapeKind::Line | ShapeKind::Arrow => dist_point_seg(p, *a, *b) <= tol,
                    ShapeKind::Rect => dist_point_rect_edge(p, *a, *b) <= tol,
                    ShapeKind::Ellipse => dist_point_ellipse_edge(p, *a, *b) <= tol,
                    // Filled box and rectangular spotlight: a hit anywhere inside the
                    // box (you grab them by their solid/bright area).
                    ShapeKind::FilledRect | ShapeKind::SpotlightRect => {
                        let (x0, x1) = (a.0.min(b.0) - tol, a.0.max(b.0) + tol);
                        let (y0, y1) = (a.1.min(b.1) - tol, a.1.max(b.1) + tol);
                        (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1)
                    }
                    // Spotlight ellipse: a hit anywhere inside the bright disc.
                    ShapeKind::SpotlightEllipse => {
                        let cx = (a.0 + b.0) * 0.5;
                        let cy = (a.1 + b.1) * 0.5;
                        let rx = (a.0 - b.0).abs() * 0.5 + tol;
                        let ry = (a.1 - b.1).abs() * 0.5 + tol;
                        if rx <= f32::EPSILON || ry <= f32::EPSILON {
                            return false;
                        }
                        let nx = (p.0 - cx) / rx;
                        let ny = (p.1 - cy) / ry;
                        nx * nx + ny * ny <= 1.0
                    }
                }
            }
            Element::Text {
                pos, text, size, ..
            } => {
                // Rough glyph box: ~0.6·size advance per char, one line tall.
                let w = (text.chars().count() as f32 * size * 0.6).max(*size);
                let (x0, y0, x1, y1) = (pos.0, pos.1, pos.0 + w, pos.1 + *size);
                p.0 >= x0 - r && p.0 <= x1 + r && p.1 >= y0 - r && p.1 <= y1 + r
            }
        }
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Distance from point `p` to segment `a`–`b`.
fn dist_point_seg(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    if len2 <= f32::EPSILON {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0);
    dist(p, (a.0 + t * abx, a.1 + t * aby))
}

/// Distance from `p` to the nearest edge of the rectangle with corners `a`, `b`.
fn dist_point_rect_edge(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    (0..4)
        .map(|i| dist_point_seg(p, corners[i], corners[(i + 1) % 4]))
        .fold(f32::INFINITY, f32::min)
}

/// Approximate distance from `p` to the ellipse inscribed in the box `a`–`b`. Uses the
/// normalized radius: cheap and good enough for hit-testing.
fn dist_point_ellipse_edge(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let cx = (a.0 + b.0) * 0.5;
    let cy = (a.1 + b.1) * 0.5;
    let rx = (a.0 - b.0).abs() * 0.5;
    let ry = (a.1 - b.1).abs() * 0.5;
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        return dist_point_seg(p, a, b);
    }
    let nx = (p.0 - cx) / rx;
    let ny = (p.1 - cy) / ry;
    let nr = (nx * nx + ny * ny).sqrt();
    // Scale the normalized boundary error back to pixels by the mean radius.
    (nr - 1.0).abs() * (rx + ry) * 0.5
}

/// What a freehand stroke was recognized as when the pen dwells (holds still) mid-draw.
/// The overlay turns this into a live, resizable shape until the button is released.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Recognized {
    /// A closed loop → an ellipse (a circle when nearly round). Resized from its centre.
    Ellipse {
        center: (f32, f32),
        /// Snap to a perfect circle (the loop was nearly round).
        circle: bool,
    },
    /// A roughly straight stroke → a line, resized from the `anchor` end.
    Line { anchor: (f32, f32) },
}

/// Try to recognize the freehand `points` as a clean shape (for dwell-to-snap). Returns
/// `None` for anything that isn't confidently a loop or a straight line, so an
/// unrecognized scribble simply stays freehand.
///
/// Heuristics on the polyline: a *line* is a path whose length barely exceeds the
/// straight distance between its ends; a *loop* returns close to its start after
/// travelling well past the bounding box. Cheap and good enough for a presenter tool.
pub fn recognize(points: &[(f32, f32)]) -> Option<Recognized> {
    if points.len() < 6 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in points {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    let diag = (bw * bw + bh * bh).sqrt();
    if diag < 24.0 {
        return None; // too small to be deliberate
    }
    let path_len: f32 = points.windows(2).map(|s| dist(s[0], s[1])).sum();
    let first = points[0];
    let last = points[points.len() - 1];
    let span = dist(first, last);

    // Straight line: the path hardly wanders from the straight shot between its ends,
    // and the ends are a meaningful distance apart.
    if span > 0.6 * diag && path_len < 1.3 * span {
        return Some(Recognized::Line { anchor: first });
    }

    // Closed loop: comes back near the start, having travelled well past the box.
    if span < 0.35 * diag && path_len > 1.8 * diag && bw > 8.0 && bh > 8.0 {
        let aspect = bw / bh;
        let circle = (0.75..=1.34).contains(&aspect);
        return Some(Recognized::Ellipse {
            center: ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
            circle,
        });
    }
    None
}

/// Constrain a two-corner shape's far corner `b` relative to its anchor `a` (held while
/// drawing): rectangles/ellipses become squares/circles; lines/arrows snap to the
/// nearest 45° (so horizontal, vertical and diagonal are easy).
pub fn constrain(kind: ShapeKind, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    match kind {
        ShapeKind::Rect
        | ShapeKind::FilledRect
        | ShapeKind::Ellipse
        | ShapeKind::SpotlightRect
        | ShapeKind::SpotlightEllipse => {
            // Square box: equal extents, keeping each axis' direction.
            let d = dx.abs().max(dy.abs());
            let sx = if dx < 0.0 { -1.0 } else { 1.0 };
            let sy = if dy < 0.0 { -1.0 } else { 1.0 };
            (a.0 + d * sx, a.1 + d * sy)
        }
        ShapeKind::Line | ShapeKind::Arrow => {
            let len = (dx * dx + dy * dy).sqrt();
            if len < f32::EPSILON {
                return b;
            }
            let step = std::f32::consts::FRAC_PI_4;
            let ang = (dy.atan2(dx) / step).round() * step;
            (a.0 + len * ang.cos(), a.1 + len * ang.sin())
        }
    }
}

/// Number of hue columns in the [`palette`] grid.
pub const PALETTE_COLS: usize = 12;

/// A grid of swatches for the colour picker, row-major (`PALETTE_COLS` per row): four
/// rows of hues (a light tint, a bright tone, a mid and a dark shade) plus a final
/// greyscale row. A real little picker, rather than a fixed cycle.
pub fn palette() -> Vec<Color> {
    // (saturation, value) per hue row.
    const ROWS: [(f32, f32); 4] = [(0.35, 1.0), (0.85, 1.0), (1.0, 0.8), (1.0, 0.5)];
    let mut out = Vec::with_capacity(PALETTE_COLS * (ROWS.len() + 1));
    for &(s, v) in &ROWS {
        for col in 0..PALETTE_COLS {
            let h = col as f32 / PALETTE_COLS as f32 * 360.0;
            let [r, g, b] = hsv_to_rgb(h, s, v);
            out.push([r, g, b, 0xff]);
        }
    }
    // Greyscale row, white → black.
    for col in 0..PALETTE_COLS {
        let t = 1.0 - col as f32 / (PALETTE_COLS - 1) as f32;
        let g = (t * 255.0).round() as u8;
        out.push([g, g, g, 0xff]);
    }
    out
}

/// HSV (`h` in degrees, `s`/`v` in 0..1) to RGB bytes.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ]
}

/// Largest number of snapshots kept on the undo stack.
const UNDO_CAP: usize = 200;

/// The annotation document: the live element list plus undo/redo snapshot stacks.
#[derive(Default)]
pub struct Document {
    els: Vec<Element>,
    undo: Vec<Vec<Element>>,
    redo: Vec<Vec<Element>>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current elements, for painting.
    pub fn elements(&self) -> &[Element] {
        &self.els
    }

    /// Snapshot the current state onto the undo stack and clear the redo stack.
    fn snapshot(&mut self) {
        self.redo.clear();
        self.undo.push(self.els.clone());
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
    }

    /// Add a finished element (one undo step).
    pub fn commit(&mut self, el: Element) {
        self.snapshot();
        self.els.push(el);
    }

    /// Begin an eraser drag: one undo step covers the whole gesture.
    pub fn begin_erase(&mut self) {
        self.snapshot();
    }

    /// Begin a move: one undo step covers a whole run of arrow-key nudges.
    pub fn begin_move(&mut self) {
        self.snapshot();
    }

    /// Translate the element at `idx` by `(dx, dy)`. Returns whether it existed.
    pub fn translate(&mut self, idx: usize, dx: f32, dy: f32) -> bool {
        match self.els.get_mut(idx) {
            Some(el) => {
                el.translate(dx, dy);
                true
            }
            None => false,
        }
    }

    /// Delete every element within `r` of `p`. Returns whether anything was removed.
    pub fn erase_at(&mut self, p: (f32, f32), r: f32) -> bool {
        let before = self.els.len();
        self.els.retain(|e| !e.hit(p, r));
        self.els.len() != before
    }

    /// End a gesture: drop the snapshot if the gesture changed nothing (e.g. an eraser
    /// click on empty space), so undo never stalls on a no-op.
    pub fn end_gesture(&mut self) {
        if self.undo.last() == Some(&self.els) {
            self.undo.pop();
        }
    }

    /// Erase everything (one undo step). No-op on an already-empty document.
    pub fn clear(&mut self) {
        if self.els.is_empty() {
            return;
        }
        self.snapshot();
        self.els.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.els, prev));
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.els, next));
            true
        } else {
            false
        }
    }
}

/// Parse a colour: a named colour or `#rgb` / `#rrggbb` / `#rrggbbaa` hex.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    Some(match s.to_ascii_lowercase().as_str() {
        "red" => [0xff, 0x3b, 0x30, 0xff],
        "green" => [0x34, 0xc7, 0x59, 0xff],
        "blue" => [0x0a, 0x84, 0xff, 0xff],
        "yellow" => [0xff, 0xd6, 0x0a, 0xff],
        "orange" => [0xff, 0x9f, 0x0a, 0xff],
        "cyan" => [0x32, 0xd7, 0xe0, 0xff],
        "magenta" | "pink" => [0xff, 0x37, 0x5f, 0xff],
        "white" => [0xff, 0xff, 0xff, 0xff],
        "black" => [0x00, 0x00, 0x00, 0xff],
        _ => return None,
    })
}

fn parse_hex(h: &str) -> Option<Color> {
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        6 => Some([byte(&h[0..2])?, byte(&h[2..4])?, byte(&h[4..6])?, 0xff]),
        8 => Some([
            byte(&h[0..2])?,
            byte(&h[2..4])?,
            byte(&h[4..6])?,
            byte(&h[6..8])?,
        ]),
        3 => {
            // Shorthand: each nibble is doubled (`#f00` → `#ff0000`).
            let nib = |c: char| {
                u8::from_str_radix(&c.to_string(), 16)
                    .ok()
                    .map(|v| v * 0x11)
            };
            let mut it = h.chars();
            Some([nib(it.next()?)?, nib(it.next()?)?, nib(it.next()?)?, 0xff])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: &[(f32, f32)]) -> Element {
        Element::Stroke {
            points: points.to_vec(),
            color: [255, 0, 0, 255],
            width: 4.0,
        }
    }

    #[test]
    fn tool_names_round_trip() {
        for t in [
            Tool::Pen,
            Tool::Rect,
            Tool::Mask,
            Tool::Arrow,
            Tool::Text,
            Tool::Eraser,
            Tool::Move,
        ] {
            assert_eq!(Tool::from_name(t.name()), Some(t));
        }
        assert_eq!(Tool::from_name("select"), Some(Tool::Move));
        assert_eq!(Tool::from_name("rectangle"), Some(Tool::Rect));
        assert_eq!(Tool::from_name("filled-rect"), Some(Tool::Mask));
        assert_eq!(Tool::from_name("nope"), None);
        // Line/ellipse have no dedicated tool (use the pen + dwell-snap).
        assert_eq!(Tool::from_name("line"), None);
        assert_eq!(Tool::from_name("ellipse"), None);
    }

    #[test]
    fn spotlight_kinds_and_conversion() {
        // Shift turns a rectangle (plain or filled) into a rectangular spotlight, and an
        // ellipse into an elliptical one; open shapes have no spotlight.
        assert_eq!(ShapeKind::Rect.spotlight(), Some(ShapeKind::SpotlightRect));
        assert_eq!(
            ShapeKind::FilledRect.spotlight(),
            Some(ShapeKind::SpotlightRect)
        );
        assert_eq!(
            ShapeKind::Ellipse.spotlight(),
            Some(ShapeKind::SpotlightEllipse)
        );
        assert_eq!(ShapeKind::Line.spotlight(), None);
        assert_eq!(ShapeKind::Arrow.spotlight(), None);
        assert!(ShapeKind::SpotlightRect.is_spotlight());
        assert!(ShapeKind::SpotlightEllipse.is_spotlight());
        assert!(!ShapeKind::Rect.is_spotlight());

        // A spotlight is grabbed by its bright interior (so the eraser can pick it up).
        let spot = Element::Shape {
            kind: ShapeKind::SpotlightRect,
            a: (0.0, 0.0),
            b: (100.0, 50.0),
            color: [0; 4],
            width: 2.0,
        };
        assert!(spot.hit((50.0, 25.0), 2.0), "inside the lit box");
        assert!(!spot.hit((400.0, 400.0), 2.0), "out in the dimmed area");
    }

    #[test]
    fn colours_named_and_hex() {
        assert_eq!(parse_color("red"), Some([0xff, 0x3b, 0x30, 0xff]));
        assert_eq!(parse_color("#ffffff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_color("#000"), Some([0, 0, 0, 255]));
        assert_eq!(parse_color("#11223344"), Some([0x11, 0x22, 0x33, 0x44]));
        assert_eq!(parse_color("#f00"), Some([255, 0, 0, 255]));
        assert_eq!(parse_color("notacolour"), None);
        assert_eq!(parse_color("#xyz"), None);
    }

    #[test]
    fn translate_moves_elements_and_bounds_follow() {
        let mut d = Document::new();
        d.commit(Element::Shape {
            kind: ShapeKind::Rect,
            a: (10.0, 20.0),
            b: (60.0, 80.0),
            color: [0; 4],
            width: 2.0,
        });
        let idx = d.elements().len() - 1;
        d.begin_move();
        assert!(d.translate(idx, 5.0, -3.0));
        let ((x0, y0), (x1, y1)) = d.elements()[idx].bounds();
        assert_eq!((x0, y0, x1, y1), (15.0, 17.0, 65.0, 77.0));
        // A move is one undo step that restores the original position.
        assert!(d.undo());
        let ((x0, y0), _) = d.elements()[0].bounds();
        assert_eq!((x0, y0), (10.0, 20.0));
        // Out-of-range index is a no-op.
        assert!(!d.translate(99, 1.0, 1.0));
    }

    #[test]
    fn undo_redo_round_trips() {
        let mut d = Document::new();
        d.commit(stroke(&[(0.0, 0.0), (1.0, 1.0)]));
        d.commit(stroke(&[(2.0, 2.0), (3.0, 3.0)]));
        assert_eq!(d.elements().len(), 2);
        assert!(d.undo());
        assert_eq!(d.elements().len(), 1);
        assert!(d.undo());
        assert!(d.elements().is_empty());
        assert!(!d.undo()); // nothing left
        assert!(d.redo());
        assert_eq!(d.elements().len(), 1);
        assert!(d.redo());
        assert_eq!(d.elements().len(), 2);
        assert!(!d.redo());
    }

    #[test]
    fn commit_clears_redo() {
        let mut d = Document::new();
        d.commit(stroke(&[(0.0, 0.0), (1.0, 1.0)]));
        d.undo();
        d.commit(stroke(&[(5.0, 5.0), (6.0, 6.0)]));
        assert!(!d.redo(), "a fresh commit must drop the redo history");
        assert_eq!(d.elements().len(), 1);
    }

    #[test]
    fn clear_is_one_undo_step() {
        let mut d = Document::new();
        d.commit(stroke(&[(0.0, 0.0), (1.0, 1.0)]));
        d.commit(stroke(&[(2.0, 2.0), (3.0, 3.0)]));
        d.clear();
        assert!(d.elements().is_empty());
        assert!(d.undo());
        assert_eq!(d.elements().len(), 2, "undo restores everything cleared");
        // Clear on an empty document does nothing (no spurious undo step).
        let mut e = Document::new();
        e.clear();
        assert!(!e.undo());
    }

    #[test]
    fn eraser_removes_hit_elements_and_is_undoable() {
        let mut d = Document::new();
        d.commit(stroke(&[(0.0, 0.0), (10.0, 0.0)]));
        d.commit(stroke(&[(0.0, 100.0), (10.0, 100.0)]));
        d.begin_erase();
        assert!(d.erase_at((5.0, 0.0), 3.0), "cursor on the first stroke");
        assert!(!d.erase_at((5.0, 50.0), 3.0), "midway: hits nothing");
        d.end_gesture();
        assert_eq!(d.elements().len(), 1);
        assert!(d.undo());
        assert_eq!(d.elements().len(), 2);
    }

    #[test]
    fn eraser_noop_gesture_leaves_no_undo_step() {
        let mut d = Document::new();
        d.commit(stroke(&[(0.0, 0.0), (10.0, 0.0)]));
        d.begin_erase();
        assert!(!d.erase_at((500.0, 500.0), 3.0));
        d.end_gesture();
        // Only the commit should be undoable, not the empty erase gesture.
        assert!(d.undo());
        assert!(d.elements().is_empty());
        assert!(!d.undo());
    }

    #[test]
    fn recognize_circle_line_and_scribble() {
        // A closed loop sampled around a circle → ellipse, snapped to a circle.
        let mut circle = Vec::new();
        for i in 0..=32 {
            let a = i as f32 / 32.0 * std::f32::consts::TAU;
            circle.push((100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin()));
        }
        match recognize(&circle) {
            Some(Recognized::Ellipse { center, circle }) => {
                assert!(circle, "a round loop should snap to a perfect circle");
                assert!((center.0 - 100.0).abs() < 2.0 && (center.1 - 100.0).abs() < 2.0);
            }
            other => panic!("expected an ellipse, got {other:?}"),
        }

        // A wide flat loop → an ellipse, not a circle.
        let mut oval = Vec::new();
        for i in 0..=32 {
            let a = i as f32 / 32.0 * std::f32::consts::TAU;
            oval.push((100.0 + 120.0 * a.cos(), 100.0 + 30.0 * a.sin()));
        }
        assert!(matches!(
            recognize(&oval),
            Some(Recognized::Ellipse { circle: false, .. })
        ));

        // A roughly straight stroke → a line anchored at the first point.
        let line: Vec<_> = (0..=20).map(|i| (i as f32 * 5.0, (i % 2) as f32)).collect();
        assert!(matches!(
            recognize(&line),
            Some(Recognized::Line { anchor }) if anchor.0.abs() < 1.0
        ));

        // A tiny jiggle is nothing deliberate.
        assert_eq!(recognize(&[(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]), None);
    }

    #[test]
    fn constrain_squares_and_axis_snaps() {
        // Rect/ellipse → square box (equal extents, sign preserved).
        assert_eq!(
            constrain(ShapeKind::Rect, (0.0, 0.0), (100.0, 40.0)),
            (100.0, 100.0)
        );
        assert_eq!(
            constrain(ShapeKind::Ellipse, (0.0, 0.0), (-30.0, 80.0)),
            (-80.0, 80.0)
        );
        // Nearly-horizontal line → snaps flat.
        let h = constrain(ShapeKind::Line, (0.0, 0.0), (100.0, 8.0));
        assert!(h.1.abs() < 0.5 && (h.0 - 100.0).abs() < 1.0, "{h:?}");
        // Nearly-vertical → snaps upright.
        let v = constrain(ShapeKind::Arrow, (0.0, 0.0), (6.0, -100.0));
        assert!(v.0.abs() < 0.5 && (v.1 + 100.0).abs() < 1.0, "{v:?}");
    }

    #[test]
    fn palette_grid_shape() {
        let p = palette();
        assert_eq!(p.len(), PALETTE_COLS * 5);
        assert!(p.iter().all(|c| c[3] == 0xff));
        // The greyscale row runs white → black.
        assert_eq!(p[PALETTE_COLS * 4], [255, 255, 255, 255]);
        assert_eq!(p[PALETTE_COLS * 5 - 1], [0, 0, 0, 255]);
    }

    #[test]
    fn shape_and_text_hit_testing() {
        let rect = Element::Shape {
            kind: ShapeKind::Rect,
            a: (0.0, 0.0),
            b: (100.0, 50.0),
            color: [0; 4],
            width: 2.0,
        };
        assert!(rect.hit((0.0, 25.0), 2.0), "on the left edge");
        assert!(!rect.hit((50.0, 25.0), 2.0), "inside, away from any edge");

        let text = Element::Text {
            pos: (10.0, 10.0),
            text: "hi".into(),
            color: [0; 4],
            size: 20.0,
        };
        assert!(text.hit((12.0, 15.0), 1.0));
        assert!(!text.hit((400.0, 400.0), 1.0));
    }
}
