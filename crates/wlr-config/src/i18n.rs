//! wlr-config's localised messages. Catalog: `i18n/<lang>/wlr_config.ftl`; loader plumbing
//! in [`wlr_i18n`]. Use the crate-local [`tr!`] macro for lookups.

#[cfg(feature = "i18n")]
mod imp {
    use rust_embed::RustEmbed;
    use std::sync::LazyLock;
    use wlr_i18n::FluentLanguageLoader;

    #[derive(RustEmbed)]
    #[folder = "i18n/"]
    struct Localizations;

    /// This crate's process-wide Fluent loader, set to the desktop locale on first use:
    /// a library has no startup of its own to negotiate it at.
    pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
        let loader = wlr_i18n::build_loader("wlr_config", &Localizations);
        wlr_i18n::select(&loader, &Localizations);
        loader
    });
}

#[cfg(feature = "i18n")]
pub(crate) use imp::LOADER;

#[cfg(not(feature = "i18n"))]
mod imp {
    // `fallback(id, args) -> String`, generated from the `en` catalog by `build.rs`.
    include!(concat!(env!("OUT_DIR"), "/i18n_fallback.rs"));
}

#[cfg(not(feature = "i18n"))]
pub(crate) use imp::fallback;

/// Look up a message, optionally with `name = value` arguments.
#[cfg(feature = "i18n")]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::LOADER.get($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = ::std::collections::HashMap::new();
        $( args.insert(::std::stringify!($name), $value.to_string()); )+
        $crate::i18n::LOADER.get_args($id, args)
    }};
}

/// English-only fallback variant (no `i18n` feature).
#[cfg(not(feature = "i18n"))]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::fallback($id, &[])
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fallback(
            $id,
            &[ $( (::std::stringify!($name), $value.to_string()) ),+ ],
        )
    };
}

pub(crate) use tr;
