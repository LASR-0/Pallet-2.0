//! Render one HUD frame offscreen and write it to a PNG.
//!
//! The overlay only exists on a live compositor, which makes comparing it with
//! `Prototype/Pallet Pick.dc.html` awkward. This draws the same frame the
//! picker would, over the design file's own backdrop, so the two can be put
//! side by side.
//!
//! ```text
//! cargo run -p pallet-overlay --example hud_preview -- out.png
//! ```

use pallet_capture::frame::PixelFormat;
use pallet_capture::monitor::{ColorProfile, Transform};
use pallet_capture::{Frame, Monitor};
use pallet_color::Color;
use pallet_overlay::hud::chrome::{Chrome, HudState, Tray};
use pallet_overlay::{ChromeGpu, LoupeView, Renderer};

/// The design's preview canvas.
const WIDTH: u32 = 900;
const HEIGHT: u32 = 506;

/// `linear-gradient(160deg,#2b3a55 0%,#6b4b6e 45%,#c9736a 78%,#e8b98a 100%)`.
const STOPS: [(f32, [u8; 3]); 4] = [
    (0.00, [0x2b, 0x3a, 0x55]),
    (0.45, [0x6b, 0x4b, 0x6e]),
    (0.78, [0xc9, 0x73, 0x6a]),
    (1.00, [0xe8, 0xb9, 0x8a]),
];

fn ramp(t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    for pair in STOPS.windows(2) {
        let (t0, a) = pair[0];
        let (t1, b) = pair[1];
        if t <= t1 {
            let k = (t - t0) / (t1 - t0);
            return std::array::from_fn(|i| {
                (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * k).round() as u8
            });
        }
    }
    STOPS[3].1
}

fn backdrop() -> Frame {
    let stride = WIDTH as usize * 4;
    let mut data = vec![0u8; stride * HEIGHT as usize];
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let u = x as f32 / WIDTH as f32;
            let v = y as f32 / HEIGHT as f32;
            // The design's own sampling function, grain included, so the
            // magnified cells have something to actually magnify.
            let axis = u * 0.34 + v * 0.94;
            let grain = (u * 22.0).sin() * 0.012 + (v * 17.0).sin() * 0.014;
            let c = ramp(axis * 0.92 + grain + 0.04);

            let b = y * stride + x * 4;
            data[b] = c[2];
            data[b + 1] = c[1];
            data[b + 2] = c[0];
            data[b + 3] = 0xFF;
        }
    }
    Frame {
        monitor: Monitor {
            id: "PREVIEW".into(),
            name: "PREVIEW".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: WIDTH,
            logical_height: HEIGHT,
            pixel_width: WIDTH,
            pixel_height: HEIGHT,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        },
        data,
        stride,
        format: PixelFormat::Bgrx8888,
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or("hud.png".into());
    let with_tray = std::env::args().any(|a| a == "--tray");

    let renderer = Renderer::new_headless().expect("no GPU adapter available");
    let frame = backdrop();
    let screen = renderer.create_screen(&frame).expect("upload");

    // The design's cursor: 44% across, 56% down.
    let cursor = ((0.44 * WIDTH as f32) as u32, (0.56 * HEIGHT as f32) as u32);
    let colour = frame.pixel(cursor.0, cursor.1).expect("in bounds");

    let mut chrome = Chrome::new(
        1.0,
        "Click to sample · Scroll to zoom · Shift averages 5×5 · Esc cancels".into(),
    );
    let state = HudState {
        colour,
        zoom: 16,
        sample: 1,
        tray: with_tray.then(|| Tray {
            collected: vec![
                Color::new(0xF0, 0xA9, 0xA0),
                Color::new(0xC9, 0x73, 0x6A),
                Color::new(0x6B, 0x4B, 0x6E),
            ],
            target: 5,
        }),
        clock: 0.6,
    };
    let centre = (cursor.0 as f32 + 0.5, cursor.1 as f32 + 0.5);
    let layers = chrome.layers(&state, (WIDTH, HEIGHT), centre);

    let mut gpu = ChromeGpu::new();
    let pixels = renderer
        .render_hud_to_pixels(
            &screen,
            WIDTH,
            HEIGHT,
            LoupeView {
                cursor,
                ..Default::default()
            },
            &mut gpu,
            &layers,
        )
        .expect("render");

    let mut rgb = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 3);
    for chunk in pixels.chunks_exact(4) {
        rgb.extend_from_slice(&chunk[..3]);
    }
    image::save_buffer(&path, &rgb, WIDTH, HEIGHT, image::ColorType::Rgb8).expect("write png");
    println!("wrote {path}");
}
