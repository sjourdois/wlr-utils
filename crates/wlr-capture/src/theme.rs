//! Colour & font theme. Sensible generic-dark defaults, overridable from
//! `~/.config/wlr-chooser/theme.toml` (or `$XDG_CONFIG_HOME/wlr-chooser/theme.toml`).
//!
//! Colour keys are `#rrggbb` / `#rrggbbaa`; font keys configure the UI font:
//!
//! ```toml
//! accent = "#89b4fa"
//! screen-accent = "#89b4fa"
//! window-accent = "#cba6f7"
//!
//! font = "JetBrains Mono"   # UI font family (resolved via fontconfig)
//! # font-path = "/path/to/Font.ttf"   # …or a direct file
//! # cjk-font = "Noto Sans CJK JP"     # CJK fallback (else auto-detected)
//! font-size = 15.0
//! ```
//!
//! Rendering CJK text (Japanese/Chinese/Korean) needs a CJK font installed; one
//! is auto-detected and used as a fallback.

use egui::Color32;
use serde::Deserialize;
use std::sync::Mutex;

/// Colours and fonts for the overlay UI, with generic-dark defaults (see [`Default`])
/// overridable from `theme.toml`.
#[derive(Clone)]
pub struct Theme {
    /// Dimmed overlay drawn behind the card (lock mode).
    pub backdrop: Color32,
    /// Window background (no-lock mode).
    pub bg: Color32,
    /// The centred card.
    pub card: Color32,
    /// Tile background.
    pub tile: Color32,
    /// Tile background when hovered.
    pub tile_hover: Color32,
    /// Tile background when selected.
    pub tile_selected: Color32,
    /// Thumbnail letterbox area.
    pub thumb: Color32,
    /// Labels.
    pub text: Color32,
    /// Placeholders and secondary text.
    pub text_dim: Color32,
    /// General accent (selection, focus).
    pub accent: Color32,
    /// Outline and glyph for OUTPUT tiles.
    pub screen_accent: Color32,
    /// Outline for WINDOW tiles.
    pub window_accent: Color32,

    /// UI font family (resolved via fontconfig).
    pub font: Option<String>,
    /// …or a direct font file.
    pub font_path: Option<String>,
    /// CJK fallback family (else auto-detected).
    pub cjk_font: Option<String>,
    /// Base UI text size in points.
    pub font_size: Option<f32>,
}

impl Default for Theme {
    fn default() -> Self {
        let c = |r, g, b| Color32::from_rgb(r, g, b);
        Self {
            backdrop: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            bg: c(0x1e, 0x21, 0x27),
            card: c(0x21, 0x25, 0x2d),
            tile: c(0x18, 0x1b, 0x22),
            tile_hover: c(0x26, 0x2b, 0x33),
            tile_selected: c(0x3b, 0x42, 0x52),
            thumb: c(0x12, 0x14, 0x1a),
            text: c(0xd8, 0xde, 0xe9),
            text_dim: c(0x7a, 0x82, 0x90),
            accent: c(0x88, 0xc0, 0xd0),
            screen_accent: c(0x81, 0xa1, 0xc1), // blue — screens
            window_accent: c(0xb4, 0x8e, 0xad), // purple — windows
            font: None,
            font_path: None,
            cjk_font: None,
            font_size: None,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
struct Raw {
    backdrop: Option<String>,
    bg: Option<String>,
    card: Option<String>,
    tile: Option<String>,
    tile_hover: Option<String>,
    tile_selected: Option<String>,
    thumb: Option<String>,
    text: Option<String>,
    text_dim: Option<String>,
    accent: Option<String>,
    screen_accent: Option<String>,
    window_accent: Option<String>,
    font: Option<String>,
    font_path: Option<String>,
    cjk_font: Option<String>,
    font_size: Option<f32>,
}

impl Theme {
    /// Load the theme, applying any overrides from the user config.
    pub fn load() -> Self {
        let mut t = Theme::default();
        let Some(raw) = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<Raw>(&s).ok())
        else {
            return t;
        };
        let set = |dst: &mut Color32, src: &Option<String>| {
            if let Some(c) = src.as_deref().and_then(parse_hex) {
                *dst = c;
            }
        };
        set(&mut t.backdrop, &raw.backdrop);
        set(&mut t.bg, &raw.bg);
        set(&mut t.card, &raw.card);
        set(&mut t.tile, &raw.tile);
        set(&mut t.tile_hover, &raw.tile_hover);
        set(&mut t.tile_selected, &raw.tile_selected);
        set(&mut t.thumb, &raw.thumb);
        set(&mut t.text, &raw.text);
        set(&mut t.text_dim, &raw.text_dim);
        set(&mut t.accent, &raw.accent);
        set(&mut t.screen_accent, &raw.screen_accent);
        set(&mut t.window_accent, &raw.window_accent);
        t.font = raw.font;
        t.font_path = raw.font_path;
        t.cjk_font = raw.cjk_font;
        t.font_size = raw.font_size;
        t
    }

    /// Apply the palette to egui's global visuals (panels, widgets, selection…).
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = egui::Visuals::dark();
        v.panel_fill = self.bg;
        v.window_fill = self.card;
        v.extreme_bg_color = self.thumb;
        v.override_text_color = Some(self.text);
        v.selection.bg_fill = self.accent.gamma_multiply(0.4);
        v.selection.stroke = egui::Stroke::new(1.0, self.accent);
        v.hyperlink_color = self.accent;
        v.widgets.hovered.bg_fill = self.tile_hover;
        v.widgets.active.bg_fill = self.tile_selected;
        ctx.set_visuals(v);

        self.install_fonts(ctx);
        if let Some(sz) = self.font_size {
            ctx.global_style_mut(|s| {
                use egui::{FontFamily, FontId, TextStyle};
                let prop = FontFamily::Proportional;
                s.text_styles
                    .insert(TextStyle::Body, FontId::new(sz, prop.clone()));
                s.text_styles
                    .insert(TextStyle::Button, FontId::new(sz, prop.clone()));
                s.text_styles
                    .insert(TextStyle::Small, FontId::new(sz * 0.85, prop.clone()));
                s.text_styles
                    .insert(TextStyle::Heading, FontId::new(sz * 1.4, prop));
                s.text_styles
                    .insert(TextStyle::Monospace, FontId::new(sz, FontFamily::Monospace));
            });
        }
    }

    /// Build egui's font set: the configured UI font first (if any), then egui's
    /// defaults, then a CJK fallback (so Japanese/Chinese/Korean render when a CJK
    /// font is installed).
    fn install_fonts(&self, ctx: &egui::Context) {
        ctx.set_fonts(self.font_definitions());
    }

    /// The font set this theme asks for, resolved once per process.
    ///
    /// Building it costs about twenty milliseconds — fontconfig lookups plus reading
    /// the font files off disk — and a tool that shows its overlay once pays that
    /// once. The switcher's daemon shows many, from the same theme, and would pay it
    /// at every one; egui then compares the result with what it already has and
    /// throws the copy away. So the answer is cached under the three fields it
    /// depends on, and a theme edit that changes any of them still rebuilds it.
    fn font_definitions(&self) -> egui::FontDefinitions {
        type Key = (Option<String>, Option<String>, Option<String>);
        static CACHE: Mutex<Option<(Key, egui::FontDefinitions)>> = Mutex::new(None);

        let key = (
            self.font_path.clone(),
            self.font.clone(),
            self.cjk_font.clone(),
        );
        let mut slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached, fonts)) = slot.as_ref()
            && *cached == key
        {
            return fonts.clone();
        }
        let fonts = self.build_fonts();
        *slot = Some((key, fonts.clone()));
        fonts
    }

    /// Resolve the configured families into an egui font set (the uncached half of
    /// [`Self::font_definitions`]).
    fn build_fonts(&self) -> egui::FontDefinitions {
        let mut fonts = egui::FontDefinitions::default();
        // `None` if libfontconfig.so.1 isn't loadable.
        let fc = fontconfig::Fontconfig::new();

        // Primary UI font: explicit file, or a family resolved via fontconfig.
        let primary = self
            .font_path
            .as_deref()
            .and_then(read_font_file)
            .or_else(|| {
                self.font
                    .as_deref()
                    .and_then(|family| find_font(fc.as_ref()?, family))
            });
        if let Some(data) = primary {
            fonts.font_data.insert("ui".into(), data.into());
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(fam)
                    .or_default()
                    .insert(0, "ui".into());
            }
        }

        // CJK fallback: configured family, else the first common one installed.
        let cjk = self
            .cjk_font
            .as_deref()
            .iter()
            .chain(CJK_FAMILIES)
            .find_map(|family| {
                let fc = fc.as_ref()?;
                // Without the separate `family_installed` check, every
                // `CJK_FAMILIES` probe would "find" a Latin font.
                family_installed(fc, family)?.then(|| find_font(fc, family))?
            });
        if let Some(data) = cjk {
            fonts.font_data.insert("cjk".into(), data.into());
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
        }

        fonts
    }
}

/// Common CJK font families to try when none is configured.
const CJK_FAMILIES: &[&str] = &[
    "Noto Sans CJK JP",
    "Noto Sans CJK SC",
    "Noto Sans CJK KR",
    "Source Han Sans",
    "Sarasa Gothic",
    "WenQuanYi Zen Hei",
];

fn read_font_file(path: impl AsRef<std::path::Path>) -> Option<egui::FontData> {
    let bytes = std::fs::read(path).ok()?;
    Some(egui::FontData::from_owned(bytes))
}

/// Loads the font that [`fontconfig`] determines to be the best match for the
/// given `family`. This will correctly resolve aliases but also fall back to
/// the system's default font if the requested family does not exist.
///
/// Note that [`fontconfig::Fontconfig::find`] falling back to another font
/// family can not be reliably detected as it returns only a single
/// locale-dependent `FC_FULLNAME` for the selected font, which doesn't have to
/// match the `FC_FAMILY` we queried the font with. If silent fallbacks are not
/// desired, first check a font's existence via [`family_installed`].
fn find_font(fc: &fontconfig::Fontconfig, family: &str) -> Option<egui::FontData> {
    let font = fc.find(family, None).ok()?;
    let mut data = read_font_file(&font.path)?;
    // fontconfig packs a named-instance ordinal (variable fonts) into the upper 16
    // bits of the index; egui wants the plain face number in the lower half.
    data.index = (font.index.unwrap_or(0) as u32) & 0xFFFF;
    Some(data)
}

/// Cheap check against the cache if a font family exists considering all
/// localized names of the family, ignoring casing and whitespace. `None` if it
/// was not possible to determine existence.
///
/// As the [`fontconfig::list_fonts`] used here returns matched families in
/// cache order, we can only use it to check if there is _some_ match, but not
/// to determine the best one; use [`find_font`] for the latter.
fn family_installed(fc: &fontconfig::Fontconfig, family: &str) -> Option<bool> {
    let mut pat = fontconfig::Pattern::new(fc).ok()?;
    pat.add_string(fontconfig::FC_FAMILY, &std::ffi::CString::new(family).ok()?)
        .ok()?;

    let mut objs = fontconfig::ObjectSet::new(fc).ok()?;
    objs.add(fontconfig::FC_FAMILY).ok()?;

    let fonts = fontconfig::list_fonts(&pat, Some(&objs)).ok()?;
    Some(fonts.iter().next().is_some())
}

fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("wlr-chooser").join("theme.toml"))
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa`.
fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().strip_prefix('#')?;
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(n(0)?, n(2)?, n(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(n(0)?, n(2)?, n(4)?, n(6)?)),
        3 => {
            let d = |i: usize| {
                let v = u8::from_str_radix(&h[i..i + 1], 16).ok()?;
                Some(v * 17)
            };
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}
