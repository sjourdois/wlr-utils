//! Internationalisation.
//!
//! With the `i18n` feature (on by default) UI strings are localised via Fluent
//! (`i18n-embed`): catalogs live in `i18n/<lang>/wlr_capture.ftl`, are embedded into
//! the binary, and the UI language is negotiated from the desktop locale, falling
//! back to English. Without the feature, [`tr!`](crate::tr) returns the English text
//! generated from the `en` catalog at build time (see `build.rs`) — no Fluent
//! dependency is pulled at all, which keeps minimal/headless builds lean.
//!
//! Use the [`tr!`](crate::tr) macro for message lookups; it works unchanged from any
//! crate in the workspace and across both build configurations.

#[cfg(feature = "i18n")]
mod fluent_impl {
    use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
    use i18n_embed::{DesktopLanguageRequester, LanguageLoader};
    use rust_embed::RustEmbed;
    use std::sync::LazyLock;

    #[derive(RustEmbed)]
    #[folder = "i18n/"]
    struct Localizations;

    /// The process-wide Fluent loader, preloaded with the fallback language.
    pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
        let loader = fluent_language_loader!();
        loader
            .load_fallback_language(&Localizations)
            .expect("fallback language must be present");
        // No bidirectional isolation marks around placeables (we render plain LTR text).
        loader.set_use_isolating(false);
        loader
    });

    /// Initialise localisation (call once at startup).
    ///
    /// The UI follows the desktop locale (`LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`),
    /// falling back to English; set `LANGUAGE` (e.g. `LANGUAGE=ja`) to override.
    pub fn init() {
        let requested = DesktopLanguageRequester::requested_languages();
        let _ = i18n_embed::select(&*LOADER, &Localizations, &requested);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every embedded catalog must parse and load (catches malformed `.ftl`).
        #[test]
        fn every_catalog_loads() {
            let loader = fluent_language_loader!();
            let langs = loader
                .available_languages(&Localizations)
                .expect("list languages");
            assert!(
                langs.len() >= 13,
                "expected ≥13 languages, got {}",
                langs.len()
            );
            for lang in langs {
                loader
                    .load_languages(&Localizations, std::slice::from_ref(&lang))
                    .unwrap_or_else(|e| panic!("catalog {lang} failed to load: {e}"));
            }
        }
    }
}

#[cfg(feature = "i18n")]
pub use fluent_impl::{LOADER, init};

#[cfg(not(feature = "i18n"))]
mod fallback_impl {
    // `fallback(id, args) -> String`, generated from the `en` catalog by `build.rs`.
    include!(concat!(env!("OUT_DIR"), "/i18n_fallback.rs"));

    /// No-op: there is no locale to negotiate without Fluent.
    pub fn init() {}
}

#[cfg(not(feature = "i18n"))]
pub use fallback_impl::{fallback, init};

/// Look up a UI message, optionally with `name = value` arguments.
///
/// With the `i18n` feature this is a runtime Fluent lookup against [`LOADER`]; without
/// it, the English text from the `en` catalog (generated at build time). Either way it
/// returns a `String` and is fully qualified through `$crate`, so it works from any
/// crate in the workspace without extra dependencies. Argument values only need to be
/// `Into<FluentValue>` (with `i18n`) / `Display` (without) — `String`, `&str`, integers.
#[cfg(feature = "i18n")]
#[macro_export]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::LOADER.get($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        // Values keep their own type; `get_args` accepts any `V: Into<FluentValue>`
        // (e.g. `String`, `&str`, integers). One map, so all args share a type.
        let mut args = ::std::collections::HashMap::new();
        $( args.insert(::std::stringify!($name), $value); )+
        $crate::i18n::LOADER.get_args($id, args)
    }};
}

/// English-only fallback variant (no `i18n` feature): expands to the generated
/// `fallback`, formatting each argument with `Display`.
#[cfg(not(feature = "i18n"))]
#[macro_export]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::fallback($id, &[])
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fallback(
            $id,
            &[ $( (::std::stringify!($name), ::std::format!("{}", $value)) ),+ ],
        )
    };
}
