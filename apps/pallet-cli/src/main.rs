//! Pallet's command line.
//!
//! On Wayland this is also the primary hotkey path: bind `pallet pick` in your
//! compositor config, e.g. for Hyprland
//!
//! ```text
//! bind = CTRL SHIFT, P, exec, pallet pick
//! ```

use anyhow::Result;
use clap::{Parser, Subcommand};
use pallet_core::{Paths, logging};

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
    },
    /// Show where Pallet keeps its configuration and library.
    Paths,
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
        }
        Command::Pick => anyhow::bail!("`pallet pick` lands in M4"),
        Command::Convert { .. } => anyhow::bail!("`pallet convert` lands in M1"),
    }

    Ok(())
}
