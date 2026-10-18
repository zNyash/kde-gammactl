use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::adjust;
use crate::curves::Adjustments;
use crate::error::Error;
use crate::wayland::{self, OutputInfo};

#[derive(Parser)]
#[command(
    name = "ns-gammactl",
    version,
    about = "Brightness / contrast / gamma for KDE Plasma Wayland (via generated ICC profiles)",
    long_about = "Brightness / contrast / gamma for KDE Plasma Wayland (via generated ICC profiles).\n\n\
Quick use:  ns-gammactl -b 0.4 -c 0.8 -g 0.7   (all displays; add -o DP-1 for one)\n\
Reset:      ns-gammactl reset",
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

/// Same meaning and defaults as wl-gammactl.
#[derive(Args, Clone)]
struct Values {
    /// Brightness (default 1)
    #[arg(short = 'b', long, allow_negative_numbers = true)]
    brightness: Option<f64>,
    /// Contrast (default 1)
    #[arg(short = 'c', long, allow_negative_numbers = true)]
    contrast: Option<f64>,
    /// Gamma, must be > 0 (default 1)
    #[arg(short = 'g', long, allow_negative_numbers = true)]
    gamma: Option<f64>,
}

impl Values {
    fn adjustments(&self) -> Result<Adjustments, Error> {
        Ok(Adjustments::new(
            self.contrast.unwrap_or(1.0),
            self.brightness.unwrap_or(1.0),
            self.gamma.unwrap_or(1.0),
        )?)
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
    if let Some(Command::Generate { values, baseline, out }) = &cli.command {
        adjust::generate_to_file(baseline.as_deref(), &values.adjustments()?, out)?;
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
            for o in adjust::set(&mut session, output.as_deref(), &adj)? {
                println!(
                    "{}: brightness {:.3}, contrast {:.3}, gamma {:.3}",
                    o.label, adj.brightness, adj.contrast, adj.gamma
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
                info.icc_profile_path.as_deref().unwrap_or("(none reported)")
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
    println!("  profile: {}", i.icc_profile_path.as_deref().unwrap_or("(none)"));
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
