//! RGB calibration-curve engine (Phase 3).
//!
//! Pure functions, no I/O. Supports the classic `wl-gammactl` formula and the
//! midgray Gamma-Manager formula:
//!
//! ```text
//! classic: y = clamp(contrast * x^(1 / gamma) + (brightness - 1), 0, 1)
//! midgray: y = clamp((contrast * (x - 127/255) + 127/255)^(1/gamma)
//!                    + brightness * 128/255, 0, 1)
//! entry = round(y * 65535)
//! ```
//!
//! The same value is used for the red, green and blue channels.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum CurveKind {
    #[default]
    Classic,
    Midgray,
}

impl CurveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Midgray => "midgray",
        }
    }

    pub fn neutral(self) -> Adjustments {
        match self {
            Self::Classic => Adjustments {
                contrast: 1.0,
                brightness: 1.0,
                gamma: 1.0,
            },
            Self::Midgray => Adjustments {
                contrast: 1.0,
                brightness: 0.0,
                gamma: 1.0,
            },
        }
    }

    pub fn validate(self, adj: &Adjustments) -> Result<(), CurveError> {
        adj.validate_finite()?;
        match self {
            Self::Classic => adj.validate(),
            Self::Midgray => {
                for (name, value, min, max) in [
                    ("contrast", adj.contrast, 0.1, 100.0),
                    ("brightness", adj.brightness, -1.0, 1.0),
                    ("gamma", adj.gamma, 0.3, 4.4),
                ] {
                    if !(min..=max).contains(&value) {
                        return Err(CurveError::OutOfRange { name, min, max });
                    }
                }
                Ok(())
            }
        }
    }

    /// Parameters must already be validated.
    pub fn apply(self, adj: &Adjustments, x: f64) -> f64 {
        match self {
            Self::Classic => adj.apply(x),
            Self::Midgray => {
                if adj.contrast == 1.0 && adj.brightness == 0.0 && adj.gamma == 1.0 {
                    return x;
                }
                let pivot = 127.0 / 255.0;
                let contrasted = adj.contrast * (x - pivot) + pivot;
                let gamma_adjusted = if adj.gamma != 1.0 && contrasted < 0.0 {
                    f64::NAN
                } else {
                    contrasted.powf(1.0 / adj.gamma)
                };
                (gamma_adjusted + adj.brightness * 128.0 / 255.0).clamp(0.0, 1.0)
            }
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
    /// A curve needs at least two entries (`N - 1` is a divisor).
    InvalidSize(usize),
    /// A parameter is outside the selected curve's supported range.
    OutOfRange {
        name: &'static str,
        min: f64,
        max: f64,
    },
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurveError::NonFinite(name) => write!(f, "{name} must be a finite number"),
            CurveError::InvalidGamma(g) => write!(f, "gamma must be greater than 0 (got {g})"),
            CurveError::InvalidSize(n) => {
                write!(f, "a curve needs at least 2 entries (got {n})")
            }
            CurveError::OutOfRange { name, min, max } => {
                write!(
                    f,
                    "{name} must be between {min} and {max} for the midgray curve"
                )
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
        self.validate_finite()?;
        if self.gamma <= 0.0 {
            return Err(CurveError::InvalidGamma(self.gamma));
        }
        Ok(())
    }

    fn validate_finite(&self) -> Result<(), CurveError> {
        if !self.contrast.is_finite() {
            return Err(CurveError::NonFinite("contrast"));
        }
        if !self.brightness.is_finite() {
            return Err(CurveError::NonFinite("brightness"));
        }
        if !self.gamma.is_finite() {
            return Err(CurveError::NonFinite("gamma"));
        }
        Ok(())
    }

    /// Output for a normalised input `x` in `[0, 1]`, clamped to `[0, 1]`.
    /// Parameters must already be validated.
    pub fn apply(&self, x: f64) -> f64 {
        (self.contrast * x.powf(1.0 / self.gamma) + (self.brightness - 1.0)).clamp(0.0, 1.0)
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
            if y.is_nan() {
                0
            } else {
                (y * f64::from(u16::MAX)).round() as u16
            }
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
        let y = adj.apply(0.5);
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
    fn midgray_pivot_is_independent_of_contrast() {
        for gamma in [0.3, 1.0, 4.4] {
            let low = generate(
                CurveKind::Midgray,
                &Adjustments {
                    contrast: 0.1,
                    brightness: 0.0,
                    gamma,
                },
                256,
            )
            .unwrap();
            let high = generate(
                CurveKind::Midgray,
                &Adjustments {
                    contrast: 100.0,
                    brightness: 0.0,
                    gamma,
                },
                256,
            )
            .unwrap();
            assert_eq!(low.red[127], high.red[127]);
        }
    }

    #[test]
    fn midgray_brightness_moves_endpoints_and_clamps() {
        for (brightness, expected_low, expected_high) in [(-1.0, 0, 32639), (1.0, 32896, 65535)] {
            let c = generate(
                CurveKind::Midgray,
                &Adjustments {
                    contrast: 1.0,
                    brightness,
                    gamma: 1.0,
                },
                256,
            )
            .unwrap();
            assert_eq!(c.red[0], expected_low);
            assert_eq!(c.red[255], expected_high);
        }
    }

    #[test]
    fn midgray_negative_gamma_input_is_not_lifted_by_brightness() {
        let negative = Adjustments {
            contrast: 2.0,
            brightness: 1.0,
            gamma: 2.0,
        };
        let gamma_one = Adjustments {
            gamma: 1.0,
            ..negative
        };
        let c = generate(CurveKind::Midgray, &negative, 256).unwrap();
        let linear = generate(CurveKind::Midgray, &gamma_one, 256).unwrap();
        assert_eq!(c.red[0], 0);
        assert!(linear.red[0] > 0);
    }

    #[test]
    fn midgray_is_monotonic_for_valid_adjustments() {
        for contrast in [0.1, 1.0, 100.0] {
            for brightness in [-1.0, 0.0, 1.0] {
                for gamma in [0.3, 1.0, 4.4] {
                    let adj = Adjustments {
                        contrast,
                        brightness,
                        gamma,
                    };
                    let c = generate(CurveKind::Midgray, &adj, 256).unwrap();
                    assert!(c.red.windows(2).all(|w| w[0] <= w[1]));
                }
            }
        }
    }

    #[test]
    fn midgray_range_validation_is_inclusive_and_rejects_outside_values() {
        let valid = [
            Adjustments {
                contrast: 0.1,
                brightness: -1.0,
                gamma: 0.3,
            },
            Adjustments {
                contrast: 100.0,
                brightness: 1.0,
                gamma: 4.4,
            },
        ];
        for adj in valid {
            assert!(CurveKind::Midgray.validate(&adj).is_ok());
        }

        for (adj, expected) in [
            (
                Adjustments {
                    contrast: 0.099,
                    brightness: 0.0,
                    gamma: 1.0,
                },
                "contrast",
            ),
            (
                Adjustments {
                    contrast: 100.1,
                    brightness: 0.0,
                    gamma: 1.0,
                },
                "contrast",
            ),
            (
                Adjustments {
                    contrast: 1.0,
                    brightness: -1.1,
                    gamma: 1.0,
                },
                "brightness",
            ),
            (
                Adjustments {
                    contrast: 1.0,
                    brightness: 1.1,
                    gamma: 1.0,
                },
                "brightness",
            ),
            (
                Adjustments {
                    contrast: 1.0,
                    brightness: 0.0,
                    gamma: 0.2,
                },
                "gamma",
            ),
            (
                Adjustments {
                    contrast: 1.0,
                    brightness: 0.0,
                    gamma: 4.5,
                },
                "gamma",
            ),
        ] {
            assert_eq!(
                CurveKind::Midgray.validate(&adj),
                Err(CurveError::OutOfRange {
                    name: expected,
                    min: match expected {
                        "contrast" => 0.1,
                        "brightness" => -1.0,
                        _ => 0.3,
                    },
                    max: match expected {
                        "contrast" => 100.0,
                        "brightness" => 1.0,
                        _ => 4.4,
                    },
                })
            );
        }
    }

    #[test]
    fn midgray_golden_values_match_the_scaled_8_bit_formula() {
        let adj = Adjustments {
            contrast: 1.3,
            brightness: 0.2,
            gamma: 1.4,
        };
        let c = generate(CurveKind::Midgray, &adj, 256).unwrap();
        let reference_original = [
            (0, 0),
            (64, 25493),
            (127, 46230),
            (128, 46520),
            (192, 63669),
            (255, 65280),
        ];
        for (index, original) in reference_original {
            let scaled = (f64::from(original) * 65535.0 / 65280.0).round() as i32;
            let actual = i32::from(c.red[index]);
            assert!(
                (actual - scaled).abs() <= 2,
                "i={index}: {actual} vs {scaled}"
            );
        }
    }
}
