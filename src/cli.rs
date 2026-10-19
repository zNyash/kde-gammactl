use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::adjust;
use crate::curves::{Adjustments, CurveKind};
use crate::error::Error;
use crate::wayland::{self, OutputInfo};

#[derive(Parser)]
#[command(
    name = "kde-gammactl",
    version,
    about = "Brightness / contrast / gamma for KDE Plasma Wayland (via generated ICC profiles)",
    long_about = "Brightness / contrast / gamma for KDE Plasma Wayland (via generated ICC profiles).\n\n\
Quick use:  kde-gammactl -b 0.4 -c 0.8 -g 0.7   (all displays; add -o DP-1 for one)\n\
Reset:      kde-gammactl reset",
    arg_required_else_help = true,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    values: Values,

    /// Display name (e.g. DP-1) or UUID; default: all enabled displays
    #[arg(short, long)]
    output: Option<String>,
}

#[derive(Args, Clone)]
struct Values {
    /// Brightness (classic default 1; midgray default 0, range -1..1)
    #[arg(short = 'b', long, allow_negative_numbers = true)]
    brightness: Option<f64>,
    /// Contrast (classic default 1; midgray default 1, range 0.1..100)
    #[arg(short = 'c', long, allow_negative_numbers = true)]
    contrast: Option<f64>,
    /// Gamma (classic must be > 0, default 1; midgray range 0.3..4.4, default 1)
    #[arg(short = 'g', long, allow_negative_numbers = true)]
    gamma: Option<f64>,
    /// Curve formula and parameter ranges (default: classic)
    #[arg(long, value_enum, default_value = "classic")]
    curve: CurveKind,
}

impl Values {
    fn kind(&self) -> CurveKind {
        self.curve
    }

    fn adjustments(&self) -> Result<Adjustments, Error> {
        let neutral = self.curve.neutral();
        let adjustments = Adjustments::new(
            self.contrast.unwrap_or(neutral.contrast),
            self.brightness.unwrap_or(neutral.brightness),
            self.gamma.unwrap_or(neutral.gamma),
        )?;
        self.curve.validate(&adjustments)?;
        Ok(adjustments)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Generate a profile from -b/-c/-g and apply it (same as the bare flags)
    Set {
        #[command(flatten)]
        values: Values,
        /// Display name or UUID; default: all enabled displays
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Remove the adjustment and restore the original profile / source
    Reset {
        /// Display name or UUID; default: all enabled displays
        #[arg(short, long)]
        output: Option<String>,
    },
    /// List connected displays and their current ICC profile path
    Outputs,
    /// Assign an existing ICC profile to a display
    Apply {
        #[arg(short, long)]
        output: Option<String>,
        /// Path to the ICC profile
        #[arg(short, long)]
        profile: PathBuf,
    },
    /// Show the current profile path of a display
    Status {
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Write a generated profile to a file without applying it (for inspection)
    Generate {
        #[command(flatten)]
        values: Values,
        /// Baseline ICC profile; default: built-in sRGB
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Where to write the profile
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    // Commands that never talk to the compositor.
    if let Some(Command::Generate {
        values,
        baseline,
        out,
    }) = &cli.command
    {
        adjust::generate_to_file(
            values.kind(),
            baseline.as_deref(),
            &values.adjustments()?,
            out,
        )?;
        println!("wrote {}", out.display());
        return Ok(());
    }

    let command = cli.command.unwrap_or(Command::Set {
        values: cli.values,
        output: cli.output,
    });

    // Validate numbers before bothering the compositor.
    if let Command::Set { values, .. } = &command {
        values.adjustments()?;
    }

    let mut session = wayland::connect()?;

    match command {
        Command::Set { values, output } => {
            let adj = values.adjustments()?;
            for o in adjust::set(&mut session, output.as_deref(), values.kind(), &adj)? {
                println!(
                    "{}: curve {}, brightness {:.3}, contrast {:.3}, gamma {:.3}",
                    o.label,
                    values.kind().as_str(),
                    adj.brightness,
                    adj.contrast,
                    adj.gamma
                );
                println!("  profile: {}", o.detail);
            }

            Ok(())
        }
        Command::Reset { output } => {
            for o in adjust::reset(&mut session, output.as_deref())? {
                println!("{}: {}", o.label, o.detail);
            }
            Ok(())
        }
        Command::Outputs => {
            let mut any = false;
            for o in session.outputs() {
                any = true;
                print_info(&o.info, false);
            }
            if !any {
                eprintln!("no displays reported by KWin");
            }
            Ok(())
        }
        Command::Status { output } => {
            let o = session.find_output(output.as_deref())?;
            print_info(&o.info, true);
            Ok(())
        }
        Command::Apply { output, profile } => {
            let info = wayland::apply_icc_profile(&mut session, output.as_deref(), &profile)?;
            println!("KDE accepted the configuration for {}", info.label());
            println!(
                "  profile: {}",
                info.icc_profile_path
                    .as_deref()
                    .unwrap_or("(none reported)")
            );
            println!("  source:  {}", info.source_label());
            if let (Ok(want), Some(got)) = (profile.canonicalize(), info.icc_profile_path.as_ref())
            {
                if want.to_string_lossy() != got.as_str() {
                    eprintln!("warning: KWin reports a different profile path than requested");
                }
            }
            Ok(())
        }
        Command::Generate { .. } => unreachable!("handled above"),
    }
}

fn print_info(i: &OutputInfo, verbose: bool) {
    let hw = match (&i.make, &i.model) {
        (Some(a), Some(b)) => format!("{a} {b}"),
        (Some(a), None) | (None, Some(a)) => a.clone(),
        _ => "unknown model".into(),
    };
    println!(
        "{}  [{}]  {}",
        i.label(),
        if i.enabled { "enabled" } else { "disabled" },
        hw
    );
    println!(
        "  profile: {}",
        i.icc_profile_path.as_deref().unwrap_or("(none)")
    );
    if verbose {
        println!("  source:  {}", i.source_label());
        if let Some(u) = &i.uuid {
            println!("  uuid:    {u}");
        }
        if let Some(s) = &i.serial {
            println!("  serial:  {s}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Values {
        match Cli::try_parse_from(args).unwrap().command {
            Some(Command::Set { values, .. }) => values,
            None => Cli::try_parse_from(args).unwrap().values,
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn omitted_curve_matches_explicit_classic() {
        let default = parse(&["kde-gammactl", "-b", "0.8", "-c", "1.2", "-g", "0.9"]);
        let explicit = parse(&[
            "kde-gammactl",
            "--curve",
            "classic",
            "-b",
            "0.8",
            "-c",
            "1.2",
            "-g",
            "0.9",
        ]);
        assert_eq!(default.kind(), explicit.kind());
        assert_eq!(
            default.adjustments().unwrap(),
            explicit.adjustments().unwrap()
        );
    }

    #[test]
    fn midgray_uses_its_neutral_brightness_for_missing_values() {
        let values = parse(&["kde-gammactl", "--curve", "midgray", "-c", "1.2"]);
        let adj = values.adjustments().unwrap();
        assert_eq!(adj.brightness, 0.0);
        assert_eq!(adj.gamma, 1.0);
    }

    #[test]
    fn curve_option_is_available_for_set_and_generate() {
        assert_eq!(
            parse(&["kde-gammactl", "set", "--curve", "midgray"]).kind(),
            CurveKind::Midgray
        );
        let generate = Cli::try_parse_from([
            "kde-gammactl",
            "generate",
            "--curve",
            "midgray",
            "--out",
            "test.icc",
        ])
        .unwrap();
        assert!(matches!(
            generate.command,
            Some(Command::Generate { values, .. }) if values.kind() == CurveKind::Midgray
        ));
    }
}
