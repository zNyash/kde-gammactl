//! RGB calibration-curve engine (Phase 3).
//!
//! Pure functions, no I/O. Supports the original `wl-gammactl` formula and
//! Gamma-Manager's mid-grey-pivot formula:
//!
//! ```text
//! x = i / (N - 1)
//! y = clamp(contrast * x^(1 / gamma) + (brightness - 1), 0, 1)
//! entry = round(y * 65535)
//! ```
//!
//! The same value is used for the red, green and blue channels.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CurveKind {
    Classic,
    Midgray,
}

impl Default for CurveKind {
    fn default() -> Self {
        CurveKind::Classic
    }
}

impl CurveKind {
    pub fn neutral(self) -> Adjustments {
        match self {
            CurveKind::Classic => Adjustments::default(),
            CurveKind::Midgray => Adjustments {
                contrast: 1.0,
                brightness: 0.0,
                gamma: 1.0,
            },
        }
    }

    pub fn validate(self, adj: &Adjustments) -> Result<(), CurveError> {
        adj.validate()?;
        if self == CurveKind::Midgray {
            for (name, value, min, max) in [
                ("gamma", adj.gamma, 0.3, 4.4),
                ("contrast", adj.contrast, 0.1, 100.0),
                ("brightness", adj.brightness, -1.0, 1.0),
            ] {
                if value < min || value > max {
                    return Err(CurveError::OutOfRange { name, min, max });
                }
            }
        }
        Ok(())
    }

    pub fn apply(self, adj: &Adjustments, x: f64) -> f64 {
        match self {
            CurveKind::Classic => {
                (adj.contrast * x.powf(1.0 / adj.gamma) + (adj.brightness - 1.0)).clamp(0.0, 1.0)
            }
            CurveKind::Midgray => {
                const PIVOT: f64 = 127.0 / 255.0;
                let contrasted = adj.contrast * (x - PIVOT) + PIVOT;
                if adj.gamma != 1.0 && contrasted < 0.0 {
                    0.0
                } else {
                    let adjusted = if adj.gamma == 1.0 {
                        contrasted
                    } else {
                        contrasted.powf(1.0 / adj.gamma)
                    };
                    (adjusted + adj.brightness * 128.0 / 255.0).clamp(0.0, 1.0)
                }
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CurveKind::Classic => "classic",
            CurveKind::Midgray => "midgray",
        }
    }
}

/// User-facing adjustment parameters. `Default` is the neutral (identity) setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjustments {
    pub contrast: f64,
    pub brightness: f64,
    pub gamma: f64,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            contrast: 1.0,
            brightness: 1.0,
            gamma: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CurveError {
    /// A parameter was NaN or infinite.
    NonFinite(&'static str),
    /// Gamma must be strictly positive.
    InvalidGamma(f64),
    /// A parameter is outside the selected curve's supported range.
    OutOfRange {
        name: &'static str,
        min: f64,
        max: f64,
    },
    /// A curve needs at least two entries (`N - 1` is a divisor).
    InvalidSize(usize),
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurveError::NonFinite(name) => write!(f, "{name} must be a finite number"),
            CurveError::InvalidGamma(g) => write!(f, "gamma must be greater than 0 (got {g})"),
            CurveError::OutOfRange { name, min, max } => {
                write!(f, "{name} must be between {min} and {max}")
            }
            CurveError::InvalidSize(n) => {
                write!(f, "a curve needs at least 2 entries (got {n})")
            }
        }
    }
}

impl std::error::Error for CurveError {}

/// Three per-channel lookup tables of identical length, 16-bit entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Curve {
    pub red: Vec<u16>,
    pub green: Vec<u16>,
    pub blue: Vec<u16>,
}

impl Curve {
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.red.len()
    }
}

impl Adjustments {
    pub fn new(contrast: f64, brightness: f64, gamma: f64) -> Result<Self, CurveError> {
        let a = Self {
            contrast,
            brightness,
            gamma,
        };
        a.validate()?;
        Ok(a)
    }

    /// Require finite values and `gamma > 0`.
    pub fn validate(&self) -> Result<(), CurveError> {
        if !self.contrast.is_finite() {
            return Err(CurveError::NonFinite("contrast"));
        }
        if !self.brightness.is_finite() {
            return Err(CurveError::NonFinite("brightness"));
        }
        if !self.gamma.is_finite() {
            return Err(CurveError::NonFinite("gamma"));
        }
        if self.gamma <= 0.0 {
            return Err(CurveError::InvalidGamma(self.gamma));
        }
        Ok(())
    }
}

/// Generate a calibration curve with `entry_count` entries per channel.
pub fn generate(
    kind: CurveKind,
    adj: &Adjustments,
    entry_count: usize,
) -> Result<Curve, CurveError> {
    kind.validate(adj)?;
    if entry_count < 2 {
        return Err(CurveError::InvalidSize(entry_count));
    }

    let last = (entry_count - 1) as f64;
    let table: Vec<u16> = (0..entry_count)
        .map(|i| {
            let y = kind.apply(adj, i as f64 / last);
            // y is clamped to [0, 1], so the cast cannot overflow.
            (y * f64::from(u16::MAX)).round() as u16
        })
        .collect();

    Ok(Curve {
        red: table.clone(),
        green: table.clone(),
        blue: table,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_entry(i: usize, n: usize) -> u16 {
        ((i as f64 / (n - 1) as f64) * 65535.0).round() as u16
    }

    #[test]
    fn neutral_is_identity() {
        for n in [2usize, 3, 256, 1024] {
            let c = generate(CurveKind::Classic, &Adjustments::default(), n).unwrap();
            assert_eq!(c.len(), n);
            for i in 0..n {
                assert_eq!(c.red[i], identity_entry(i, n), "n={n} i={i}");
            }
        }
    }

    #[test]
    fn neutral_endpoints() {
        let c = generate(CurveKind::Classic, &Adjustments::default(), 256).unwrap();
        assert_eq!(c.red[0], 0);
        assert_eq!(c.red[255], 65535);
    }

    #[test]
    fn channels_are_identical() {
        let adj = Adjustments::new(0.8, 1.05, 0.7).unwrap();
        let c = generate(CurveKind::Classic, &adj, 256).unwrap();
        assert_eq!(c.red, c.green);
        assert_eq!(c.green, c.blue);
    }

    #[test]
    fn monotonic_over_normal_ranges() {
        for &contrast in &[0.0, 0.25, 0.8, 1.0, 1.5, 3.0] {
            for &brightness in &[0.0, 0.5, 1.0, 1.5, 2.0] {
                for &gamma in &[0.1, 0.5, 0.7, 1.0, 2.2, 5.0] {
                    let adj = Adjustments::new(contrast, brightness, gamma).unwrap();
                    let c = generate(CurveKind::Classic, &adj, 512).unwrap();
                    assert!(
                        c.red.windows(2).all(|w| w[0] <= w[1]),
                        "not monotonic for c={contrast} b={brightness} g={gamma}"
                    );
                }
            }
        }
    }

    #[test]
    fn clips_high() {
        // y = x + 1 >= 1 everywhere
        let adj = Adjustments::new(1.0, 2.0, 1.0).unwrap();
        let c = generate(CurveKind::Classic, &adj, 256).unwrap();
        assert!(c.red.iter().all(|&v| v == 65535));
    }

    #[test]
    fn clips_low() {
        // y = x - 1 <= 0 everywhere
        let adj = Adjustments::new(1.0, 0.0, 1.0).unwrap();
        let c = generate(CurveKind::Classic, &adj, 256).unwrap();
        assert!(c.red.iter().all(|&v| v == 0));
    }

    #[test]
    fn partial_clipping_keeps_range() {
        // contrast 2 saturates the top half, brightness 0.5 lifts nothing (shifts down)
        let adj = Adjustments::new(2.0, 1.0, 1.0).unwrap();
        let c = generate(CurveKind::Classic, &adj, 256).unwrap();
        assert_eq!(c.red[0], 0);
        assert_eq!(c.red[255], 65535);
        assert_eq!(c.red[128], 65535); // x ~ 0.502 -> 1.004 -> clamped
    }

    #[test]
    fn zero_contrast_is_flat_at_brightness_offset() {
        let adj = Adjustments::new(0.0, 1.25, 1.0).unwrap();
        let c = generate(CurveKind::Classic, &adj, 64).unwrap();
        assert!(c.red.iter().all(|&v| v == 16384)); // round(0.25 * 65535)
    }

    #[test]
    fn gamma_below_one_darkens_midtones_and_above_one_lightens() {
        // y = x^(1/g): g < 1 -> exponent > 1 -> darker; g > 1 -> brighter.
        let dark = generate(
            CurveKind::Classic,
            &Adjustments::new(1.0, 1.0, 0.5).unwrap(),
            257,
        )
        .unwrap();
        let bright = generate(
            CurveKind::Classic,
            &Adjustments::new(1.0, 1.0, 2.0).unwrap(),
            257,
        )
        .unwrap();
        let id = generate(CurveKind::Classic, &Adjustments::default(), 257).unwrap();
        assert!(dark.red[128] < id.red[128]);
        assert!(bright.red[128] > id.red[128]);
    }

    #[test]
    fn known_value() {
        // x = 0.5, c = 0.8, b = 1, g = 0.7 -> 0.8 * 0.5^(1/0.7) = 0.8 * 0.37159... ~ 0.29727
        let adj = Adjustments::new(0.8, 1.0, 0.7).unwrap();
        let y = CurveKind::Classic.apply(&adj, 0.5);
        assert!((y - 0.297_27).abs() < 1e-4, "y = {y}");
    }

    #[test]
    fn matches_original_c_within_one_step() {
        // The C tool truncates `(uint16_t)(UINT16_MAX * val)`; we round.
        let adj = Adjustments::new(0.9, 1.02, 0.8).unwrap();
        let n = 1024;
        let c = generate(CurveKind::Classic, &adj, n).unwrap();
        for i in 0..n {
            let x = i as f64 / (n - 1) as f64;
            let v =
                (adj.contrast * x.powf(1.0 / adj.gamma) + (adj.brightness - 1.0)).clamp(0.0, 1.0);
            let truncated = (65535.0 * v) as u16;
            let diff = (i32::from(c.red[i]) - i32::from(truncated)).abs();
            assert!(diff <= 1, "i={i} diff={diff}");
        }
    }

    #[test]
    fn rejects_non_finite() {
        assert_eq!(
            Adjustments::new(f64::NAN, 1.0, 1.0),
            Err(CurveError::NonFinite("contrast"))
        );
        assert_eq!(
            Adjustments::new(1.0, f64::INFINITY, 1.0),
            Err(CurveError::NonFinite("brightness"))
        );
        assert_eq!(
            Adjustments::new(1.0, 1.0, f64::NEG_INFINITY),
            Err(CurveError::NonFinite("gamma"))
        );
    }

    #[test]
    fn rejects_non_positive_gamma() {
        assert_eq!(
            Adjustments::new(1.0, 1.0, 0.0),
            Err(CurveError::InvalidGamma(0.0))
        );
        assert_eq!(
            Adjustments::new(1.0, 1.0, -1.0),
            Err(CurveError::InvalidGamma(-1.0))
        );
    }

    #[test]
    fn generate_revalidates() {
        // Struct fields are public, so generate() must not trust its input.
        let bad = Adjustments {
            contrast: 1.0,
            brightness: 1.0,
            gamma: 0.0,
        };
        assert!(generate(CurveKind::Classic, &bad, 256).is_err());
    }

    #[test]
    fn rejects_tiny_sizes() {
        assert_eq!(
            generate(CurveKind::Classic, &Adjustments::default(), 0),
            Err(CurveError::InvalidSize(0))
        );
        assert_eq!(
            generate(CurveKind::Classic, &Adjustments::default(), 1),
            Err(CurveError::InvalidSize(1))
        );
        assert!(generate(CurveKind::Classic, &Adjustments::default(), 2).is_ok());
    }

    #[test]
    fn extreme_gamma_stays_in_range() {
        for &g in &[1e-6, 1e6] {
            let c = generate(
                CurveKind::Classic,
                &Adjustments::new(1.0, 1.0, g).unwrap(),
                256,
            )
            .unwrap();
            assert_eq!(c.red[0], 0);
            assert!(c.red.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    #[test]
    fn midgray_neutral_is_exact_identity() {
        let c = generate(CurveKind::Midgray, &CurveKind::Midgray.neutral(), 256).unwrap();
        for (i, value) in c.red.iter().enumerate() {
            assert_eq!(*value, (i * 257) as u16, "i={i}");
        }
    }

    #[test]
    fn midgray_pivot_is_fixed_by_contrast_at_gamma_one() {
        for contrast in [0.1, 1.0, 2.0, 100.0] {
            let adj = Adjustments::new(contrast, 0.0, 1.0).unwrap();
            let c = generate(CurveKind::Midgray, &adj, 256).unwrap();
            assert_eq!(c.red[127], (127 * 257) as u16, "contrast={contrast}");
        }
    }

    #[test]
    fn midgray_brightness_shifts_and_clamps_endpoints() {
        let positive = generate(
            CurveKind::Midgray,
            &Adjustments::new(1.0, 1.0, 1.0).unwrap(),
            256,
        )
        .unwrap();
        let negative = generate(
            CurveKind::Midgray,
            &Adjustments::new(1.0, -1.0, 1.0).unwrap(),
            256,
        )
        .unwrap();
        assert_eq!(
            positive.red[0],
            (128.0_f64 / 255.0 * 65535.0).round() as u16
        );
        assert_eq!(positive.red[255], 65535);
        assert_eq!(negative.red[0], 0);
        assert_eq!(
            negative.red[255],
            (127.0_f64 / 255.0 * 65535.0).round() as u16
        );
    }

    #[test]
    fn midgray_negative_input_pow_quirk_depends_on_gamma() {
        let gamma_one = generate(
            CurveKind::Midgray,
            &Adjustments::new(1.0, 0.5, 1.0).unwrap(),
            256,
        )
        .unwrap();
        let other_gamma = generate(
            CurveKind::Midgray,
            &Adjustments::new(2.0, 0.5, 2.0).unwrap(),
            256,
        )
        .unwrap();
        assert!(gamma_one.red[0] > 0);
        assert_eq!(other_gamma.red[0], 0);
    }

    #[test]
    fn midgray_is_monotonic_for_positive_contrast() {
        for contrast in [0.1, 1.0, 2.0, 100.0] {
            for gamma in [0.3, 1.0, 4.4] {
                let adj = Adjustments::new(contrast, 0.0, gamma).unwrap();
                let c = generate(CurveKind::Midgray, &adj, 256).unwrap();
                assert!(c.red.windows(2).all(|w| w[0] <= w[1]));
            }
        }
    }

    #[test]
    fn midgray_rejects_values_outside_each_range() {
        for (adj, name, min, max) in [
            (Adjustments::new(1.0, 0.0, 0.29).unwrap(), "gamma", 0.3, 4.4),
            (Adjustments::new(1.0, 0.0, 4.41).unwrap(), "gamma", 0.3, 4.4),
            (
                Adjustments::new(0.09, 0.0, 1.0).unwrap(),
                "contrast",
                0.1,
                100.0,
            ),
            (
                Adjustments::new(100.01, 0.0, 1.0).unwrap(),
                "contrast",
                0.1,
                100.0,
            ),
            (
                Adjustments::new(1.0, -1.01, 1.0).unwrap(),
                "brightness",
                -1.0,
                1.0,
            ),
            (
                Adjustments::new(1.0, 1.01, 1.0).unwrap(),
                "brightness",
                -1.0,
                1.0,
            ),
        ] {
            assert_eq!(
                CurveKind::Midgray.validate(&adj),
                Err(CurveError::OutOfRange { name, min, max })
            );
        }
        for adj in [
            Adjustments::new(0.1, -1.0, 0.3).unwrap(),
            Adjustments::new(100.0, 1.0, 4.4).unwrap(),
        ] {
            assert!(CurveKind::Midgray.validate(&adj).is_ok());
        }
    }

    #[test]
    fn midgray_matches_reference_8_bit_entries() {
        let adj = Adjustments::new(1.4, 0.2, 1.8).unwrap();
        let curve = generate(CurveKind::Midgray, &adj, 256).unwrap();
        let reference_8bit_scaled_by_256 = [
            (0, 0),
            (32, 0),
            (64, 29_488),
            (127, 50_872),
            (128, 51_143),
            (192, 65_280),
            (255, 65_280),
        ];
        for (i, reference) in reference_8bit_scaled_by_256 {
            let adapted = (f64::from(reference) * 65535.0 / 65280.0).round() as i32;
            let diff = (i32::from(curve.red[i]) - adapted).abs();
            assert!(diff <= 2, "i={i}: got {}, expected {adapted}", curve.red[i]);
        }
    }
}
