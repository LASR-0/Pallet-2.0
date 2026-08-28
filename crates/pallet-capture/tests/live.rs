//! Tests that need a real display server.
//!
//! These skip themselves when no compositor is reachable, so CI (which has no
//! session) stays green while a developer machine still exercises the real
//! path. A capture backend cannot be meaningfully faked: the value of these
//! tests is precisely that they talk to the compositor.

use pallet_capture::ScreenCapture;

/// Returns a backend, or `None` when this machine has no display session.
fn backend() -> Option<Box<dyn ScreenCapture>> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
        eprintln!("skipping: no WAYLAND_DISPLAY or DISPLAY");
        return None;
    }
    match pallet_capture::open() {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("skipping: no usable backend ({e})");
            None
        }
    }
}

#[test]
fn monitors_are_reported_in_physical_pixels() {
    let Some(mut capture) = backend() else { return };
    let monitors = capture.monitors().expect("monitor enumeration");

    assert!(
        !monitors.is_empty(),
        "a session must have at least one output"
    );
    for m in &monitors {
        assert!(
            m.pixel_width > 0 && m.pixel_height > 0,
            "{} had no framebuffer size",
            m.id
        );
        assert!(
            m.logical_width > 0 && m.logical_height > 0,
            "{} had no logical size",
            m.id
        );
        assert!(m.scale_x() > 0.0, "{} had a non-positive scale", m.id);
        assert!(!m.id.is_empty(), "every monitor needs an id");
    }
}

#[test]
fn monitors_do_not_overlap() {
    // Overlapping bounds would make pixel_at ambiguous at the seam.
    let Some(mut capture) = backend() else { return };
    let monitors = capture.monitors().expect("monitor enumeration");

    for (i, a) in monitors.iter().enumerate() {
        for b in &monitors[i + 1..] {
            let disjoint = a.logical_x + (a.logical_width as i32) <= b.logical_x
                || b.logical_x + (b.logical_width as i32) <= a.logical_x
                || a.logical_y + (a.logical_height as i32) <= b.logical_y
                || b.logical_y + (b.logical_height as i32) <= a.logical_y;
            assert!(disjoint, "{} and {} overlap", a.id, b.id);
        }
    }
}

#[test]
fn a_capture_covers_every_monitor_with_a_correctly_sized_buffer() {
    let Some(mut capture) = backend() else { return };
    let shot = capture.capture_all().expect("capture");

    assert_eq!(
        shot.frames.len(),
        capture.monitors().expect("monitors").len(),
        "every monitor should produce a frame"
    );

    for frame in &shot.frames {
        let m = &frame.monitor;
        assert!(
            frame.stride >= m.pixel_width as usize * 4,
            "{} stride {} is narrower than its width",
            m.id,
            frame.stride
        );
        assert!(
            frame.data.len() >= frame.stride * m.pixel_height as usize,
            "{} buffer is short of its declared geometry",
            m.id
        );
        // Corners must be readable; these are the indices most likely to be
        // wrong if stride handling is broken.
        assert!(frame.pixel(0, 0).is_some());
        assert!(frame.pixel(m.pixel_width - 1, m.pixel_height - 1).is_some());
        assert!(
            frame.pixel(m.pixel_width, 0).is_none(),
            "width is exclusive"
        );
    }
}

#[test]
fn every_pixel_of_the_desktop_is_addressable_and_beyond_it_is_not() {
    let Some(mut capture) = backend() else { return };
    let shot = capture.capture_all().expect("capture");
    let Some((min_x, min_y, max_x, max_y)) = shot.bounds() else {
        return;
    };

    // Each monitor's own corners, via global coordinates.
    for frame in &shot.frames {
        let m = &frame.monitor;
        for (x, y) in [
            (m.logical_x, m.logical_y),
            (m.logical_x + m.logical_width as i32 - 1, m.logical_y),
            (m.logical_x, m.logical_y + m.logical_height as i32 - 1),
            (
                m.logical_x + m.logical_width as i32 - 1,
                m.logical_y + m.logical_height as i32 - 1,
            ),
        ] {
            assert!(shot.pixel_at(x, y).is_ok(), "{} corner ({x},{y})", m.id);
        }
    }

    // Outside the desktop must be a clean error, never a panic or a wrong
    // colour read from a neighbouring monitor's buffer.
    assert!(shot.pixel_at(max_x + 1, min_y).is_err());
    assert!(shot.pixel_at(min_x - 1, min_y).is_err());
    assert!(shot.pixel_at(min_x, max_y + 1).is_err());
}

#[test]
fn averaging_at_a_desktop_corner_clips_instead_of_failing() {
    let Some(mut capture) = backend() else { return };
    let shot = capture.capture_all().expect("capture");
    let Some((min_x, min_y, _, _)) = shot.bounds() else {
        return;
    };

    // Most of a 5x5 window at the very corner lies off-screen.
    assert!(
        shot.average_at(min_x, min_y, 5).is_ok(),
        "a corner average must clip rather than fail"
    );
}

#[test]
fn a_single_monitor_capture_matches_that_monitor_in_the_full_capture() {
    let Some(mut capture) = backend() else { return };
    let monitors = capture.monitors().expect("monitors");
    let Some(first) = monitors.first() else {
        return;
    };

    let frame = capture.capture_monitor(&first.id).expect("single capture");
    assert_eq!(frame.monitor.id, first.id);
    assert_eq!(frame.monitor.pixel_width, first.pixel_width);
    assert_eq!(frame.monitor.pixel_height, first.pixel_height);
}

#[test]
fn an_unknown_monitor_id_is_an_error() {
    let Some(mut capture) = backend() else { return };
    assert!(capture.capture_monitor("NO-SUCH-OUTPUT-1").is_err());
}

#[test]
fn the_backend_can_be_reused_for_repeated_captures() {
    // The picker keeps one backend warm for the life of the process, so
    // capturing twice in a row must not leak state or wedge the queue.
    let Some(mut capture) = backend() else { return };
    for i in 0..3 {
        let shot = capture
            .capture_all()
            .unwrap_or_else(|e| panic!("capture {i}: {e}"));
        assert!(!shot.frames.is_empty(), "capture {i} produced no frames");
    }
}
