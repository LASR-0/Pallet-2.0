//! Pallet's command line.
//!
//! On Wayland this is also the primary hotkey path: bind `pallet pick` in your
//! compositor config, e.g. for Hyprland
//!
//! ```text
//! bind = CTRL SHIFT, P, exec, pallet pick
//! ```

use std::path::PathBuf;

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
    Pick {
        /// Magnification, overriding the configured value.
        #[arg(long)]
        zoom: Option<u32>,
        /// Print only the hex, with no logging noise.
        #[arg(long)]
        quiet: bool,
    },
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
    /// Report the capture backend and connected displays.
    Displays,
    /// Hold one backend open and re-enumerate on a timer. Used to verify that
    /// a long-lived picker survives monitors coming and going.
    Watch {
        /// How many times to poll.
        #[arg(long, default_value_t = 10)]
        ticks: u32,
        /// Milliseconds between polls.
        #[arg(long, default_value_t = 1000)]
        every: u64,
    },
    /// Capture a monitor to a PNG. A diagnostic, not part of normal use:
    /// Pallet never writes captured frames to disk on its own.
    Capture {
        /// Monitor id, e.g. `DP-1`. Defaults to the first connected display.
        #[arg(long)]
        monitor: Option<String>,
        /// Destination PNG.
        #[arg(long)]
        out: PathBuf,
    },
    /// Read the colour at a global physical coordinate, without any UI.
    Probe {
        /// Global physical x.
        x: i32,
        /// Global physical y.
        y: i32,
        /// Average a square of this many pixels instead of reading one.
        #[arg(long, default_value_t = 1)]
        average: u32,
    },
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
        Command::Pick { zoom, quiet } => {
            let paths = Paths::from_env_or_discover()?;
            let loaded = Config::load(&paths.config_file());
            for warning in &loaded.warnings {
                eprintln!("warning: {warning}");
            }
            let config = loaded.config;

            let mut capture = pallet_capture::open()?;
            let shot = capture.capture_all()?;
            if shot.frames.is_empty() {
                anyhow::bail!("no displays are connected");
            }

            let outcome = pallet_overlay::run(
                shot,
                zoom.unwrap_or(u32::from(config.picker.loupe_zoom)),
                u32::from(config.picker.average_size),
            )?;

            match outcome {
                pallet_overlay::Outcome::Picked {
                    color,
                    source_space,
                    ..
                } => {
                    println!("{}", color.to_hex());

                    if config.picker.copy_on_pick {
                        // Clipboard support lands with the resident daemon in
                        // M5; until then the hex goes to stdout only.
                        tracing::debug!("copy-on-pick is configured but lands in M5");
                    }

                    paths.ensure_dirs()?;
                    let store = Store::open(&paths.database_file())?;
                    store.record_pick(color, source_space.as_deref(), None)?;

                    if !quiet
                        && config.color.name_new_colors
                        && let Some(m) = pallet_color::naming::nearest(color)
                    {
                        eprintln!("  {}", m.named.name);
                    }
                }
                pallet_overlay::Outcome::Cancelled => {
                    if !quiet {
                        eprintln!("cancelled");
                    }
                    std::process::exit(1);
                }
            }
        }
        Command::Capture { monitor, out } => {
            let mut capture = pallet_capture::open()?;
            let id = match monitor {
                Some(id) => id,
                None => capture
                    .monitors()?
                    .first()
                    .map(|m| m.id.clone())
                    .context("no monitors are connected")?,
            };

            let frame = capture.capture_monitor(&id)?;
            let (w, h) = (frame.monitor.pixel_width, frame.monitor.pixel_height);

            let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
            for y in 0..h {
                for x in 0..w {
                    let c = frame.pixel(x, y).context("pixel inside frame bounds")?;
                    rgb.extend_from_slice(&[c.r, c.g, c.b]);
                }
            }

            image::RgbImage::from_raw(w, h, rgb)
                .context("buffer matches the frame dimensions")?
                .save(&out)?;
            println!("wrote {} ({w}x{h}) from {id}", out.display());
        }
        Command::Watch { ticks, every } => {
            let mut capture = pallet_capture::open()?;
            for tick in 1..=ticks {
                match capture.capture_all() {
                    Ok(shot) => {
                        let ids: Vec<_> =
                            shot.frames.iter().map(|f| f.monitor.id.clone()).collect();
                        println!("tick {tick:>2}: {} frames {:?}", shot.frames.len(), ids);
                    }
                    Err(e) => println!("tick {tick:>2}: ERROR {e}"),
                }
                std::thread::sleep(std::time::Duration::from_millis(every));
            }
        }
        Command::Displays => {
            let mut capture = pallet_capture::open()?;
            println!("backend  {}", capture.backend_name());
            for m in capture.monitors()? {
                println!(
                    "  {:<10} {}x{}px  logical {}x{} at ({},{})  scale {:.3}  {:?}  {:?}",
                    m.id,
                    m.pixel_width,
                    m.pixel_height,
                    m.logical_width,
                    m.logical_height,
                    m.logical_x,
                    m.logical_y,
                    m.scale_x(),
                    m.transform,
                    m.profile
                );
            }
        }
        Command::Probe { x, y, average } => {
            let started = std::time::Instant::now();
            let mut capture = pallet_capture::open()?;
            let connected = started.elapsed();

            let grab = std::time::Instant::now();
            let shot = capture.capture_all()?;
            let grabbed = grab.elapsed();

            let color = if average > 1 {
                shot.average_at(x, y, average)?
            } else {
                shot.pixel_at(x, y)?
            };

            let bytes: usize = shot.frames.iter().map(|f| f.size_bytes()).sum();
            println!("{}", color.to_hex());
            eprintln!(
                "  {} frames, {:.1} MiB, connect {:?}, grab {:?}",
                shot.frames.len(),
                bytes as f64 / (1024.0 * 1024.0),
                connected,
                grabbed
            );
        }
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
