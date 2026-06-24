//! wlr-pip — deprecated.
//!
//! The floating live mirror moved into `wlr-peek` (the screen-inspection tool):
//!   - `wlr-peek mirror <ID>`            — window picture-in-picture (what wlr-pip did)
//!   - `wlr-peek mirror -g "X,Y WxH"`     — live magnifier of a screen region
//!
//! This binary is a thin pointer kept only because the crate name is published; it
//! does no mirroring itself.

fn main() {
    eprintln!(
        "wlr-pip is deprecated and now part of wlr-peek.\n\
         Use:  wlr-peek mirror <ID>            (window picture-in-picture)\n\
         or:   wlr-peek mirror -g \"X,Y WxH\"   (live region magnifier)"
    );
    std::process::exit(1);
}
