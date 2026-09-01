//! How long one HUD frame costs on the CPU.
//!
//! The overlay must draw inside a frame callback, so anything here comes
//! straight off the budget for tracking the cursor.

use pallet_capture::frame::PixelFormat;
use pallet_capture::monitor::{ColorProfile, Transform};
use pallet_capture::{Frame, Monitor};
use pallet_color::Color;
use pallet_overlay::hud::chrome::{Chrome, HudState, Tray};
use pallet_overlay::{ChromeGpu, LoupeView, Renderer};

/// A blank 1080p frame, the size the overlay actually draws.
fn frame() -> Frame {
    let (w, h) = (1920u32, 1080u32);
    Frame {
        monitor: Monitor {
            id: "BENCH".into(),
            name: "BENCH".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: w,
            logical_height: h,
            pixel_width: w,
            pixel_height: h,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        },
        data: vec![0x80; w as usize * h as usize * 4],
        stride: w as usize * 4,
        format: PixelFormat::Bgrx8888,
    }
}

fn main() {
    let mut chrome = Chrome::new(
        1.0,
        "Click to sample · Scroll to zoom · Shift averages 5×5 · Esc cancels".into(),
    );
    let mut state = HudState {
        colour: Color::new(0xA5, 0x23, 0x6E),
        zoom: 16,
        sample: 1,
        tray: None,
        clock: 0.0,
    };

    // Warm the caches the way the first frame of a pick would.
    chrome.layers(&state, (1920, 1080), (960.0, 540.0));

    for (label, changing) in [("same colour", false), ("colour changes", true)] {
        let t = std::time::Instant::now();
        let n = 200;
        for i in 0..n {
            if changing {
                state.colour = Color::new((i * 7) as u8, (i * 13) as u8, (i * 29) as u8);
            }
            let layers = chrome.layers(&state, (1920, 1080), (960.0 + i as f32, 540.0));
            std::hint::black_box(&layers);
        }
        println!(
            "{label:>16}: {:>7.0} us/frame",
            t.elapsed().as_micros() as f64 / n as f64
        );
    }

    state.tray = Some(Tray {
        collected: vec![Color::new(1, 2, 3), Color::new(4, 5, 6)],
        target: 5,
    });
    let t = std::time::Instant::now();
    let n = 200;
    for i in 0..n {
        state.clock = i as f32 * 0.016;
        state.colour = Color::new((i * 7) as u8, (i * 13) as u8, (i * 29) as u8);
        std::hint::black_box(chrome.layers(&state, (1920, 1080), (960.0, 540.0)));
    }
    println!(
        "{:>16}: {:>7.0} us/frame",
        "with tray",
        t.elapsed().as_micros() as f64 / n as f64
    );

    // --- GPU side -------------------------------------------------------
    let Ok(renderer) = Renderer::new_headless() else {
        println!("no GPU adapter; skipping the GPU half");
        return;
    };
    let f = frame();
    let screen = renderer.create_screen(&f).expect("upload");
    let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1920,
            height: 1080,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: pallet_overlay::render::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut gpu = ChromeGpu::new();
    state.tray = None;
    let loupe = LoupeView {
        cursor: (960, 540),
        ..Default::default()
    };

    // Steady state: the panel sizes hold, so the textures are reused.
    state.colour = Color::new(200, 200, 200);
    let layers = chrome.layers(&state, (1920, 1080), (960.0, 540.0));
    renderer.draw_hud(&screen, &view, loupe, &mut gpu, &layers);
    renderer
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .ok();

    let n = 100;
    let t = std::time::Instant::now();
    for i in 0..n {
        // Same digit count, so every panel keeps its texture.
        state.colour = Color::new(200 + (i % 50) as u8, 200, 200);
        let layers = chrome.layers(&state, (1920, 1080), (960.0, 540.0));
        renderer.draw_hud(&screen, &view, loupe, &mut gpu, &layers);
        renderer
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();
    }
    println!(
        "{:>16}: {:>7.0} us/frame",
        "gpu, cached",
        t.elapsed().as_micros() as f64 / n as f64
    );

    // The readout is as wide as its RGB triple, so crossing from "9 9 9" to
    // "100 100 100" changes the panel's size and forces a new texture.
    let t = std::time::Instant::now();
    for i in 0..n {
        state.colour = if i % 2 == 0 {
            Color::new(9, 9, 9)
        } else {
            Color::new(100, 100, 100)
        };
        let layers = chrome.layers(&state, (1920, 1080), (960.0, 540.0));
        renderer.draw_hud(&screen, &view, loupe, &mut gpu, &layers);
        renderer
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();
    }
    println!(
        "{:>16}: {:>7.0} us/frame",
        "gpu, resized",
        t.elapsed().as_micros() as f64 / n as f64
    );

    // For comparison, the same frame with no HUD at all.
    let t = std::time::Instant::now();
    for _ in 0..n {
        renderer.draw(&screen, &view, loupe);
        renderer
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();
    }
    println!(
        "{:>16}: {:>7.0} us/frame",
        "gpu, no hud",
        t.elapsed().as_micros() as f64 / n as f64
    );
}
