//! Pallet's command line.
//!
//! On Wayland this is also the primary hotkey path: bind `pallet pick` in your
//! compositor config, e.g. for Hyprland
//!
//! ```text
//! bind = CTRL SHIFT, P, exec, pallet pick
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pallet_color::{Color, Harmony, Space, contrast, naming, ramp};
use pallet_core::{Paths, logging};
use pallet_store::{Config, Store};

#[derive(Debug, Parser)]
#[command(
    name = "pallet",
    version,
    about = "Pick colours from anywhere on screen"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Freeze the screen and pick a colour.
    Pick,
    /// Print the colour codes for a value, as the Current screen shows them.
    Convert {
        /// A colour in any supported notation, e.g. `#A5236E`.
        value: String,
        /// Compute harmony and ramps in HSL, as the prototype did, rather than
        /// in perceptually uniform Oklch.
        #[arg(long)]
        hsl: bool,
    },
    /// Show where Pallet keeps its configuration and library.
    Paths,
    /// Create the config file and library, seeding the sample palettes.
    Init,
}

fn main() -> Result<()> {
    logging::init("info");
    let cli = Cli::parse();

    match cli.command {
        Command::Paths => {
            let paths = Paths::from_env_or_discover()?;
            println!("config  {}", paths.config_file().display());
            println!("library {}", paths.database_file().display());
            println!("exports {}", paths.exports_dir().display());

            let loaded = Config::load(&paths.config_file());
            for warning in &loaded.warnings {
                eprintln!("warning: {warning}");
            }

            if paths.database_file().exists() {
                let store = Store::open(&paths.database_file())?;
                println!();
                println!("library  schema v{}", store.schema_version()?);
                println!("         {} colours", store.colours()?.len());
                println!("         {} palettes", store.palettes()?.len());
                println!(
                    "         {} recent picks",
                    store.recent_picks(usize::MAX)?.len()
                );
            }
        }
        Command::Init => {
            let paths = Paths::from_env_or_discover()?;
            paths.ensure_dirs()?;

            let config_path = paths.config_file();
            if !config_path.exists() {
                Config::default().save(&config_path)?;
                println!("wrote {}", config_path.display());
            }

            let store = Store::open(&paths.database_file())?;
            if pallet_store::seed::seed_if_empty(&store)? {
                println!("seeded the library with the sample palettes");
            }
            println!("library ready at {}", paths.database_file().display());
        }
        Command::Pick => anyhow::bail!("`pallet pick` lands in M4"),
        Command::Convert { value, hsl } => {
            let color = Color::parse_hex(&value)
                .with_context(|| format!("could not read `{value}` as a colour"))?;
            let space = if hsl { Space::Hsl } else { Space::Oklch };
            print_current(color, space);
        }
    }

    Ok(())
}

/// Render what the Current screen shows, for one colour.
fn print_current(color: Color, space: Space) {
    let (r, g, b) = color.to_rgb();
    let (h, s, l) = color.to_hsl();
    let name = naming::nearest(color);

    println!("{}", color.to_hex());
    if let Some(m) = &name {
        let qualifier = if m.exact { "exact" } else { "nearest" };
        println!("  {}  ({qualifier}, dE {:.1})", m.named.name, m.distance);
    }
    println!();
    println!("  HEX  {}", color.to_hex());
    println!("  RGB  {r} · {g} · {b}");
    println!(
        "  HSL  {}° · {}% · {}%",
        h.round(),
        (s * 100.0).round(),
        (l * 100.0).round()
    );

    let white = Color::new(255, 255, 255);
    let black = Color::new(0, 0, 0);
    println!();
    println!("CONTRAST");
    for (label, other) in [("on white", white), ("on black", black)] {
        let ratio = contrast::wcag21_ratio(color, other);
        println!(
            "  {label}   WCAG {ratio:.2}:1 {:<8}  APCA {:+.1}",
            contrast::WcagLevel::of(ratio).label(),
            contrast::apca_lc(color, other)
        );
    }

    println!();
    println!("HARMONY  ({})", space.id());
    for harmony in Harmony::ALL {
        let swatches: Vec<String> = harmony
            .swatches(color, space)
            .iter()
            .map(|c| c.to_hex())
            .collect();
        println!("  {:<7} {}", harmony.label(), swatches.join("  "));
    }

    println!();
    println!("TINTS & SHADES  ({})", space.id());
    for swatch in ramp::ramp(color, space) {
        let marker = if swatch.is_base { " ←" } else { "" };
        println!("  {:>3}  {}{marker}", swatch.step, swatch.color.to_hex());
    }
}
