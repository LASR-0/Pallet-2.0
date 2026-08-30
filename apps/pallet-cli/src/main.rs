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
use pallet_ipc::{Request, Response, read_message, transport, write_message};
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
        /// Print only the hex, with no extra commentary.
        #[arg(long)]
        quiet: bool,
        /// Do the work in this process rather than asking a running picker.
        #[arg(long)]
        no_daemon: bool,
    },
    /// Report whether the resident picker is running.
    Status,
    /// Show how to bind a global shortcut in this desktop.
    Hotkey,
    /// Hold text on the clipboard until something replaces it.
    ///
    /// Spawned as a detached child by the copy path; not meant to be run by
    /// hand. See `copy_to_clipboard`.
    #[command(hide = true)]
    ClipboardHold {
        /// The text to serve.
        text: String,
    },
    /// Print the colour codes for a value, as the Current screen shows them.
    Convert {
        /// A colour in any supported notation, e.g. `#A5236E`.
        value: String,
        /// Compute harmony and ramps in HSL, as the prototype did, rather than
        /// in perceptually uniform Oklch.
        #[arg(long)]
        hsl: bool,
        /// Put the hex on the clipboard.
        #[arg(long)]
        copy: bool,
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
        Command::ClipboardHold { text } => {
            // Blocks until another application takes the clipboard, then
            // exits. This is the whole job of this process.
            use arboard::SetExtLinux as _;
            let mut clipboard = arboard::Clipboard::new()?;
            clipboard.set().wait().text(text)?;
        }
        Command::Hotkey => {
            let paths = Paths::from_env_or_discover()?;
            let config = Config::load(&paths.config_file()).config;
            let shortcut = &config.picker.shortcut;
            let compositor = pallet_hotkey::Compositor::detect();

            let binary = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "pallet".into());
            let command = format!("{binary} pick");

            println!("desktop   {compositor:?}");
            println!("shortcut  {shortcut}");
            println!();

            match compositor.bind_line(shortcut, &command) {
                Some(line) => {
                    if let Some(file) = compositor.config_hint() {
                        println!("Add this to {file}:");
                    } else {
                        println!("Add this to your compositor config:");
                    }
                    println!();
                    println!("    {line}");
                }
                None => match compositor.manual_hint() {
                    Some(hint) => {
                        println!("{hint}");
                        println!();
                        println!("    {command}");
                    }
                    None => {
                        println!("Pallet does not know this desktop's config format.");
                        println!("Bind a shortcut to this command however your desktop allows:");
                        println!();
                        println!("    {command}");
                    }
                },
            }

            println!();
            println!("Start the resident picker at login so picks are instant:");
            println!();
            println!("    {} &", binary.replace("pallet", "pallet-picker"));
        }
        Command::Status => {
            match ping() {
                Some(version) => println!("picker running (version {version})"),
                // A live picker mid-overlay looks the same as none from here;
                // say so rather than claiming it is not running.
                None if transport::socket_path().exists() => {
                    println!("picker not responding (busy with a pick, or stale socket)");
                }
                None => println!("picker not running"),
            }
            println!("socket  {}", transport::socket_path().display());
        }
        Command::Pick {
            zoom,
            quiet,
            no_daemon,
        } => {
            let paths = Paths::from_env_or_discover()?;
            let loaded = Config::load(&paths.config_file());
            for warning in &loaded.warnings {
                eprintln!("warning: {warning}");
            }
            let config = loaded.config;

            let options = pallet_ipc::PickOptions {
                zoom: zoom.or(Some(u32::from(config.picker.loupe_zoom))),
                average_size: Some(u32::from(config.picker.average_size)),
            };

            // Prefer the resident picker: it holds a warm GPU context, which is
            // roughly 220 ms of the ~250 ms a cold pick costs.
            let response = if no_daemon {
                None
            } else {
                ask_picker(&options)
            };

            let outcome = match response {
                Some(r) => r,
                None => {
                    if !no_daemon {
                        tracing::debug!("no picker running; picking in-process");
                    }
                    pick_in_process(&options)?
                }
            };

            match outcome {
                Response::Picked {
                    hex,
                    source_space,
                    save,
                    ..
                } => {
                    println!("{hex}");

                    let color = pallet_color::Color::parse_hex(&hex)?;

                    if config.picker.copy_on_pick
                        && let Err(e) = copy_to_clipboard(&hex)
                    {
                        eprintln!("warning: could not copy to the clipboard: {e}");
                    }

                    paths.ensure_dirs()?;
                    let store = Store::open(&paths.database_file())?;
                    store.record_pick(color, source_space.as_deref(), None)?;

                    // `S` in the loupe means keep it, not just copy it. The
                    // name is a suggestion the user is expected to change.
                    if save {
                        let name =
                            pallet_color::naming::nearest(color).map(|m| m.named.name.to_string());
                        let mut colour = pallet_store::NewColour::new(color);
                        colour.name = name.clone();
                        colour.source_space = source_space.clone();
                        store.add_colour(&colour)?;
                        if !quiet {
                            eprintln!(
                                "  saved to the library as {}",
                                name.as_deref().unwrap_or("an unnamed colour")
                            );
                        }
                    }

                    if !quiet
                        && config.color.name_new_colors
                        && let Some(m) = pallet_color::naming::nearest(color)
                    {
                        eprintln!("  {}", m.named.name);
                    }
                }
                Response::Cancelled => {
                    if !quiet {
                        eprintln!("cancelled");
                    }
                    std::process::exit(1);
                }
                Response::Error { message } => anyhow::bail!(message),
                Response::Pong { .. } => anyhow::bail!("unexpected reply from the picker"),
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
        Command::Convert { value, hsl, copy } => {
            let color = Color::parse_hex(&value)
                .with_context(|| format!("could not read `{value}` as a colour"))?;
            let space = if hsl { Space::Hsl } else { Space::Oklch };
            print_current(color, space);
            if copy {
                copy_to_clipboard(&color.to_hex())?;
            }
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

/// Ask the resident picker to run a pick, if one is listening.
///
/// Returns `None` when no picker is reachable, so the caller can fall back to
/// doing the work itself rather than failing.
fn ask_picker(options: &pallet_ipc::PickOptions) -> Option<Response> {
    let mut stream = std::os::unix::net::UnixStream::connect(transport::socket_path()).ok()?;
    write_message(&mut stream, &Request::Pick(options.clone())).ok()?;
    // No read timeout: a pick lasts exactly as long as the user takes.
    read_message(&mut stream).ok()
}

/// Ask the picker for its version, to see whether it is alive.
///
/// Unlike a pick, this has a read timeout. The picker serves one request at a
/// time, so while an overlay is up it accepts the connection but does not read
/// it; without a deadline `pallet status` would block until the user finished
/// picking, which is not what "status" should ever do.
fn ping() -> Option<String> {
    let stream = std::os::unix::net::UnixStream::connect(transport::socket_path()).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .ok()?;
    let mut stream = stream;
    write_message(&mut stream, &Request::Ping).ok()?;
    match read_message(&mut stream).ok()? {
        Response::Pong { version } => Some(version),
        _ => None,
    }
}

/// Run a pick in this process, paying full start-up cost.
fn pick_in_process(options: &pallet_ipc::PickOptions) -> Result<Response> {
    let t = std::time::Instant::now();
    let mut capture = pallet_capture::open()?;
    let shot = capture.capture_all()?;
    let captured = t.elapsed();
    if shot.frames.is_empty() {
        anyhow::bail!("no displays are connected");
    }

    let context = pallet_overlay::context()?;
    tracing::info!(
        capture_ms = captured.as_millis(),
        gpu_ms = (t.elapsed() - captured).as_millis(),
        "cold start"
    );
    let outcome = pallet_overlay::run(
        &context,
        shot,
        options.zoom.unwrap_or(16),
        options.average_size.unwrap_or(5),
    )?;

    Ok(match outcome {
        pallet_overlay::Outcome::Picked {
            color,
            at,
            source_space,
            save,
        } => Response::Picked {
            hex: color.to_hex(),
            at,
            source_space,
            save,
        },
        pallet_overlay::Outcome::Cancelled => Response::Cancelled,
    })
}

/// Put the hex on the clipboard, and keep it there.
///
/// A Wayland (or X11) clipboard is not storage: it is a promise served by a
/// live process, and the selection is dropped the moment that process exits.
/// Setting it and returning therefore does nothing at all for a command-line
/// tool, which was the first thing this did and it silently failed.
///
/// So the work is handed to a detached copy of this binary that blocks until
/// something else takes the clipboard, then exits. This is what `wl-copy` does
/// for the same reason.
fn copy_to_clipboard(text: &str) -> Result<()> {
    let exe = std::env::current_exe().context("locating this binary")?;
    std::process::Command::new(exe)
        .arg("clipboard-hold")
        .arg(text)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("spawning the clipboard holder")?;
    Ok(())
}
