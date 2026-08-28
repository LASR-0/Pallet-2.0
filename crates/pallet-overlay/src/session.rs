//! The picking interaction, as a pure state machine.
//!
//! Deliberately free of GPU and compositor code so the behaviour that decides
//! whether a pick is *correct* — which pixel the cursor is over, what Shift
//! does, where the loupe may travel — is testable without a display server.
//! The renderer reads this state; it never owns it.

use pallet_capture::{Capture, Frame};
use pallet_color::Color;

/// Magnification limits. Below 2x a loupe is pointless; above 64x a single
/// pixel fills so much of the screen that aiming becomes harder, not easier.
pub const MIN_ZOOM: u32 = 2;
/// Upper magnification limit.
pub const MAX_ZOOM: u32 = 64;

/// What the user did to the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The pointer moved to a logical desktop position.
    PointerTo {
        /// Logical x.
        x: i32,
        /// Logical y.
        y: i32,
    },
    /// Move by whole pixels, for aiming without a mouse.
    Nudge {
        /// Horizontal pixels.
        dx: i32,
        /// Vertical pixels.
        dy: i32,
    },
    /// Increase magnification one step.
    ZoomIn,
    /// Decrease magnification one step.
    ZoomOut,
    /// Start or stop averaging a square instead of reading one pixel.
    Averaging(bool),
    /// Take the colour under the cursor.
    Commit,
    /// Abandon the pick.
    Cancel,
}

/// How a picking session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The user took a colour.
    Picked {
        /// The colour taken.
        color: Color,
        /// Where it came from, in logical desktop coordinates.
        at: (i32, i32),
        /// The display profile of the monitor it came from.
        source_space: Option<String>,
    },
    /// The user backed out.
    Cancelled,
}

/// One run of the picker over a frozen desktop.
#[derive(Debug)]
pub struct Session {
    capture: Capture,
    cursor: (i32, i32),
    zoom: u32,
    average_size: u32,
    averaging: bool,
    outcome: Option<Outcome>,
}

impl Session {
    /// Begin picking over `capture`, with the cursor at a logical position.
    ///
    /// The cursor is clamped onto a real monitor, so a stale pointer position
    /// from a display that has since been unplugged cannot start the session
    /// pointing at nothing.
    pub fn new(capture: Capture, cursor: (i32, i32), zoom: u32, average_size: u32) -> Self {
        let mut session = Self {
            capture,
            cursor,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            // An even window has no centre pixel to anchor on.
            average_size: if average_size.is_multiple_of(2) {
                average_size.saturating_add(1)
            } else {
                average_size
            }
            .max(1),
            averaging: false,
            outcome: None,
        };
        session.cursor = session.clamp_to_desktop(cursor);
        session
    }

    /// The current cursor position, in logical desktop coordinates.
    pub fn cursor(&self) -> (i32, i32) {
        self.cursor
    }

    /// Current magnification.
    pub fn zoom(&self) -> u32 {
        self.zoom
    }

    /// Whether Shift-averaging is active.
    pub fn is_averaging(&self) -> bool {
        self.averaging
    }

    /// The size of the averaged square, or 1 when reading a single pixel.
    pub fn sample_size(&self) -> u32 {
        if self.averaging { self.average_size } else { 1 }
    }

    /// The frozen desktop being picked from.
    pub fn capture(&self) -> &Capture {
        &self.capture
    }

    /// The frame under the cursor, if any.
    pub fn frame(&self) -> Option<&Frame> {
        self.capture.frame_at(self.cursor.0, self.cursor.1)
    }

    /// The colour currently under the cursor.
    ///
    /// `None` only if the cursor is somewhere no monitor covers, which the
    /// clamping in [`Session::new`] and [`Session::apply`] makes unreachable on
    /// a normal desktop but which is representable rather than a panic.
    pub fn current_color(&self) -> Option<Color> {
        let (x, y) = self.cursor;
        if self.averaging {
            self.capture.average_at(x, y, self.average_size).ok()
        } else {
            self.capture.pixel_at(x, y).ok()
        }
    }

    /// How the session ended, once it has.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// Whether the session is over.
    pub fn is_finished(&self) -> bool {
        self.outcome.is_some()
    }

    /// Feed the state machine an input.
    ///
    /// Inputs after the session ends are ignored, so a stray key arriving
    /// between commit and teardown cannot change what was picked.
    pub fn apply(&mut self, input: Input) {
        if self.outcome.is_some() {
            return;
        }

        match input {
            Input::PointerTo { x, y } => self.cursor = self.clamp_to_desktop((x, y)),
            Input::Nudge { dx, dy } => {
                let target = (
                    self.cursor.0.saturating_add(dx),
                    self.cursor.1.saturating_add(dy),
                );
                // A nudge that would leave the desktop keeps the old position
                // rather than sliding along an edge to somewhere unexpected.
                if self.capture.frame_at(target.0, target.1).is_some() {
                    self.cursor = target;
                }
            }
            Input::ZoomIn => self.zoom = (self.zoom * 2).min(MAX_ZOOM),
            Input::ZoomOut => self.zoom = (self.zoom / 2).max(MIN_ZOOM),
            Input::Averaging(on) => self.averaging = on,
            Input::Commit => {
                self.outcome = Some(match self.current_color() {
                    Some(color) => Outcome::Picked {
                        color,
                        at: self.cursor,
                        source_space: self.frame().and_then(|f| f.monitor.profile.tag()),
                    },
                    // Nothing under the cursor is not a colour worth returning.
                    None => Outcome::Cancelled,
                });
            }
            Input::Cancel => self.outcome = Some(Outcome::Cancelled),
        }
    }

    /// Move a point onto the nearest covered pixel, if it is not already.
    fn clamp_to_desktop(&self, (x, y): (i32, i32)) -> (i32, i32) {
        if self.capture.frame_at(x, y).is_some() {
            return (x, y);
        }

        // Nearest point on any monitor, by squared distance to its rectangle.
        self.capture
            .frames
            .iter()
            .map(|frame| {
                let m = &frame.monitor;
                let cx = x.clamp(m.logical_x, m.logical_x + m.logical_width as i32 - 1);
                let cy = y.clamp(m.logical_y, m.logical_y + m.logical_height as i32 - 1);
                let dx = i64::from(x - cx);
                let dy = i64::from(y - cy);
                ((cx, cy), dx * dx + dy * dy)
            })
            .min_by_key(|(_, d)| *d)
            .map(|(p, _)| p)
            .unwrap_or((x, y))
    }
}
