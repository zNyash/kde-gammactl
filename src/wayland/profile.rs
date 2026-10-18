//! Assigning an ICC profile path to a display and waiting for KWin's verdict.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use wayland_client::Proxy;

use super::discovery::Session;
use super::kde::kde_output_configuration_v2::ColorProfileSource;
use super::OutputInfo;
use crate::error::Error;

/// How long we wait for `applied` / `failed` before giving up.
const APPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve and sanity-check a profile path. KWin reads the file itself, so the
/// path we send must be absolute. Full ICC validation arrives in Phase 2; here
/// we only refuse obviously wrong input (missing file, no `acsp` signature).
pub fn check_profile(path: &Path) -> Result<PathBuf, Error> {
    let abs = path
        .canonicalize()
        .map_err(|e| Error::BadProfile(format!("{}: {e}", path.display())))?;
    if !abs.is_file() {
        return Err(Error::BadProfile(format!("{} is not a file", abs.display())));
    }
    let mut header = [0u8; 40];
    File::open(&abs)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|e| Error::BadProfile(format!("cannot read {}: {e}", abs.display())))?;
    if &header[36..40] != b"acsp" {
        return Err(Error::BadProfile(format!(
            "{} does not look like an ICC profile (missing 'acsp' signature)",
            abs.display()
        )));
    }
    Ok(abs)
}

/// Assign `profile` as the display's ICC profile (and switch its colour-profile
/// source to ICC), then wait for KWin's verdict.
pub fn apply_icc_profile(
    session: &mut Session,
    wanted: Option<&str>,
    profile: &Path,
) -> Result<OutputInfo, Error> {
    let abs = check_profile(profile)?;
    apply_profile(session, wanted, Some(&abs), SOURCE_ICC)
}

/// `color_profile_source` values: 0 = sRGB, 1 = ICC, 2 = EDID.
pub const SOURCE_ICC: u32 = 1;

/// Low-level apply: optionally set the ICC path, and set the colour-profile
/// source. Only the selected display is mentioned in the configuration, so no
/// other output setting is touched. Returns the display's refreshed metadata.
pub fn apply_profile(
    session: &mut Session,
    wanted: Option<&str>,
    path: Option<&Path>,
    source: u32,
) -> Result<OutputInfo, Error> {
    let abs_str = match path {
        Some(p) => Some(
            p.to_str()
                .ok_or_else(|| Error::BadProfile("profile path is not valid UTF-8".into()))?
                .to_owned(),
        ),
        None => None,
    };

    let device = session.find_output(wanted)?.device.clone();

    session.state.apply_result = None;
    session.state.failure_reason = None;

    let config = session.management.create_configuration(&session.qh, ());
    if let Some(p) = abs_str {
        config.set_icc_profile_path(&device, p);
    }
    // KWin ignores the path unless the colour-profile source is ICC (this is
    // what System Settings does when you pick a file).
    if config.version() >= 8 {
        let src = ColorProfileSource::try_from(source)
            .map_err(|_| Error::Protocol(format!("unknown colour-profile source {source}")))?;
        config.set_color_profile_source(&device, src);
    } else if source != SOURCE_ICC {
        config.destroy();
        return Err(Error::ProtocolUnavailable(
            "this Plasma version cannot switch the colour-profile source (needs protocol v8)"
                .into(),
        ));
    }
    config.apply();

    // CLI-only watchdog: if KWin never answers, don't hang forever.
    let answered = Arc::new(AtomicBool::new(false));
    {
        let answered = Arc::clone(&answered);
        std::thread::spawn(move || {
            std::thread::sleep(APPLY_TIMEOUT);
            if !answered.load(Ordering::SeqCst) {
                eprintln!("error: timed out waiting for KWin to answer");
                std::process::exit(1);
            }
        });
    }

    while session.state.apply_result.is_none() {
        session
            .queue
            .blocking_dispatch(&mut session.state)
            .map_err(|e| Error::Protocol(e.to_string()))?;
    }
    answered.store(true, Ordering::SeqCst);
    config.destroy();

    if let Some(Err(why)) = session.state.apply_result.take() {
        return Err(Error::Rejected(why));
    }

    // Let KWin report the new state of the output.
    session.roundtrip()?;
    session.roundtrip()?;

    let info = session
        .state
        .outputs
        .iter()
        .find(|o| o.device == device)
        .map(|o| o.info.clone())
        .ok_or_else(|| Error::Protocol("display disappeared after applying".into()))?;
    Ok(info)
}
