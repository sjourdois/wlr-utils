//! Build script for `wlr-capture`.
//!
//! Generates the English fallback used by `tr!` when the `i18n` feature is off, so a
//! build without Fluent still has the UI text. Parses the `en` Fluent catalog (simple
//! one-line `key = value` entries) into a `fallback(id, args) -> String` function with
//! a `match` over the message ids, substituting `{ $name }` placeholders from `args`.
//! Also re-runs whenever any catalog changes (so embedded translations refresh).

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=i18n");

    let catalog = "i18n/en/wlr_capture.ftl";
    let src = fs::read_to_string(catalog).expect("read en catalog");
    let mut arms = String::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // `{key:?}`/`{value:?}` emit valid, escaped Rust string literals.
        writeln!(arms, "        {:?} => {:?},", key.trim(), value.trim()).unwrap();
    }

    let code = format!(
        "/// English fallback text generated from the `en` Fluent catalog.\n\
         pub fn fallback(id: &str, args: &[(&'static str, String)]) -> String {{\n\
         \x20   let template: &str = match id {{\n\
         {arms}\
         \x20       _ => id,\n\
         \x20   }};\n\
         \x20   let mut out = template.to_string();\n\
         \x20   for (name, value) in args {{\n\
         \x20       out = out.replace(&format!(\"{{{{ ${{name}} }}}}\"), value);\n\
         \x20   }}\n\
         \x20   out\n\
         }}\n"
    );

    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("i18n_fallback.rs");
    fs::write(out, code).expect("write i18n_fallback.rs");
}
