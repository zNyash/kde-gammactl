//! All KDE-specific Wayland code lives under this module.
//!
//! KDE's output-management protocol is an internal, unstable API; keeping it
//! isolated here means a Plasma change only touches this directory.

mod discovery;
mod profile;

pub use discovery::{connect, Session};
pub use profile::{apply_icc_profile, apply_profile};

/// Generated protocol bindings (see `protocols/*.xml`).
pub mod kde {
    pub mod device {
        #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
        #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
        #![allow(missing_docs, clippy::all)]

        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocols/kde-output-device-v2.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("protocols/kde-output-device-v2.xml");
    }

    pub mod management {
        #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
        #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
        #![allow(missing_docs, clippy::all)]

        use super::device::*;
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use super::super::device::__interfaces::*;
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocols/kde-output-management-v2.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("protocols/kde-output-management-v2.xml");
    }

    pub use device::*;
    pub use management::*;
}

/// Display metadata collected from `kde_output_device_v2` events.
#[derive(Debug, Clone, Default)]
pub struct OutputInfo {
    pub name: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub uuid: Option<String>,
    pub enabled: bool,
    pub icc_profile_path: Option<String>,
    /// Raw `color_profile_source` value (0 = sRGB, 1 = ICC, 2 = EDID).
    pub color_profile_source: Option<u32>,
}

impl OutputInfo {
    pub fn label(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.uuid.clone())
            .unwrap_or_else(|| "<unnamed>".into())
    }

    pub fn source_label(&self) -> &'static str {
        match self.color_profile_source {
            Some(0) => "sRGB",
            Some(1) => "ICC",
            Some(2) => "EDID",
            _ => "unknown",
        }
    }
}
