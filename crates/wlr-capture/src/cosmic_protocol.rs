//! Client bindings for the COSMIC protocol extensions this crate needs.
//!
//! COSMIC ships no IPC socket and no command-line client: the only way to ask
//! cosmic-comp which window is active, and where it is, is `zcosmic_toplevel_info_v1`,
//! and the only way to focus one is `zcosmic_toplevel_manager_v1`. Their XML lives in
//! [pop-os/cosmic-protocols], published on crates.io as `cosmic-protocols` — but that
//! crate is GPL-3.0-only, and wlr-utils is MIT OR Apache-2.0, so the protocol files
//! themselves (permissively licensed, see their `<copyright>` headers) are vendored
//! under `protocols/` and generated here instead.
//!
//! `cosmic-workspace-unstable-v1` is generated only because deprecated requests and
//! events carry a `zcosmic_workspace_handle_v1` argument; nothing in the crate uses
//! workspaces.
//!
//! [pop-os/cosmic-protocols]: https://github.com/pop-os/cosmic-protocols

#![allow(
    dead_code,
    non_camel_case_types,
    unused_unsafe,
    unused_variables,
    non_upper_case_globals,
    non_snake_case,
    unused_imports,
    missing_docs,
    clippy::all,
    clippy::pedantic
)]

pub mod workspace {
    //! `cosmic-workspace-unstable-v1`.

    pub mod v1 {
        pub mod client {
            //! Client-side API of this protocol.
            use wayland_client;
            use wayland_client::protocol::*;

            pub mod __interfaces {
                use wayland_client::protocol::__interfaces::*;
                wayland_scanner::generate_interfaces!("protocols/cosmic-workspace-unstable-v1.xml");
            }
            use self::__interfaces::*;

            wayland_scanner::generate_client_code!("protocols/cosmic-workspace-unstable-v1.xml");
        }
    }
}

pub mod toplevel_info {
    //! `cosmic-toplevel-info-unstable-v1`.

    pub mod v1 {
        pub mod client {
            //! Client-side API of this protocol.
            use wayland_client;
            use wayland_client::protocol::*;
            use wayland_protocols::ext::foreign_toplevel_list::v1::client::*;
            use wayland_protocols::ext::workspace::v1::client::*;

            use crate::cosmic_protocol::workspace::v1::client::*;

            pub mod __interfaces {
                use wayland_client::protocol::__interfaces::*;
                use wayland_protocols::ext::foreign_toplevel_list::v1::client::__interfaces::*;
                use wayland_protocols::ext::workspace::v1::client::__interfaces::*;

                use crate::cosmic_protocol::workspace::v1::client::__interfaces::*;

                wayland_scanner::generate_interfaces!(
                    "protocols/cosmic-toplevel-info-unstable-v1.xml"
                );
            }
            use self::__interfaces::*;

            wayland_scanner::generate_client_code!(
                "protocols/cosmic-toplevel-info-unstable-v1.xml"
            );
        }
    }
}

pub mod toplevel_management {
    //! `cosmic-toplevel-management-unstable-v1`.

    pub mod v1 {
        pub mod client {
            //! Client-side API of this protocol.
            use wayland_client;
            use wayland_client::protocol::*;
            use wayland_protocols::ext::workspace::v1::client::*;

            use crate::cosmic_protocol::toplevel_info::v1::client::*;
            use crate::cosmic_protocol::workspace::v1::client::*;

            pub mod __interfaces {
                use wayland_client::protocol::__interfaces::*;
                use wayland_protocols::ext::workspace::v1::client::__interfaces::*;

                use crate::cosmic_protocol::toplevel_info::v1::client::__interfaces::*;
                use crate::cosmic_protocol::workspace::v1::client::__interfaces::*;

                wayland_scanner::generate_interfaces!(
                    "protocols/cosmic-toplevel-management-unstable-v1.xml"
                );
            }
            use self::__interfaces::*;

            wayland_scanner::generate_client_code!(
                "protocols/cosmic-toplevel-management-unstable-v1.xml"
            );
        }
    }
}
