//! The Pallet window.
//!
//! Every colour value shown here is computed by `pallet-color` rather than in
//! the frontend. The prototype did its own maths in JavaScript, but two
//! implementations of the same conversions would inevitably disagree, and the
//! hex a user copies from the window must be the hex the picker recorded.

// A release build should not also open a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pallet_color::{Color, Harmony, Space, contrast, naming, ramp};
use pallet_store::{Config, Store};
use serde::Serialize;
use tauri::Manager;

/// One row of the HEX / RGB / HSL block.
#[derive(Debug, Serialize)]
struct CodeRow {
    label: String,
    value: String,
}

/// One swatch on the tints and shades ramp.
#[derive(Debug, Serialize)]
struct RampStep {
    step: u16,
    hex: String,
}

/// Everything the Current screen displays for one colour.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ColorDetail {
    hex: String,
    name: String,
    on_color: String,
    code_rows: Vec<CodeRow>,
    harmony: Vec<String>,
    ramp: Vec<RampStep>,
}

/// Application state shared with commands.
///
/// The store is behind a mutex because a `rusqlite::Connection` is `Send` but
/// not `Sync`, and Tauri hands state to commands running on a pool of threads.
/// Contention is a non-issue: this is a single window doing one thing at a
/// time, and the alternative — a connection pool — would be machinery for a
/// problem that does not exist here.
struct AppState {
    store: std::sync::Mutex<Store>,
    space: Space,
}

/// Compute everything the Current screen needs for a colour.
#[tauri::command]
fn color_detail(
    hex: String,
    harmony: String,
    state: tauri::State<'_, AppState>,
) -> Result<ColorDetail, String> {
    let color = Color::parse_hex(&hex).map_err(|e| e.to_string())?;
    let (r, g, b) = color.to_rgb();
    let (h, s, l) = color.to_hsl();

    let harmony = match harmony.as_str() {
        "analogous" => Harmony::Analogous,
        "triadic" => Harmony::Triadic,
        "split" => Harmony::Split,
        _ => Harmony::Complementary,
    };

    Ok(ColorDetail {
        hex: color.to_hex(),
        name: naming::nearest(color)
            .map(|m| m.named.name.to_string())
            .unwrap_or_else(|| "Unnamed".into()),
        // The prototype picks the overlay text from HSL lightness with a 0.62
        // cutoff. `pallet_color::contrast::best_text_on` is more accurate, but
        // this value is part of how the design looks, so the design wins.
        on_color: if l > 0.62 {
            "rgba(0,0,0,.78)".into()
        } else {
            "rgba(255,255,255,.94)".into()
        },
        code_rows: vec![
            CodeRow {
                label: "HEX".into(),
                value: color.to_hex(),
            },
            CodeRow {
                label: "RGB".into(),
                value: format!("{r} · {g} · {b}"),
            },
            CodeRow {
                label: "HSL".into(),
                value: format!(
                    "{}° · {}% · {}%",
                    h.round(),
                    (s * 100.0).round(),
                    (l * 100.0).round()
                ),
            },
        ],
        harmony: harmony
            .swatches(color, state.space)
            .iter()
            .map(|c| c.to_hex())
            .collect(),
        ramp: ramp::ramp(color, state.space)
            .into_iter()
            .map(|s| RampStep {
                step: s.step,
                hex: s.color.to_hex(),
            })
            .collect(),
    })
}

/// A palette as the Palettes screen shows it.
#[derive(Debug, Serialize)]
struct PaletteCard {
    id: String,
    /// Two-digit index, as the prototype numbers its cards.
    num: String,
    name: String,
    /// "5 · 2019" — swatch count and the year it was created.
    meta: String,
    colors: Vec<String>,
}

/// A colour as the Colours screen shows it.
#[derive(Debug, Serialize)]
struct ColourChip {
    id: String,
    name: String,
    hex: String,
}

/// Every palette, newest first.
#[tauri::command]
fn palettes(state: tauri::State<'_, AppState>) -> Result<Vec<PaletteCard>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;

    let mut palettes = store.palettes().map_err(|e| e.to_string())?;
    // The library returns newest first, which suits a recents list. A swatch
    // book reads the other way, and it is the order the prototype numbers its
    // cards in: Winter Sunset is 01.
    palettes.reverse();

    Ok(palettes
        .into_iter()
        .enumerate()
        .map(|(i, p)| PaletteCard {
            num: format!("{:02}", i + 1),
            meta: format!("{} · {}", p.colours.len(), p.created_at.year()),
            colors: p.colours.iter().map(|c| c.color.to_hex()).collect(),
            id: p.id,
            name: p.name,
        })
        .collect())
}

/// Every named colour in the library, newest first.
///
/// Unnamed colours are left out: the Colours screen is a grid of *named*
/// swatches, and a palette's members are shown on its own card rather than
/// duplicated here.
#[tauri::command]
fn colours(state: tauri::State<'_, AppState>) -> Result<Vec<ColourChip>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;

    let mut colours = store.colours().map_err(|e| e.to_string())?;
    // Insertion order, as above.
    colours.reverse();

    Ok(colours
        .into_iter()
        .filter_map(|c| {
            c.name.map(|name| ColourChip {
                id: c.id,
                name,
                hex: c.color.to_hex(),
            })
        })
        .collect())
}

/// How many colours a palette may hold.
///
/// Three is the smallest group that reads as a palette rather than a pair;
/// twenty-five is where the strip stops being legible at window width and a
/// palette stops being a palette.
const MIN_PALETTE: usize = 3;
const MAX_PALETTE: usize = 25;

/// Ask the resident picker for a colour.
///
/// Async because a pick lasts as long as the user takes: on a blocking command
/// Tauri would hold a worker thread and the window would stop responding until
/// they clicked.
#[tauri::command]
async fn pick_colour() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(pallet_ipc::transport::socket_path())
            .map_err(|_| "no picker is running — start pallet-picker".to_string())?;

        pallet_ipc::write_message(
            &mut stream,
            &pallet_ipc::Request::Pick(pallet_ipc::PickOptions::default()),
        )
        .map_err(|e| e.to_string())?;

        match pallet_ipc::read_message::<_, pallet_ipc::Response>(&mut stream)
            .map_err(|e| e.to_string())?
        {
            pallet_ipc::Response::Picked { hex, .. } => Ok(Some(hex)),
            pallet_ipc::Response::Cancelled => Ok(None),
            pallet_ipc::Response::Error { message } => Err(message),
            pallet_ipc::Response::Pong { .. } => Err("unexpected reply".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The limits the Build screen enforces, so the frontend need not restate them.
#[tauri::command]
fn palette_limits() -> (usize, usize) {
    (MIN_PALETTE, MAX_PALETTE)
}

/// A default name for the next palette, matching the prototype's "Untitled 13".
#[tauri::command]
fn next_palette_name(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;
    let count = store.palettes().map_err(|e| e.to_string())?.len();
    Ok(format!("Untitled {}", count + 1))
}

/// Save a built palette.
///
/// Colours are added to the library as unnamed entries: a palette member is a
/// colour in its own right, and naming every one of them would be busywork.
#[tauri::command]
fn save_palette(
    name: String,
    hexes: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    if hexes.len() < MIN_PALETTE {
        return Err(format!("a palette needs at least {MIN_PALETTE} colours"));
    }
    if hexes.len() > MAX_PALETTE {
        return Err(format!("a palette holds at most {MAX_PALETTE} colours"));
    }

    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;

    let mut ids = Vec::with_capacity(hexes.len());
    for hex in &hexes {
        let color = Color::parse_hex(hex).map_err(|e| e.to_string())?;
        ids.push(
            store
                .add_colour(&pallet_store::NewColour::new(color))
                .map_err(|e| e.to_string())?,
        );
    }

    store
        .create_palette(name.trim(), &ids)
        .map_err(|e| e.to_string())
}

/// The most recent pick, so the window opens showing something real.
#[tauri::command]
fn latest_pick(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .recent_picks(1)
        .map(|picks| picks.first().map(|p| p.color.to_hex()))
        .map_err(|e| e.to_string())
}

/// The WCAG and APCA figures for a colour against white and black.
#[tauri::command]
fn contrast_report(hex: String) -> Result<Vec<CodeRow>, String> {
    let color = Color::parse_hex(&hex).map_err(|e| e.to_string())?;
    Ok([
        ("on white", Color::new(255, 255, 255)),
        ("on black", Color::new(0, 0, 0)),
    ]
    .into_iter()
    .map(|(label, other)| {
        let ratio = contrast::wcag21_ratio(color, other);
        CodeRow {
            label: label.into(),
            value: format!(
                "{ratio:.2}:1 {}  APCA {:+.1}",
                contrast::WcagLevel::of(ratio).label(),
                contrast::apca_lc(color, other)
            ),
        }
    })
    .collect())
}

/// Work around WebKitGTK's DMABUF renderer, which fails on several drivers.
///
/// On NVIDIA under Wayland it aborts start-up outright with
/// "Error 71 (Protocol error) dispatching to Wayland display", so the window
/// never appears. Disabling it costs a little rendering throughput that a
/// small, mostly static window will never notice, and it is what practically
/// every WebKitGTK application ends up doing.
///
/// Only set when the user has not chosen a value, so it can be overridden.
#[cfg(target_os = "linux")]
fn apply_webkit_workarounds() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called at the very top of main, before any thread that might
        // read the environment has been spawned.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
        tracing::debug!("disabled WebKitGTK's DMABUF renderer");
    }
}

fn main() {
    pallet_core::logging::init("info");

    #[cfg(target_os = "linux")]
    apply_webkit_workarounds();

    tauri::Builder::default()
        .setup(|app| {
            let paths = pallet_core::Paths::from_env_or_discover()?;
            paths.ensure_dirs()?;

            let loaded = Config::load(&paths.config_file());
            for warning in &loaded.warnings {
                tracing::warn!("{warning}");
            }

            let store = Store::open(&paths.database_file())?;
            pallet_store::seed::seed_if_empty(&store)?;

            app.manage(AppState {
                store: std::sync::Mutex::new(store),
                space: loaded.config.color.space.into(),
            });

            tracing::info!(
                windows = app.webview_windows().len(),
                labels = ?app.webview_windows().keys().collect::<Vec<_>>(),
                "setup complete"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            color_detail,
            latest_pick,
            contrast_report,
            palettes,
            colours,
            pick_colour,
            palette_limits,
            next_palette_name,
            save_palette
        ])
        .run(tauri::generate_context!())
        .expect("the Pallet window failed to start");
}
