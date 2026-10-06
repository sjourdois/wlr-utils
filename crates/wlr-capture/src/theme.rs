//! Applying a [`Theme`] to egui: its palette and its fonts. The theme itself — its keys,
//! its defaults and where it is read from — belongs to `wlr-config`.
//!
//! Rendering CJK text (Japanese/Chinese/Korean) needs a CJK font installed; one
//! is auto-detected and used as a fallback.

use std::sync::Mutex;

pub use wlr_config::Theme;

/// What a theme does to an egui context.
pub trait ApplyTheme {
    /// Apply the palette to egui's global visuals (panels, widgets, selection…), and the
    /// fonts and text size to its style.
    fn apply(&self, ctx: &egui::Context);
}

impl ApplyTheme for Theme {
    fn apply(&self, ctx: &egui::Context) {
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

        // The configured UI font first (if any), then egui's defaults, then a CJK
        // fallback (so Japanese/Chinese/Korean render when a CJK font is installed).
        ctx.set_fonts(font_definitions(self));
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
}

/// The font set `theme` asks for, resolved once per process.
///
/// Building it costs about twenty milliseconds — fontconfig lookups plus reading
/// the font files off disk — and a tool that shows its overlay once pays that
/// once. The switcher's daemon shows many, from the same theme, and would pay it
/// at every one; egui then compares the result with what it already has and
/// throws the copy away. So the answer is cached under the three fields it
/// depends on, and a theme edit that changes any of them still rebuilds it.
fn font_definitions(theme: &Theme) -> egui::FontDefinitions {
    type Key = (Option<String>, Option<String>, Option<String>);
    static CACHE: Mutex<Option<(Key, egui::FontDefinitions)>> = Mutex::new(None);

    let key = (
        theme.font_path.clone(),
        theme.font.clone(),
        theme.cjk_font.clone(),
    );
    let mut slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, fonts)) = slot.as_ref()
        && *cached == key
    {
        return fonts.clone();
    }
    let fonts = build_fonts(theme);
    *slot = Some((key, fonts.clone()));
    fonts
}

/// Resolve the configured families into an egui font set (the uncached half of
/// [`font_definitions`]).
fn build_fonts(theme: &Theme) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    // `None` if libfontconfig.so.1 isn't loadable.
    let fc = fontconfig::Fontconfig::new();

    // Primary UI font: explicit file, or a family resolved via fontconfig.
    let primary = theme
        .font_path
        .as_deref()
        .and_then(read_font_file)
        .or_else(|| {
            theme
                .font
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
    let cjk = theme
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
