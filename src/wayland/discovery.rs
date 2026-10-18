//! Connecting to KWin and enumerating displays.

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::{
    event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
};

use super::kde::kde_output_configuration_v2::{self, KdeOutputConfigurationV2};
use super::kde::kde_output_device_mode_v2::{self, KdeOutputDeviceModeV2};
use super::kde::kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2};
use super::kde::kde_output_device_v2::{self, KdeOutputDeviceV2};
use super::kde::kde_output_management_v2::{self, KdeOutputManagementV2};
use super::OutputInfo;
use crate::error::Error;

/// Oldest/newest versions we are willing to bind.
/// Management >= 6 is required for `set_icc_profile_path`.
const MANAGEMENT_VERSIONS: std::ops::RangeInclusive<u32> = 6..=22;
/// The registry global only exists from Plasma versions that speak v21+.
const REGISTRY_VERSIONS: std::ops::RangeInclusive<u32> = 21..=26;
const DEVICE_MAX_VERSION: u32 = 26;

pub struct Output {
    pub device: KdeOutputDeviceV2,
    pub info: OutputInfo,
    pub removed: bool,
}

#[derive(Default)]
pub struct State {
    pub outputs: Vec<Output>,
    /// Set by `applied` (Ok) / `failed` (Err) on the configuration object.
    pub apply_result: Option<Result<(), String>>,
    pub failure_reason: Option<String>,
}

/// A live connection to KWin with every display enumerated.
pub struct Session {
    #[allow(dead_code)] // keeps the connection alive
    pub conn: Connection,
    pub queue: EventQueue<State>,
    pub qh: QueueHandle<State>,
    pub state: State,
    pub management: KdeOutputManagementV2,
}

pub fn connect() -> Result<Session, Error> {
    let conn = Connection::connect_to_env().map_err(|e| Error::Connect(e.to_string()))?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&conn).map_err(|e| Error::Protocol(e.to_string()))?;
    let qh = queue.handle();
    let mut state = State::default();

    let management = globals
        .bind::<KdeOutputManagementV2, _, _>(&qh, MANAGEMENT_VERSIONS, ())
        .map_err(|e| {
            Error::ProtocolUnavailable(format!(
                "kde_output_management_v2 (version >= 6) is not available: {e}. \
                 This tool needs KWin on Plasma Wayland."
            ))
        })?;

    // Preferred path: the output-device registry (newer Plasma).
    let have_registry = globals
        .bind::<KdeOutputDeviceRegistryV2, _, _>(&qh, REGISTRY_VERSIONS, ())
        .is_ok();

    // Fallback path: older Plasma announces each kde_output_device_v2 as a
    // plain wl_registry global.
    if !have_registry {
        let list = globals.contents().clone_list();
        for g in list
            .iter()
            .filter(|g| g.interface == KdeOutputDeviceV2::interface().name)
        {
            let device = globals.registry().bind::<KdeOutputDeviceV2, _, _>(
                g.name,
                g.version.min(DEVICE_MAX_VERSION),
                &qh,
                (),
            );
            state.outputs.push(Output {
                device,
                info: OutputInfo::default(),
                removed: false,
            });
        }
        if state.outputs.is_empty() {
            return Err(Error::ProtocolUnavailable(
                "neither kde_output_device_registry_v2 nor kde_output_device_v2 is advertised \
                 by the compositor"
                    .into(),
            ));
        }
    }

    // First roundtrip delivers the output announcements, the second one
    // delivers each output's property events up to `done`.
    for _ in 0..2 {
        queue
            .roundtrip(&mut state)
            .map_err(|e| Error::Protocol(e.to_string()))?;
    }

    Ok(Session {
        conn,
        queue,
        qh,
        state,
        management,
    })
}

impl Session {
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.state.outputs.iter().filter(|o| !o.removed)
    }

    pub fn roundtrip(&mut self) -> Result<(), Error> {
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|e| Error::Protocol(e.to_string()))
    }

    /// Resolve `--output`: match by connector name or UUID, or auto-select
    /// when exactly one display is connected.
    pub fn find_output(&self, wanted: Option<&str>) -> Result<&Output, Error> {
        let labels = || self.outputs().map(|o| o.info.label()).collect::<Vec<_>>();
        match wanted {
            Some(w) => self
                .outputs()
                .find(|o| {
                    o.info.name.as_deref() == Some(w) || o.info.uuid.as_deref() == Some(w)
                })
                .ok_or_else(|| Error::OutputNotFound {
                    wanted: w.to_string(),
                    available: labels(),
                }),
            None => {
                let mut it = self.outputs();
                match (it.next(), it.next()) {
                    (Some(o), None) => Ok(o),
                    _ => Err(Error::AmbiguousOutput(labels())),
                }
            }
        }
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ---- Dispatch implementations ------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Hotplug is irrelevant for a one-shot CLI.
    }
}

impl Dispatch<KdeOutputManagementV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &KdeOutputManagementV2,
        _: kde_output_management_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<KdeOutputDeviceRegistryV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &KdeOutputDeviceRegistryV2,
        event: kde_output_device_registry_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let kde_output_device_registry_v2::Event::Output { output } = event {
            state.outputs.push(Output {
                device: output,
                info: OutputInfo::default(),
                removed: false,
            });
        }
    }

    event_created_child!(State, KdeOutputDeviceRegistryV2, [
        kde_output_device_registry_v2::EVT_OUTPUT_OPCODE => (KdeOutputDeviceV2, ()),
    ]);
}

impl Dispatch<KdeOutputDeviceV2, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &KdeOutputDeviceV2,
        event: kde_output_device_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use kde_output_device_v2::Event;

        let Some(out) = state.outputs.iter_mut().find(|o| &o.device == proxy) else {
            return;
        };

        match event {
            Event::Geometry { make, model, .. } => {
                out.info.make = non_empty(make);
                out.info.model = non_empty(model);
            }
            Event::Name { name } => out.info.name = non_empty(name),
            Event::Uuid { uuid } => out.info.uuid = non_empty(uuid),
            Event::SerialNumber { serialNumber } => out.info.serial = non_empty(serialNumber),
            Event::Enabled { enabled } => out.info.enabled = enabled != 0,
            Event::IccProfilePath { profile_path } => {
                out.info.icc_profile_path = non_empty(profile_path)
            }
            Event::ColorProfileSource { source } => {
                out.info.color_profile_source = match source {
                    WEnum::Value(v) => Some(v as u32),
                    WEnum::Unknown(v) => Some(v),
                }
            }
            Event::Removed => {
                out.removed = true;
                if proxy.version() >= 21 {
                    proxy.release();
                }
            }
            _ => {}
        }
    }

    event_created_child!(State, KdeOutputDeviceV2, [
        kde_output_device_v2::EVT_MODE_OPCODE => (KdeOutputDeviceModeV2, ()),
    ]);
}

impl Dispatch<KdeOutputDeviceModeV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &KdeOutputDeviceModeV2,
        _: kde_output_device_mode_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<KdeOutputConfigurationV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &KdeOutputConfigurationV2,
        event: kde_output_configuration_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use kde_output_configuration_v2::Event;
        match event {
            Event::Applied => state.apply_result = Some(Ok(())),
            Event::FailureReason { reason } => state.failure_reason = Some(reason),
            Event::Failed => {
                let why = state
                    .failure_reason
                    .take()
                    .unwrap_or_else(|| "no reason given by the compositor".into());
                state.apply_result = Some(Err(why));
            }
        }
    }
}
