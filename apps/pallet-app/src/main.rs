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

/// Push settings that affect the window itself onto the window.
///
/// `stay_on_top` was stored and displayed but never applied, so the toggle
/// did nothing at all.
fn apply_window_settings(window: &tauri::WebviewWindow, config: &Config) {
    let _ = window.set_always_on_top(config.general.stay_on_top);
}

/// Whether the desktop draws its own window corners.
///
/// Tiling compositors round windows themselves, so the app drawing its own
/// radius on top gives a visible double corner at a different curvature. On a
/// floating desktop — or Windows — nothing rounds the window for us and the
/// design's radius is what makes it look like a panel rather than a box.
#[tauri::command]
fn compositor_rounds_windows() -> bool {
    use pallet_hotkey::Compositor;
    matches!(
        Compositor::detect(),
        Compositor::Hyprland | Compositor::Sway | Compositor::River
    )
}

/// One remappable command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Binding {
    key: String,
    label: String,
    hint: String,
    combo: String,
    /// True for the three the picker reads while the loupe is up.
    in_loupe: bool,
}

/// Every binding, for the Settings screen.
#[tauri::command]
fn bindings(state: tauri::State<'_, AppState>) -> Result<Vec<Binding>, String> {
    let c = state
        .config
        .lock()
        .map_err(|_| "settings are unavailable".to_string())?;
    let k = &c.keys;

    let row = |key: &str, label: &str, hint: &str, combo: &str, in_loupe: bool| Binding {
        key: key.into(),
        label: label.into(),
        hint: hint.into(),
        combo: combo.into(),
        in_loupe,
    };

    Ok(vec![
        row(
            "pick",
            "Pick a colour",
            "While the window has focus",
            &k.pick,
            false,
        ),
        row(
            "theme",
            "Switch theme",
            "Sketchbook or Studio",
            &k.theme,
            false,
        ),
        row(
            "search",
            "Search the library",
            "Palettes and Colours",
            &k.search,
            false,
        ),
        row(
            "save_palette",
            "Save palette",
            "On the Build screen",
            &k.save_palette,
            false,
        ),
        row(
            "save_colour",
            "Keep this colour",
            "Adds the Current colour to the library",
            &k.save_colour,
            false,
        ),
        row(
            "loupe_commit",
            "Take the colour",
            "In the loupe",
            &k.loupe_commit,
            true,
        ),
        row(
            "loupe_save",
            "Take and keep it",
            "In the loupe, adds to the library",
            &k.loupe_save,
            true,
        ),
        row(
            "loupe_cancel",
            "Abandon the pick",
            "In the loupe",
            &k.loupe_cancel,
            true,
        ),
    ])
}

/// Remap a command.
#[tauri::command]
fn set_binding(
    key: String,
    combo: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let combo = combo.trim();
    if combo.is_empty() {
        return Err("a binding needs at least one key".into());
    }

    let mut c = state
        .config
        .lock()
        .map_err(|_| "settings are unavailable".to_string())?;

    let slot = match key.as_str() {
        "pick" => &mut c.keys.pick,
        "theme" => &mut c.keys.theme,
        "search" => &mut c.keys.search,
        "save_palette" => &mut c.keys.save_palette,
        "save_colour" => &mut c.keys.save_colour,
        "loupe_commit" => &mut c.keys.loupe_commit,
        "loupe_save" => &mut c.keys.loupe_save,
        "loupe_cancel" => &mut c.keys.loupe_cancel,
        other => return Err(format!("`{other}` is not a bindable command")),
    };
    *slot = combo.to_string();

    c.save(&state.paths.config_file())
        .map_err(|e| e.to_string())
}

/// Advance one setting to its next value and save.
///
/// Every control cycles rather than opening an editor: the values are all
/// short lists or toggles, and one interaction model keeps the row compact
/// enough to match the prototype's design.
#[tauri::command]
fn cycle_setting(
    key: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
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

    // Stay-on-top has to reach the window, not just the file.
    apply_window_settings(&window, &c);
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

    // Beside this binary, so a build tree and an installed bundle both work.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let picker = exe
        .parent()
        .ok_or("could not locate the picker binary")?
        .join("pallet-picker");
    if !picker.exists() {
        return Err(format!("the picker is missing from {}", picker.display()));
    }

    // Probe on a connection of its own, and let it close before opening the
    // one that carries the pick. The picker serves a single connection at a
    // time, so holding one open while opening a second deadlocks: the second
    // is never accepted, and the request on it is never read.
    match probe_picker(&pallet_ipc::transport::socket_path(), &picker) {
        Probe::Usable => {
            return UnixStream::connect(pallet_ipc::transport::socket_path())
                .map_err(|e| format!("could not reach the picker: {e}"));
        }
        // The picker outlives rebuilds, so a running one may predate the
        // binary now on disk and would answer with the previous version's
        // behaviour. Retiring it costs one start-up; not retiring it costs an
        // afternoon wondering why a change had no effect.
        Probe::Stale => {
            tracing::info!("the running picker is from an older build; restarting it");
            retire_picker(&pallet_ipc::transport::socket_path());
        }
        Probe::Absent => {}
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
    let socket = pallet_ipc::transport::socket_path();
    for _ in 0..100 {
        if let Ok(stream) = UnixStream::connect(&socket) {
            return Ok(stream);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    Err("the picker did not start in time".into())
}

/// What a probe of the picker's socket found.
#[cfg(unix)]
enum Probe {
    /// Nothing is listening.
    Absent,
    /// A picker is listening and matches the binary on disk.
    Usable,
    /// A picker is listening but was built before the binary on disk.
    Stale,
}

/// Ask the running picker what build it is, on a connection of its own.
#[cfg(unix)]
fn probe_picker(socket: &std::path::Path, binary: &std::path::Path) -> Probe {
    let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) else {
        return Probe::Absent;
    };
    // A wedged picker must not hang the window; treat silence as usable and
    // let the pick itself surface the problem.
    let timeout = std::time::Duration::from_secs(2);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    if pallet_ipc::write_message(&mut stream, &pallet_ipc::Request::Ping).is_err() {
        return Probe::Usable;
    }
    match pallet_ipc::read_message::<_, pallet_ipc::Response>(&mut stream) {
        Ok(pallet_ipc::Response::Pong { build, .. }) => {
            let want = pallet_ipc::transport::build_stamp(binary);
            // A zero on either side means "unknown", which is not a mismatch.
            if build == 0 || want == 0 || build == want {
                Probe::Usable
            } else {
                Probe::Stale
            }
        }
        // Not a picker we can reason about; leave it alone.
        _ => Probe::Usable,
    }
}

/// Ask the running picker to exit, and wait for its socket to go quiet.
#[cfg(unix)]
fn retire_picker(socket: &std::path::Path) {
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let _ = pallet_ipc::write_message(&mut stream, &pallet_ipc::Request::Shutdown);
        let _ = pallet_ipc::read_message::<_, pallet_ipc::Response>(&mut stream);
    }
    for _ in 0..40 {
        if std::os::unix::net::UnixStream::connect(socket).is_err() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Ask the resident picker for one colour, or for a whole palette.
///
/// `held` turns the pick into a palette pass: the overlay stays up until the
/// tray fills, and returns everything gathered. Picking a palette one colour
/// per request would release and re-freeze the screen between every one, which
/// makes choosing colours that go together impossible.
///
/// Async because a pick lasts as long as the user takes: on a blocking command
/// Tauri would hold a worker thread and the window would stop responding until
/// they clicked.
#[tauri::command]
async fn pick_colour(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    held: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    // The loupe magnification and the Shift-average size are settings. Sending
    // the defaults instead — which this did — made both appear to do nothing.
    let options = {
        let c = state
            .config
            .lock()
            .map_err(|_| "settings are unavailable".to_string())?;
        pallet_ipc::PickOptions {
            zoom: Some(u32::from(c.picker.loupe_zoom)),
            average_size: Some(u32::from(c.picker.average_size)),
            keys: Some(pallet_ipc::LoupeKeys {
                commit: c.keys.loupe_commit.clone(),
                save: c.keys.loupe_save.clone(),
                cancel: c.keys.loupe_cancel.clone(),
            }),
            // The palette's length is the multi-pick setting, not the Build
            // screen's 25-colour ceiling — the setting is what "one pass"
            // means. It stretches if the user keeps going past it.
            palette: held.map(|collected| {
                let intended = usize::from(c.picker.multi_pick_length).max(1);
                pallet_ipc::PaletteRequest {
                    target: intended.max(collected.len() + 1).min(MAX_PALETTE),
                    collected,
                }
            }),
        }
    };

    // Get out of the way before the screen is frozen. Otherwise the picker
    // captures this window sitting on top of whatever the user meant to pick
    // from, and the frozen image is mostly Pallet.
    let hidden = window.hide().is_ok();

    let result: Result<Vec<pallet_ipc::TakenColour>, String> =
        tauri::async_runtime::spawn_blocking(move || {
            let mut stream = connect_to_picker()?;

            pallet_ipc::write_message(&mut stream, &pallet_ipc::Request::Pick(options))
                .map_err(|e| e.to_string())?;

            match pallet_ipc::read_message::<_, pallet_ipc::Response>(&mut stream)
                .map_err(|e| e.to_string())?
            {
                pallet_ipc::Response::Picked {
                    hex,
                    at,
                    source_space,
                    ..
                } => Ok(vec![pallet_ipc::TakenColour {
                    hex,
                    at,
                    source_space,
                }]),
                pallet_ipc::Response::PickedSet { colours } => Ok(colours),
                pallet_ipc::Response::Cancelled => Ok(Vec::new()),
                pallet_ipc::Response::Error { message } => Err(message),
                pallet_ipc::Response::Pong { .. } => Err("unexpected reply".into()),
            }
        })
        .await
        .map_err(|e| e.to_string())?;

    // Come back whether the pick succeeded, was cancelled, or failed.
    if hidden {
        let _ = window.show();
        let _ = window.set_focus();
    }

    let taken = result?;

    // Every pick goes into the history, including each colour of a palette
    // pass. Only the CLI did this, so picking from the window left the Pick
    // screen showing the same handful of colours for ever.
    if !taken.is_empty()
        && let Ok(store) = state.store.lock()
    {
        for colour in &taken {
            let Ok(parsed) = pallet_color::Color::parse_hex(&colour.hex) else {
                continue;
            };
            // A pick that cannot be filed is not a pick that failed: the user
            // has their colour either way, so this never fails the command.
            if let Err(e) = store.record_pick(parsed, colour.source_space.as_deref(), None) {
                tracing::warn!("could not record the pick: {e}");
            }
        }
    }

    Ok(taken.into_iter().map(|c| c.hex).collect())
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

/// The export formats, for the Build screen's chips.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportFormat {
    id: String,
    label: String,
}

/// Which formats can be written.
#[tauri::command]
fn export_formats() -> Vec<ExportFormat> {
    pallet_export::Format::ALL
        .into_iter()
        .map(|f| ExportFormat {
            id: f.id().into(),
            label: f.label().into(),
        })
        .collect()
}

/// Write a palette to the exports directory.
///
/// Written to a known folder rather than through a file dialog: the path is in
/// the plan, it needs no extra permission, and a picker interrupting the flow
/// to ask "where?" every time is worse than telling the user where it went.
#[tauri::command]
fn export_palette(
    name: String,
    hexes: Vec<String>,
    format: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let format = pallet_export::Format::parse(&format)
        .ok_or_else(|| format!("`{format}` is not a format Pallet writes"))?;

    let swatches = hexes
        .iter()
        .map(|hex| {
            Color::parse_hex(hex)
                .map(pallet_export::Swatch::new)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;

    let palette = pallet_export::Palette::new(name.trim(), swatches).with_suggested_names();
    let bytes = pallet_export::write(&palette, format).map_err(|e| e.to_string())?;

    let dir = state.paths.exports_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let stem = pallet_export::model::slug(&palette.name, 0);
    let path = dir.join(format!("{stem}.{}", format.extension()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;

    Ok(path.display().to_string())
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

            if let Some(window) = app.get_webview_window("main") {
                apply_window_settings(&window, &loaded.config);
            }

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
            cycle_setting,
            export_formats,
            export_palette,
            compositor_rounds_windows,
            bindings,
            set_binding
        ])
        .run(tauri::generate_context!())
        .expect("the Pallet window failed to start");
}
