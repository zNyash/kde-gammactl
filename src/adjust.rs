//! `set` and `reset`: generate a profile from the immutable baseline and
//! activate it (or restore the original). Ties curves + icc + wayland + state.

use std::fs;
use std::path::{Path, PathBuf};

use crate::curves::{self, Adjustments, CurveKind};
use crate::error::Error;
use crate::icc::{self, Profile};
use crate::state::{safe_name, write_atomic, OutputState, Paths, State};
use crate::wayland::{self, OutputInfo, Session};

/// Entries per channel in the generated `vcgt` table.
pub const LUT_ENTRIES: usize = 256;

const SOURCE_ICC: u32 = 1;

pub struct Outcome {
    pub label: String,
    pub detail: String,
}

fn fnv1a(data: &[u8]) -> u32 {
    data.iter().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)
    })
}

/// Displays an adjustment applies to: the one named, or every enabled one.
fn targets(session: &Session, wanted: Option<&str>) -> Result<Vec<OutputInfo>, Error> {
    if wanted.is_some() {
        return Ok(vec![session.find_output(wanted)?.info.clone()]);
    }
    let list: Vec<OutputInfo> = session
        .outputs()
        .filter(|o| o.info.enabled)
        .map(|o| o.info.clone())
        .collect();
    if list.is_empty() {
        return Err(Error::Invalid("KWin reports no enabled displays".into()));
    }
    Ok(list)
}

fn state_key(info: &OutputInfo) -> String {
    info.uuid.clone().unwrap_or_else(|| info.label())
}

/// Remove every generated profile of this display except `keep`.
fn prune_generated(paths: &Paths, safe: &str, keep: Option<&Path>) {
    let prefix = format!("{safe}_");
    let Ok(rd) = fs::read_dir(&paths.generated) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && Some(p.as_path()) != keep {
            let _ = fs::remove_file(p);
        }
    }
}

/// Decide the original profile + baseline for a display.
fn establish_entry(
    paths: &Paths,
    key: &str,
    info: &OutputInfo,
    ours: bool,
    existing: Option<&OutputState>,
) -> Result<OutputState, Error> {
    if let (true, Some(e)) = (ours, existing) {
        return Ok(e.clone()); // re-adjusting: baseline stays what it was
    }

    // First contact (or the user picked another profile in System Settings
    // since our last run): whatever KWin reports now is the new original.
    let (original_path, original_source) = if ours {
        // Our profile is active but the state file is gone; the true original
        // is unknown, so fall back to sRGB for a later reset.
        (None, 0)
    } else {
        (
            info.icc_profile_path.clone(),
            info.color_profile_source.unwrap_or(0),
        )
    };

    let mut baseline = None;
    if original_source == SOURCE_ICC {
        if let Some(orig) = &original_path {
            let bytes = fs::read(orig).map_err(|e| {
                Error::BadProfile(format!("cannot read current profile {orig}: {e}"))
            })?;
            Profile::parse(&bytes).map_err(|e| {
                Error::BadProfile(format!(
                    "current profile {orig} cannot be used as a baseline: {e}"
                ))
            })?;
            let snap = paths
                .profiles
                .join(format!("{}-baseline.icc", safe_name(key)));
            write_atomic(&snap, &bytes)?;
            baseline = Some(snap.to_string_lossy().into_owned());
        }
    }

    Ok(OutputState {
        original_path,
        original_source,
        baseline,
        generated: None,
        curve: "classic".into(),
        contrast: 1.0,
        brightness: 1.0,
        gamma: 1.0,
    })
}

pub fn set(
    session: &mut Session,
    wanted: Option<&str>,
    kind: CurveKind,
    adj: &Adjustments,
) -> Result<Vec<Outcome>, Error> {
    let curve = curves::generate(kind, adj, LUT_ENTRIES)?;
    let paths = Paths::new()?;
    let mut state = State::load(&paths)?;
    let mut results = Vec::new();

    for info in targets(session, wanted)? {
        let label = info.label();
        let key = state_key(&info);
        let safe = safe_name(&key);

        let current = info.icc_profile_path.as_deref().map(PathBuf::from);
        let ours = current
            .as_ref()
            .is_some_and(|p| p.starts_with(&paths.generated));

        let mut entry = establish_entry(&paths, &key, &info, ours, state.outputs.get(&key))?;

        // The baseline is always read from the immutable snapshot, never from
        // a previously generated profile.
        let baseline_bytes = match &entry.baseline {
            Some(p) => Some(fs::read(p).map_err(|e| {
                Error::BadProfile(format!("cannot read baseline snapshot {p}: {e}"))
            })?),
            None => None,
        };
        let bytes = icc::build_profile(baseline_bytes.as_deref(), &curve)?;

        let file = paths.generated.join(format!(
            "{safe}_{}_c{:.3}_b{:.3}_g{:.3}_{:08x}.icc",
            kind.as_str(),
            adj.contrast,
            adj.brightness,
            adj.gamma,
            fnv1a(&bytes)
        ));
        write_atomic(&file, &bytes)?;

        match wayland::apply_icc_profile(session, Some(&label), &file) {
            Ok(applied) => {
                entry.generated = Some(file.to_string_lossy().into_owned());
                entry.curve = kind.as_str().into();
                entry.contrast = adj.contrast;
                entry.brightness = adj.brightness;
                entry.gamma = adj.gamma;
                state.outputs.insert(key, entry);
                state.save(&paths)?;
                prune_generated(&paths, &safe, Some(&file));
                results.push(Outcome {
                    label,
                    detail: applied
                        .icc_profile_path
                        .unwrap_or_else(|| file.to_string_lossy().into_owned()),
                });
            }
            Err(e) => {
                // Don't leave a rejected profile behind (unless it is the one
                // already active).
                if current.as_deref() != Some(file.as_path()) {
                    let _ = fs::remove_file(&file);
                }
                return Err(e);
            }
        }
    }
    Ok(results)
}

pub fn reset(session: &mut Session, wanted: Option<&str>) -> Result<Vec<Outcome>, Error> {
    let paths = Paths::new()?;
    let mut state = State::load(&paths)?;
    let mut results = Vec::new();

    for info in targets(session, wanted)? {
        let label = info.label();
        let key = state_key(&info);
        let safe = safe_name(&key);

        let Some(entry) = state.outputs.get(&key).cloned() else {
            results.push(Outcome {
                label,
                detail: "nothing to reset (no adjustment recorded)".into(),
            });
            continue;
        };

        let original = entry.original_path.as_deref().map(Path::new);
        let detail = match original {
            Some(p) if entry.original_source == SOURCE_ICC && p.is_file() => {
                wayland::apply_icc_profile(session, Some(&label), p)?;
                format!("restored {}", p.display())
            }
            _ => {
                wayland::apply_profile(session, Some(&label), None, entry.original_source)?;
                format!(
                    "restored colour-profile source {}",
                    match entry.original_source {
                        1 => "ICC (original file missing, see warning)",
                        2 => "EDID",
                        _ => "sRGB",
                    }
                )
            }
        };

        prune_generated(&paths, &safe, None);
        if let Some(b) = &entry.baseline {
            let _ = fs::remove_file(b);
        }
        state.outputs.remove(&key);
        state.save(&paths)?;
        results.push(Outcome { label, detail });
    }
    Ok(results)
}

/// Write a generated profile to `out` without touching the compositor.
pub fn generate_to_file(
    kind: CurveKind,
    baseline: Option<&Path>,
    adj: &Adjustments,
    out: &Path,
) -> Result<(), Error> {
    let curve = curves::generate(kind, adj, LUT_ENTRIES)?;
    let base = match baseline {
        Some(p) => {
            Some(fs::read(p).map_err(|e| Error::BadProfile(format!("{}: {e}", p.display())))?)
        }
        None => None,
    };
    let bytes = icc::build_profile(base.as_deref(), &curve)?;
    write_atomic(out, &bytes)
}
