//! Application data directory and persisted state (Phase 2 layout):
//!
//! ```text
//! ~/.local/share/kde-gammactl/
//! ├── profiles/
//! │   ├── <output>-baseline.icc     immutable snapshot of the original profile
//! │   └── generated/                profiles we generated (old ones are pruned)
//! └── state.json
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputState {
    /// Profile path KWin reported before we first touched this display.
    pub original_path: Option<String>,
    /// Colour-profile source before we touched it (0 sRGB, 1 ICC, 2 EDID).
    pub original_source: u32,
    /// Snapshot of the original profile used as the immutable baseline;
    /// `None` means the built-in sRGB baseline.
    pub baseline: Option<String>,
    /// Currently active generated profile.
    pub generated: Option<String>,
    /// Formula used to generate the active profile.
    #[serde(default = "default_curve")]
    pub curve: String,
    pub contrast: f64,
    pub brightness: f64,
    pub gamma: f64,
}

fn default_curve() -> String {
    "classic".into()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub outputs: BTreeMap<String, OutputState>,
}

pub struct Paths {
    pub profiles: PathBuf,
    pub generated: PathBuf,
    pub state_file: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Paths, Error> {
        let base = match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            Some(v) => PathBuf::from(v),
            None => PathBuf::from(
                std::env::var_os("HOME")
                    .ok_or_else(|| Error::Io("neither XDG_DATA_HOME nor HOME is set".into()))?,
            )
            .join(".local/share"),
        };
        let root = base.join("kde-gammactl");
        let generated = root.join("profiles/generated");
        fs::create_dir_all(&generated)
            .map_err(|e| Error::Io(format!("cannot create {}: {e}", generated.display())))?;
        // Canonicalise so "is this path ours?" comparisons survive symlinks.
        let root = root.canonicalize()?;
        Ok(Paths {
            profiles: root.join("profiles"),
            generated: root.join("profiles/generated"),
            state_file: root.join("state.json"),
        })
    }
}

impl State {
    pub fn load(paths: &Paths) -> Result<State, Error> {
        match fs::read_to_string(&paths.state_file) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                Error::Io(format!(
                    "{} is corrupt ({e}); delete it to start fresh",
                    paths.state_file.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<(), Error> {
        let text = serde_json::to_string_pretty(self).map_err(|e| Error::Io(e.to_string()))?;
        write_atomic(&paths.state_file, text.as_bytes())
    }
}

/// Write `data` to a sibling temp file, then rename over `dest`.
pub fn write_atomic(dest: &Path, data: &[u8]) -> Result<(), Error> {
    let tmp = dest.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::Io(format!("cannot write {}: {e}", dest.display()))
    })
}

/// Filesystem-safe form of an output name or UUID.
pub fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_output_state_defaults_to_classic_curve() {
        let state: OutputState = serde_json::from_str(
            r#"{"original_path":null,"original_source":0,"baseline":null,"generated":null,"contrast":1.0,"brightness":1.0,"gamma":1.0}"#,
        )
        .unwrap();
        assert_eq!(state.curve, "classic");
    }
}
