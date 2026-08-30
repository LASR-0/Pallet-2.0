//! The picking interaction, tested against a synthetic desktop.

use pallet_capture::frame::PixelFormat;
use pallet_capture::monitor::{ColorProfile, Transform};
use pallet_capture::{Capture, Frame, Monitor};
use pallet_color::Color;
use pallet_overlay::session::{MAX_ZOOM, MIN_ZOOM};
use pallet_overlay::{Input, Outcome, Session};

/// A monitor filled with a colour that varies by position, so a wrong
/// coordinate produces a visibly wrong colour rather than a lucky match.
fn frame(id: &str, x: i32, w: u32, h: u32, profile: ColorProfile) -> Frame {
    let stride = w as usize * 4;
    let mut data = vec![0u8; stride * h as usize];
    for py in 0..h as usize {
        for px in 0..w as usize {
            let base = py * stride + px * 4;
            // BGRX: red encodes column, green encodes row.
            data[base] = 0x40;
            data[base + 1] = (py % 256) as u8;
            data[base + 2] = (px % 256) as u8;
            data[base + 3] = 0xFF;
        }
    }
    Frame {
        monitor: Monitor {
            id: id.into(),
            name: id.into(),
            logical_x: x,
            logical_y: 0,
            logical_width: w,
            logical_height: h,
            pixel_width: w,
            pixel_height: h,
            transform: Transform::Normal,
            profile,
        },
        data,
        stride,
        format: PixelFormat::Bgrx8888,
    }
}

fn desktop() -> Capture {
    Capture {
        frames: vec![
            frame("DP-2", 0, 100, 80, ColorProfile::Srgb),
            frame("DP-1", 100, 100, 80, ColorProfile::DisplayP3),
        ],
    }
}

fn session() -> Session {
    Session::new(desktop(), (10, 10), 16, 5)
}

#[test]
fn reads_the_pixel_under_the_cursor() {
    let s = session();
    // Column 10, row 10 on DP-2.
    assert_eq!(s.current_color(), Some(Color::new(10, 10, 0x40)));
}

#[test]
fn pointer_moves_track_across_the_monitor_seam() {
    let mut s = session();
    s.apply(Input::PointerTo { x: 150, y: 20 });
    // 50 columns into DP-1, row 20.
    assert_eq!(s.current_color(), Some(Color::new(50, 20, 0x40)));
    assert_eq!(s.frame().unwrap().monitor.id, "DP-1");
}

#[test]
fn a_cursor_off_the_desktop_is_pulled_onto_the_nearest_monitor() {
    // A stale pointer position, e.g. from a display since unplugged.
    let s = Session::new(desktop(), (9999, 9999), 16, 5);
    assert!(s.current_color().is_some());
    assert_eq!(s.cursor(), (199, 79));
}

#[test]
fn nudging_moves_exactly_one_pixel() {
    let mut s = session();
    s.apply(Input::Nudge { dx: 1, dy: 0 });
    assert_eq!(s.cursor(), (11, 10));
    s.apply(Input::Nudge { dx: 0, dy: -1 });
    assert_eq!(s.cursor(), (11, 9));
    assert_eq!(s.current_color(), Some(Color::new(11, 9, 0x40)));
}

#[test]
fn nudging_off_the_desktop_holds_position() {
    let mut s = Session::new(desktop(), (0, 0), 16, 5);
    s.apply(Input::Nudge { dx: -1, dy: 0 });
    assert_eq!(s.cursor(), (0, 0), "should not slide along the edge");
    s.apply(Input::Nudge { dx: 0, dy: -5 });
    assert_eq!(s.cursor(), (0, 0));
}

#[test]
fn nudging_can_cross_between_monitors() {
    let mut s = Session::new(desktop(), (99, 40), 16, 5);
    assert_eq!(s.frame().unwrap().monitor.id, "DP-2");
    s.apply(Input::Nudge { dx: 1, dy: 0 });
    assert_eq!(s.frame().unwrap().monitor.id, "DP-1");
    assert_eq!(s.cursor(), (100, 40));
}

#[test]
fn zoom_steps_and_saturates_at_both_ends() {
    let mut s = session();
    assert_eq!(s.zoom(), 16);
    s.apply(Input::ZoomIn);
    assert_eq!(s.zoom(), 32);
    for _ in 0..10 {
        s.apply(Input::ZoomIn);
    }
    assert_eq!(s.zoom(), MAX_ZOOM);
    for _ in 0..10 {
        s.apply(Input::ZoomOut);
    }
    assert_eq!(s.zoom(), MIN_ZOOM);
}

#[test]
fn a_requested_zoom_outside_the_range_is_clamped_at_construction() {
    assert_eq!(Session::new(desktop(), (0, 0), 999, 5).zoom(), MAX_ZOOM);
    assert_eq!(Session::new(desktop(), (0, 0), 0, 5).zoom(), MIN_ZOOM);
}

/// A frame split down the middle into black and white, for testing the
/// averaging window where it actually matters: a hard edge.
fn split_frame() -> Capture {
    let (w, h) = (40u32, 40u32);
    let stride = w as usize * 4;
    let mut data = vec![0u8; stride * h as usize];
    for py in 0..h as usize {
        for px in 0..w as usize {
            let base = py * stride + px * 4;
            let v = if px < 20 { 0x00 } else { 0xFF };
            data[base..base + 4].copy_from_slice(&[v, v, v, 0xFF]);
        }
    }
    Capture {
        frames: vec![Frame {
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
        }],
    }
}

#[test]
fn averaging_is_reversible_and_leaves_flat_areas_alone() {
    // Over a uniform region the mean is the pixel, in any colour space.
    let mut s = Session::new(split_frame(), (5, 20), 16, 5);
    let single = s.current_color();
    assert_eq!(single, Some(Color::new(0, 0, 0)));

    s.apply(Input::Averaging(true));
    assert!(s.is_averaging());
    assert_eq!(s.sample_size(), 5);
    assert_eq!(
        s.current_color(),
        single,
        "a flat area must average to itself"
    );

    s.apply(Input::Averaging(false));
    assert_eq!(s.sample_size(), 1);
    assert_eq!(s.current_color(), single);
}

#[test]
fn averaging_across_a_hard_edge_blends_in_linear_light() {
    // Centred on the last black column, so the 5-wide window covers columns
    // 17..=21: three black, two white.
    //
    //   linear mean   = (3*0 + 2*1) / 5           = 0.4
    //   encoded       = 1.055 * 0.4^(1/2.4) - .055 = 0.665 -> 170 = 0xAA
    //   naive byte mean = (3*0 + 2*255) / 5        = 102 = 0x66
    //
    // Averaging encoded bytes rather than linear light biases every sample
    // dark, and a high-contrast edge is exactly where someone reaches for it.
    let mut s = Session::new(split_frame(), (19, 20), 16, 5);
    assert_eq!(s.current_color(), Some(Color::new(0, 0, 0)));

    s.apply(Input::Averaging(true));
    let averaged = s.current_color().expect("a colour under the cursor");
    assert_eq!(
        averaged,
        Color::new(0xAA, 0xAA, 0xAA),
        "expected the linear-light mean, not the naive byte mean (#666666)"
    );
}

#[test]
fn an_even_average_size_is_pushed_odd() {
    // A 4x4 window has no centre pixel for the loupe to anchor on.
    let mut s = Session::new(desktop(), (10, 10), 16, 4);
    s.apply(Input::Averaging(true));
    assert_eq!(s.sample_size(), 5);
}

#[test]
fn committing_returns_the_colour_and_where_it_came_from() {
    let mut s = session();
    s.apply(Input::PointerTo { x: 150, y: 20 });
    s.apply(Input::Commit);

    match s.outcome() {
        Some(Outcome::Picked {
            color,
            at,
            source_space,
            save,
        }) => {
            assert_eq!(*color, Color::new(50, 20, 0x40));
            assert_eq!(*at, (150, 20));
            // The wide-gamut provenance must survive to the library.
            assert_eq!(source_space.as_deref(), Some("display-p3"));
            assert!(!save, "a plain commit copies but does not keep");
        }
        other => panic!("expected a pick, got {other:?}"),
    }
}

#[test]
fn an_srgb_monitor_records_no_profile_tag() {
    let mut s = session();
    s.apply(Input::Commit);
    match s.outcome() {
        Some(Outcome::Picked { source_space, .. }) => assert_eq!(*source_space, None),
        other => panic!("expected a pick, got {other:?}"),
    }
}

#[test]
fn cancelling_yields_no_colour() {
    let mut s = session();
    s.apply(Input::Cancel);
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
    assert!(s.is_finished());
}

#[test]
fn input_after_the_session_ends_is_ignored() {
    // A stray key between commit and teardown must not alter the result.
    let mut s = session();
    s.apply(Input::Commit);
    let settled = s.outcome().cloned();

    s.apply(Input::PointerTo { x: 150, y: 20 });
    s.apply(Input::Cancel);
    s.apply(Input::ZoomIn);

    assert_eq!(s.outcome().cloned(), settled);
    assert_eq!(s.cursor(), (10, 10));
    assert_eq!(s.zoom(), 16);
}

#[test]
fn committing_with_nothing_under_the_cursor_cancels_rather_than_inventing_a_colour() {
    let mut s = Session::new(Capture::default(), (0, 0), 16, 5);
    s.apply(Input::Commit);
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
}

#[test]
fn committing_with_save_asks_for_the_colour_to_be_kept() {
    // "S" in the loupe means take it *and* keep it, so the user does not have
    // to come back to the window to file a colour they already decided on.
    let mut s = session();
    s.apply(Input::CommitAndSave);

    match s.outcome() {
        Some(Outcome::Picked { save, color, .. }) => {
            assert!(save);
            assert_eq!(*color, Color::new(10, 10, 0x40));
        }
        other => panic!("expected a saved pick, got {other:?}"),
    }
}

#[test]
fn a_save_commit_with_nothing_under_the_cursor_still_cancels() {
    let mut s = Session::new(Capture::default(), (0, 0), 16, 5);
    s.apply(Input::CommitAndSave);
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
}
