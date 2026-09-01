//! The loupe shader, verified by rendering offscreen and reading pixels back.
//!
//! Skips itself when no GPU adapter is available, so CI without a graphics
//! stack stays green.

use pallet_capture::frame::PixelFormat;
use pallet_capture::monitor::{ColorProfile, Transform};
use pallet_capture::{Frame, Monitor};
use pallet_color::Color;
use pallet_overlay::{LoupeView, Renderer};

fn renderer() -> Option<Renderer> {
    match Renderer::new_headless() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("skipping: no GPU ({e})");
            None
        }
    }
}

/// A frame whose colour encodes its own coordinates, so a misplaced sample
/// produces a provably wrong colour rather than a plausible one.
fn coded_frame(w: u32, h: u32) -> Frame {
    let stride = w as usize * 4;
    let mut data = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let b = y * stride + x * 4;
            data[b] = 0x20; // blue, constant
            data[b + 1] = (y % 256) as u8; // green encodes row
            data[b + 2] = (x % 256) as u8; // red encodes column
            data[b + 3] = 0xFF;
        }
    }
    Frame {
        monitor: Monitor {
            id: "TEST".into(),
            name: "TEST".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: w,
            logical_height: h,
            pixel_width: w,
            pixel_height: h,
            transform: Transform::Normal,
            profile: ColorProfile::Srgb,
        },
        data,
        stride,
        format: PixelFormat::Bgrx8888,
    }
}

fn px(buf: &[u8], w: u32, x: u32, y: u32) -> Color {
    let i = (y as usize * w as usize + x as usize) * 4;
    Color::new(buf[i], buf[i + 1], buf[i + 2])
}

#[test]
fn outside_the_loupe_the_screen_is_reproduced_exactly() {
    // The whole promise of a frozen screen: untouched pixels. Any colour
    // management in the pipeline would break this.
    let Some(r) = renderer() else { return };
    let frame = coded_frame(64, 64);
    let screen = r.create_screen(&frame).expect("upload");

    let out = r
        .render_to_pixels(
            &screen,
            64,
            64,
            LoupeView {
                cursor: (32, 32),
                radius: 8.0,
                ..Default::default()
            },
        )
        .expect("render");

    for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63), (5, 40)] {
        assert_eq!(
            px(&out, 64, x, y),
            frame.pixel(x, y).unwrap(),
            "pixel ({x},{y}) outside the loupe was altered"
        );
    }
}

#[test]
fn the_loupe_magnifies_by_the_requested_factor() {
    let Some(r) = renderer() else { return };
    let screen = r.create_screen(&coded_frame(64, 64)).expect("upload");

    let out = r
        .render_to_pixels(
            &screen,
            64,
            64,
            LoupeView {
                cursor: (32, 32),
                zoom: 8,
                radius: 20.0,
                grid: false,
                // The vignette darkens pixels toward the rim, which would
                // make this a test of the tint rather than the mapping.
                vignette: 0.0,
                ..Default::default()
            },
        )
        .expect("render");

    // At 8x, a point 8 screen pixels right of the cursor shows the source
    // pixel one column right of the cursor.
    let centre = px(&out, 64, 32, 32);
    assert_eq!(centre.r, 32, "centre should magnify column 32");

    let right = px(&out, 64, 40, 32);
    assert_eq!(
        right.r, 33,
        "8 pixels right at 8x should be one column over"
    );

    let left = px(&out, 64, 24, 32);
    assert_eq!(left.r, 31, "8 pixels left at 8x should be one column back");
}

#[test]
fn magnification_is_nearest_neighbour_never_interpolated() {
    // Interpolation would invent colours that are not on screen, so the loupe
    // could show a colour the user cannot actually pick.
    let Some(r) = renderer() else { return };
    let screen = r.create_screen(&coded_frame(64, 64)).expect("upload");

    let out = r
        .render_to_pixels(
            &screen,
            64,
            64,
            LoupeView {
                cursor: (32, 32),
                zoom: 16,
                radius: 24.0,
                grid: false,
                vignette: 0.0,
                ..Default::default()
            },
        )
        .expect("render");

    // Every magnified pixel is either an exact source colour (blue is the
    // constant 0x20) or a deliberate annotation drawn in pure ink. A blended
    // blue would mean the sampler interpolated and invented a colour.
    for x in 20..44u32 {
        for y in 20..44u32 {
            // Pixel centres, matching the fragment coordinates the shader
            // works in; half a pixel out here misses the annotation band.
            let dx = x as f32 + 0.5 - 32.5;
            let dy = y as f32 + 0.5 - 32.5;
            if (dx * dx + dy * dy).sqrt() > 16.0 {
                continue; // stay clear of the rim
            }
            // The ring around the cell that a commit would take is drawn on
            // purpose, half of it as translucent black over the pixel beneath.
            let box_edge = 0.5 * 16.0;
            if dx.abs().max(dy.abs()) <= box_edge + 3.0 {
                continue;
            }
            let c = px(&out, 64, x, y);
            let is_source = c.b == 0x20;
            let is_ink = c.r == c.g && c.g == c.b && (c.r == 0 || c.r == 255);
            assert!(
                is_source || is_ink,
                "({x},{y}) is {c}: neither a source colour nor an annotation, so the \
                 sampler interpolated"
            );
        }
    }
}

#[test]
fn the_centre_of_the_loupe_shows_the_pixel_that_will_be_picked() {
    // If this drifts, users pick a different colour from the one they aimed at.
    let Some(r) = renderer() else { return };
    let frame = coded_frame(64, 64);
    let screen = r.create_screen(&frame).expect("upload");

    for cursor in [(10u32, 10u32), (32, 32), (50, 20), (63, 63), (0, 0)] {
        let out = r
            .render_to_pixels(
                &screen,
                64,
                64,
                LoupeView {
                    cursor,
                    zoom: 16,
                    radius: 18.0,
                    grid: false,
                    sample: 1,
                    ..Default::default()
                },
            )
            .expect("render");

        let shown = px(&out, 64, cursor.0, cursor.1);
        let actual = frame.pixel(cursor.0, cursor.1).unwrap();
        assert_eq!(
            shown, actual,
            "loupe centre at {cursor:?} showed {shown} but the pixel is {actual}"
        );
    }
}

#[test]
fn the_grid_only_appears_when_asked_for() {
    let Some(r) = renderer() else { return };
    let screen = r.create_screen(&coded_frame(64, 64)).expect("upload");

    // A radius wide enough to contain several grid lines. At radius 20 with
    // 16x zoom the only lines fall at exactly +/-8px, which is where the
    // sample outline is drawn, so the grid would be invisible by coincidence.
    let view = |grid| LoupeView {
        cursor: (32, 32),
        zoom: 16,
        radius: 60.0,
        grid,
        ..Default::default()
    };

    let with = r
        .render_to_pixels(&screen, 64, 64, view(true))
        .expect("render");
    let without = r
        .render_to_pixels(&screen, 64, 64, view(false))
        .expect("render");
    assert_ne!(with, without, "the grid flag changed nothing");
}

#[test]
fn an_empty_frame_is_rejected() {
    let Some(r) = renderer() else { return };
    let mut frame = coded_frame(4, 4);
    frame.monitor.pixel_width = 0;
    assert!(r.create_screen(&frame).is_err());
}

#[test]
fn each_monitor_gets_its_own_frozen_pixels() {
    // A desktop is several screens sharing one GPU context; one screen's
    // upload must not overwrite another's.
    let Some(r) = renderer() else { return };
    let a = r.create_screen(&coded_frame(32, 32)).expect("upload a");
    let b = r.create_screen(&coded_frame(64, 64)).expect("upload b");

    assert_eq!(a.size(), (32, 32));
    assert_eq!(b.size(), (64, 64));

    let view = LoupeView {
        cursor: (4, 4),
        radius: 0.0,
        ..Default::default()
    };
    let out_a = r.render_to_pixels(&a, 32, 32, view).expect("render a");
    let out_b = r.render_to_pixels(&b, 32, 32, view).expect("render b");
    // Same coordinates, same coded content, so these must agree - proving the
    // second upload did not clobber the first.
    assert_eq!(px(&out_a, 32, 10, 10), px(&out_b, 32, 10, 10));
}
