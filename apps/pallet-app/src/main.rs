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
    /// The live settings. Held rather than only their derived values so the
    /// Settings screen can edit them and everything else sees the change.
    config: std::sync::Mutex<Config>,
    paths: pallet_core::Paths,
}

impl AppState {
    /// The colour space harmony and ramps are computed in.
    fn space(&self) -> Space {
        self.config
            .lock()
            .map(|c| c.color.space.into())
            .unwrap_or_default()
    }
}

/// One row of the Settings screen.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingRow {
    key: String,
    label: String,
    hint: String,
    /// What the control shows, e.g. "ON", "16x", "CTRL + SHIFT + P".
    value: String,
    /// Lit like the prototype's ON pill.
    on: bool,
    /// False for rows that only report, such as the compositor-owned shortcut.
    editable: bool,
}

/// The Settings screen's rows, in order.
#[tauri::command]
fn settings(state: tauri::State<'_, AppState>) -> Result<Vec<SettingRow>, String> {
    let c = state
        .config
        .lock()
        .map_err(|_| "settings are unavailable".to_string())?;

    let toggle = |on: bool| (if on { "ON" } else { "OFF" }.to_string(), on);
    let (copy_v, copy_on) = toggle(c.picker.copy_on_pick);
    let (name_v, name_on) = toggle(c.color.name_new_colors);
    let (top_v, top_on) = toggle(c.general.stay_on_top);

    let row =
        |key: &str, label: &str, hint: &str, value: String, on: bool, editable: bool| SettingRow {
            key: key.into(),
            label: label.into(),
            hint: hint.into(),
            value,
            on,
            editable,
        };

    Ok(vec![
        // The prototype's six, in its order.
        row(
            "shortcut",
            "Global pick shortcut",
            "Your compositor owns this — run `pallet hotkey`",
            c.picker.shortcut.clone(),
            false,
            false,
        ),
        row(
            "loupe_zoom",
            "Loupe zoom",
            "Magnification at rest",
            format!("{}×", c.picker.loupe_zoom),
            false,
            true,
        ),
        row(
            "copy_on_pick",
            "Copy on pick",
            "Puts hex on the clipboard immediately",
            copy_v,
            copy_on,
            true,
        ),
        row(
            "multi_pick_length",
            "Multi-pick length",
            "Colours gathered in one pass",
            c.picker.multi_pick_length.to_string(),
            false,
            true,
        ),
        row(
            "name_new_colors",
            "Name new colours",
            "Matches the nearest named colour",
            name_v,
            name_on,
            true,
        ),
        row(
            "stay_on_top",
            "Stay on top",
            "Window floats above other apps",
            top_v,
            top_on,
            true,
        ),
        // Settings that exist but the prototype never drew. Leaving them
        // unreachable would mean editing config.toml by hand to change how
        // colours are reported.
        row(
            "theme",
            "Theme",
            "Sketchbook is warm, Studio is dark",
            format!("{:?}", c.general.theme).to_uppercase(),
            false,
            true,
        ),
        row(
            "average_size",
            "Shift-average size",
            "Square sampled while Shift is held",
            format!("{}×{}", c.picker.average_size, c.picker.average_size),
            false,
            true,
        ),
        row(
            "space",
            "Harmony and ramps",
            "Oklch is perceptually even; HSL matches other tools",
            format!("{:?}", c.color.space).to_uppercase(),
            false,
            true,
        ),
        row(
            "report_space",
            "Report colours as",
            "sRGB is what a designer pastes into CSS",
            match c.picker.report_space {
                pallet_store::config::ReportSpace::Srgb => "sRGB".into(),
                pallet_store::config::ReportSpace::Display => "DISPLAY".into(),
            },
            false,
            true,
        ),
    ])
}

/// Advance one setting to its next value and save.
///
/// Every control cycles rather than opening an editor: the values are all
/// short lists or toggles, and one interaction model keeps the row compact
/// enough to match the prototype's design.
#[tauri::command]
fn cycle_setting(key: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use pallet_store::config::{ReportSpace, Space as CfgSpace, Theme};

    let mut c = state
        .config
        .lock()
        .map_err(|_| "settings are unavailable".to_string())?;

    /// Step through a list, wrapping.
    fn next<T: PartialEq + Copy>(current: T, options: &[T]) -> T {
        let i = options.iter().position(|o| *o == current).unwrap_or(0);
        options[(i + 1) % options.len()]
    }

    match key.as_str() {
        "loupe_zoom" => c.picker.loupe_zoom = next(c.picker.loupe_zoom, &[2, 4, 8, 16, 32, 64]),
        "copy_on_pick" => c.picker.copy_on_pick = !c.picker.copy_on_pick,
        "multi_pick_length" => {
            c.picker.multi_pick_length = next(c.picker.multi_pick_length, &[3, 5, 8, 12, 25]);
        }
        "name_new_colors" => c.color.name_new_colors = !c.color.name_new_colors,
        "stay_on_top" => c.general.stay_on_top = !c.general.stay_on_top,
        "theme" => c.general.theme = next(c.general.theme, &[Theme::Sketchbook, Theme::Studio]),
        // Odd sizes only: an even square has no centre pixel to anchor on.
        "average_size" => c.picker.average_size = next(c.picker.average_size, &[1, 3, 5, 7, 9]),
        "space" => c.color.space = next(c.color.space, &[CfgSpace::Oklch, CfgSpace::Hsl]),
        "report_space" => {
            c.picker.report_space = next(
                c.picker.report_space,
                &[ReportSpace::Srgb, ReportSpace::Display],
            );
        }
        other => return Err(format!("`{other}` is not an editable setting")),
    }

    c.save(&state.paths.config_file())
        .map_err(|e| e.to_string())
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
            .swatches(color, state.space())
            .iter()
            .map(|c| c.to_hex())
            .collect(),
        ramp: ramp::ramp(color, state.space())
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
fn palettes(
    facets: Vec<String>,
    sort: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PaletteCard>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;

    let mut palettes = store.palettes().map_err(|e| e.to_string())?;
    // The library returns newest first, which suits a recents list. A swatch
    // book reads the other way, and it is the order the prototype numbers its
    // cards in: Winter Sunset is 01.
    palettes.reverse();

    let cards: Vec<PaletteCard> = palettes
        .into_iter()
        .enumerate()
        .map(|(i, p)| PaletteCard {
            num: format!("{:02}", i + 1),
            meta: format!("{} · {}", p.colours.len(), p.created_at.year()),
            colors: p.colours.iter().map(|c| c.color.to_hex()).collect(),
            id: p.id,
            name: p.name,
        })
        .collect();

    // A palette is filtered by its first swatch, which is the one a person
    // reads it by. Judging it by every member would match almost everything
    // once a palette has more than a few colours.
    let mut cards = arrange(cards, &facets, &sort, |c| {
        c.colors
            .first()
            .and_then(|h| Color::parse_hex(h).ok())
            .unwrap_or(Color::new(0, 0, 0))
    });
    if sort == "name" {
        cards.sort_by_key(|c| c.name.to_lowercase());
    }
    Ok(cards)
}

/// Apply the UI's facet chips and sort choice to a list of colours.
fn arrange<T, F>(mut items: Vec<T>, facets: &[String], sort: &str, colour_of: F) -> Vec<T>
where
    F: Fn(&T) -> Color,
{
    let facets: Vec<pallet_color::Facet> = facets
        .iter()
        .filter_map(|f| pallet_color::Facet::parse(f))
        .collect();
    items.retain(|item| pallet_color::facets::matches_all(colour_of(item), &facets));

    if let Some(sort) = pallet_color::Sort::parse(sort)
        && sort != pallet_color::Sort::Added
    {
        items.sort_by(|a, b| {
            let (ka, kb) = (sort.key(colour_of(a)), sort.key(colour_of(b)));
            ka.0.cmp(&kb.0)
                .then(ka.1.partial_cmp(&kb.1).unwrap_or(std::cmp::Ordering::Equal))
        });
    }
    items
}

/// Every named colour in the library, newest first.
///
/// Unnamed colours are left out: the Colours screen is a grid of *named*
/// swatches, and a palette's members are shown on its own card rather than
/// duplicated here.
#[tauri::command]
fn colours(
    facets: Vec<String>,
    sort: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ColourChip>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?;

    let mut colours = store.colours().map_err(|e| e.to_string())?;
    // Insertion order, as above.
    colours.reverse();

    let chips: Vec<ColourChip> = colours
        .into_iter()
        .filter_map(|c| {
            c.name.map(|name| ColourChip {
                id: c.id,
                name,
                hex: c.color.to_hex(),
            })
        })
        .collect();

    let mut chips = arrange(chips, &facets, &sort, |c| {
        Color::parse_hex(&c.hex).unwrap_or(Color::new(0, 0, 0))
    });
    if sort == "name" {
        chips.sort_by_key(|c| c.name.to_lowercase());
    }
    Ok(chips)
}

/// How many colours a palette may hold.
///
/// Three is the smallest group that reads as a palette rather than a pair;
/// twenty-five is where the strip stops being legible at window width and a
/// palette stops being a palette.
const MIN_PALETTE: usize = 3;
const MAX_PALETTE: usize = 25;

/// Connect to the picker, starting it if it is not already running.
///
/// The picker is a separate process holding a warm GPU context, but that is an
/// implementation detail of how picking is made fast — not something a user
/// should have to know about or launch. Without this, the Build screen's
/// primary action fails with an error telling them to go run a daemon.
#[cfg(unix)]
fn connect_to_picker() -> Result<std::os::unix::net::UnixStream, String> {
    use std::os::unix::net::UnixStream;

    let socket = pallet_ipc::transport::socket_path();
    if let Ok(stream) = UnixStream::connect(&socket) {
        return Ok(stream);
    }

    // Beside this binary, so a build tree and an installed bundle both work.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let picker = exe
        .parent()
        .ok_or("could not locate the picker binary")?
        .join("pallet-picker");
    if !picker.exists() {
        return Err(format!("the picker is missing from {}", picker.display()));
    }

    tracing::info!("starting the picker");
    std::process::Command::new(&picker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start the picker: {e}"))?;

    // It builds a GPU context before it listens, which measured about 190 ms.
    // Poll rather than sleep a fixed time so a fast machine is not penalised
    // and a slow one still succeeds.
    for _ in 0..100 {
        if let Ok(stream) = UnixStream::connect(&socket) {
            return Ok(stream);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    Err("the picker did not start in time".into())
}

/// Ask the resident picker for a colour.
///
/// Async because a pick lasts as long as the user takes: on a blocking command
/// Tauri would hold a worker thread and the window would stop responding until
/// they clicked.
#[tauri::command]
async fn pick_colour() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut stream = connect_to_picker()?;

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

/// Rename a palette.
#[tauri::command]
fn rename_palette(
    id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a palette needs a name".into());
    }
    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .rename_palette(&id, name)
        .map_err(|e| e.to_string())
}

/// Delete a palette. Its colours stay in the library.
#[tauri::command]
fn delete_palette(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .delete_palette(&id)
        .map_err(|e| e.to_string())
}

/// Rename a colour.
#[tauri::command]
fn rename_colour(
    id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a colour needs a name".into());
    }
    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .rename_colour(&id, Some(name))
        .map_err(|e| e.to_string())
}

/// Delete a colour. It is removed from any palette holding it.
#[tauri::command]
fn delete_colour(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .delete_colour(&id)
        .map_err(|e| e.to_string())
}

/// One entry in the pick history.
#[derive(Debug, Serialize)]
struct RecentPick {
    id: String,
    hex: String,
}

/// The pick history, newest first.
#[tauri::command]
fn recent_picks(
    limit: usize,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RecentPick>, String> {
    Ok(state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .recent_picks(limit)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| RecentPick {
            id: p.id,
            hex: p.color.to_hex(),
        })
        .collect())
}

/// Keep a picked colour in the library.
///
/// The suggested name is the nearest match, which the user is expected to
/// change; naming is a suggestion, not a verdict.
#[tauri::command]
fn save_colour(hex: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let color = Color::parse_hex(&hex).map_err(|e| e.to_string())?;
    let mut colour = pallet_store::NewColour::new(color);
    colour.name = naming::nearest(color).map(|m| m.named.name.to_string());

    state
        .store
        .lock()
        .map_err(|_| "the colour library is unavailable".to_string())?
        .add_colour(&colour)
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
                config: std::sync::Mutex::new(loaded.config),
                paths,
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
            save_palette,
            rename_palette,
            delete_palette,
            rename_colour,
            delete_colour,
            recent_picks,
            save_colour,
            settings,
            cycle_setting
        ])
        .run(tauri::generate_context!())
        .expect("the Pallet window failed to start");
}
