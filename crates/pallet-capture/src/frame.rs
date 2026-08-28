//! Captured pixels.

use pallet_color::Color;

use crate::error::{Error, Result};
use crate::monitor::Monitor;

/// The byte layouts Pallet can read.
///
/// Compositors hand back whatever suits their pipeline, so the decoder handles
/// the common orderings rather than demanding one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Blue, green, red, ignored alpha.
    Bgrx8888,
    /// Blue, green, red, alpha.
    Bgra8888,
    /// Red, green, blue, ignored alpha.
    Rgbx8888,
    /// Red, green, blue, alpha.
    Rgba8888,
}

impl PixelFormat {
    /// Bytes occupied by one pixel.
    pub const fn bytes_per_pixel(self) -> usize {
        4
    }

    /// Byte offsets of the red, green and blue channels within a pixel.
    const fn rgb_offsets(self) -> (usize, usize, usize) {
        match self {
            PixelFormat::Bgrx8888 | PixelFormat::Bgra8888 => (2, 1, 0),
            PixelFormat::Rgbx8888 | PixelFormat::Rgba8888 => (0, 1, 2),
        }
    }
}

/// One monitor's pixels, frozen at a moment in time.
///
/// Frames are held in memory only. Pallet never writes a captured frame to
/// disk: the picker reads a colour out of it and lets it go.
#[derive(Clone)]
pub struct Frame {
    /// The display this came from, including its colour profile.
    pub monitor: Monitor,
    /// Raw pixel bytes, `stride` bytes per row.
    pub data: Vec<u8>,
    /// Bytes per row. Often wider than `width * 4` because of alignment.
    pub stride: usize,
    /// Byte layout of each pixel.
    pub format: PixelFormat,
}

// A frame holds megabytes of pixels; printing them would be useless and slow.
impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("monitor", &self.monitor.id)
            .field(
                "pixels",
                &(self.monitor.pixel_width, self.monitor.pixel_height),
            )
            .field(
                "logical",
                &(self.monitor.logical_width, self.monitor.logical_height),
            )
            .field("stride", &self.stride)
            .field("format", &self.format)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl Frame {
    /// Read the pixel at a position local to this frame.
    ///
    /// Returns `None` outside the frame rather than panicking, because the
    /// loupe routinely asks about pixels near an edge.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.monitor.pixel_width || y >= self.monitor.pixel_height {
            return None;
        }

        let (ro, go, bo) = self.format.rgb_offsets();
        let base = y as usize * self.stride + x as usize * self.format.bytes_per_pixel();
        let pixel = self.data.get(base..base + 4)?;

        Some(Color::new(pixel[ro], pixel[go], pixel[bo]))
    }

    /// Mean colour of a square of `size` pixels centred on `(x, y)`.
    ///
    /// Averaging happens in **linear light**, not on gamma-encoded sRGB values.
    /// Averaging the encoded bytes directly — the obvious implementation — is
    /// simply wrong: it biases every result dark, most visibly on high-contrast
    /// edges, which is exactly where someone reaches for a 5x5 average.
    ///
    /// The window is clipped to the frame, so a sample near an edge averages
    /// fewer pixels rather than failing.
    pub fn average(&self, x: u32, y: u32, size: u32) -> Option<Color> {
        if size <= 1 {
            return self.pixel(x, y);
        }

        let radius = (size / 2) as i64;
        let (mut r, mut g, mut b) = (0.0f64, 0.0f64, 0.0f64);
        let mut counted = 0u32;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let sx = x as i64 + dx;
                let sy = y as i64 + dy;
                if sx < 0 || sy < 0 {
                    continue;
                }
                let Some(c) = self.pixel(sx as u32, sy as u32) else {
                    continue;
                };
                r += srgb_to_linear(c.r);
                g += srgb_to_linear(c.g);
                b += srgb_to_linear(c.b);
                counted += 1;
            }
        }

        if counted == 0 {
            return None;
        }

        let n = f64::from(counted);
        Some(Color::new(
            linear_to_srgb(r / n),
            linear_to_srgb(g / n),
            linear_to_srgb(b / n),
        ))
    }

    /// Total bytes held by this frame.
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }
}

/// A whole desktop, frozen: every monitor captured at the same moment.
#[derive(Debug, Clone, Default)]
pub struct Capture {
    /// One frame per connected monitor.
    pub frames: Vec<Frame>,
}

impl Capture {
    /// Read the pixel at a **logical** desktop coordinate.
    ///
    /// Logical is the right unit for the public API because that is what the
    /// pointer reports. The conversion to a physical pixel happens per monitor,
    /// so a mixed-scale desktop resolves correctly.
    pub fn pixel_at(&self, x: i32, y: i32) -> Result<Color> {
        let (frame, px, py) = self.locate(x, y)?;
        frame.pixel(px, py).ok_or(Error::OutOfBounds { x, y })
    }

    /// Average a square of physical pixels around a **logical** coordinate.
    pub fn average_at(&self, x: i32, y: i32, size: u32) -> Result<Color> {
        let (frame, px, py) = self.locate(x, y)?;
        frame
            .average(px, py, size)
            .ok_or(Error::OutOfBounds { x, y })
    }

    /// Resolve a logical point to a frame and a physical pixel within it.
    fn locate(&self, x: i32, y: i32) -> Result<(&Frame, u32, u32)> {
        let frame = self
            .frames
            .iter()
            .find(|f| f.monitor.contains(x, y))
            .ok_or(Error::OutOfBounds { x, y })?;

        let (px, py) = frame
            .monitor
            .to_pixel(x, y)
            .ok_or(Error::OutOfBounds { x, y })?;

        Ok((frame, px, py))
    }

    /// The frame covering a **logical** point, if any.
    pub fn frame_at(&self, x: i32, y: i32) -> Option<&Frame> {
        self.frames.iter().find(|f| f.monitor.contains(x, y))
    }

    /// The bounding box of the desktop, in **logical** units.
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let first = self.frames.first()?;
        let m = &first.monitor;
        let mut min_x = m.logical_x;
        let mut min_y = m.logical_y;
        let mut max_x = m.logical_x + m.logical_width as i32;
        let mut max_y = m.logical_y + m.logical_height as i32;

        for f in &self.frames[1..] {
            let m = &f.monitor;
            min_x = min_x.min(m.logical_x);
            min_y = min_y.min(m.logical_y);
            max_x = max_x.max(m.logical_x + m.logical_width as i32);
            max_y = max_y.max(m.logical_y + m.logical_height as i32);
        }
        Some((min_x, min_y, max_x, max_y))
    }
}

/// sRGB byte to linear light, per the sRGB transfer function.
fn srgb_to_linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light back to an sRGB byte.
fn linear_to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::{ColorProfile, Transform};

    fn frame_of(width: u32, height: u32, stride_pad: usize, fill: &[u8; 4]) -> Frame {
        let stride = width as usize * 4 + stride_pad;
        let mut data = vec![0u8; stride * height as usize];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let base = y * stride + x * 4;
                data[base..base + 4].copy_from_slice(fill);
            }
        }
        Frame {
            monitor: Monitor {
                id: "TEST".into(),
                name: "TEST".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: width,
                logical_height: height,
                pixel_width: width,
                pixel_height: height,
                transform: Transform::Normal,
                profile: ColorProfile::Srgb,
            },
            data,
            stride,
            format: PixelFormat::Bgrx8888,
        }
    }

    #[test]
    fn bgrx_channels_are_decoded_in_the_right_order() {
        // Bytes B=0x11 G=0x22 R=0x33 must read as #332211.
        let f = frame_of(4, 4, 0, &[0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(f.pixel(0, 0).unwrap().to_hex(), "#332211");
    }

    #[test]
    fn padded_strides_do_not_skew_rows() {
        // A stride wider than width*4 is the norm, not the exception.
        let f = frame_of(3, 3, 16, &[0x11, 0x22, 0x33, 0xFF]);
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(f.pixel(x, y).unwrap().to_hex(), "#332211", "at {x},{y}");
            }
        }
    }

    #[test]
    fn out_of_bounds_reads_are_none_not_panics() {
        let f = frame_of(4, 4, 0, &[0, 0, 0, 255]);
        assert!(f.pixel(4, 0).is_none());
        assert!(f.pixel(0, 4).is_none());
        assert!(f.pixel(u32::MAX, u32::MAX).is_none());
    }

    #[test]
    fn averaging_a_uniform_area_returns_that_colour() {
        let f = frame_of(8, 8, 0, &[0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(f.average(4, 4, 5).unwrap().to_hex(), "#332211");
    }

    #[test]
    fn averaging_happens_in_linear_light_not_on_encoded_bytes() {
        // Half black, half white. The naive byte mean is 127/128 (#7F7F7F);
        // the correct linear-light mean is around #BCBCBC. Getting this wrong
        // biases every averaged pick dark.
        let mut f = frame_of(4, 1, 0, &[0, 0, 0, 255]);
        for x in 2..4 {
            let base = x * 4;
            f.data[base..base + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        let avg = f.average(1, 0, 5).unwrap();
        assert!(
            avg.r > 180,
            "linear-light average was {avg}, suspiciously close to the naive byte mean"
        );
    }

    #[test]
    fn averaging_clips_at_edges_rather_than_failing() {
        let f = frame_of(4, 4, 0, &[0x11, 0x22, 0x33, 0xFF]);
        // A 5x5 window centred on a corner mostly falls outside the frame.
        assert_eq!(f.average(0, 0, 5).unwrap().to_hex(), "#332211");
    }

    #[test]
    fn a_size_one_average_is_just_the_pixel() {
        let f = frame_of(4, 4, 0, &[0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(f.average(1, 1, 1), f.pixel(1, 1));
    }

    #[test]
    fn global_coordinates_route_to_the_right_monitor() {
        let mut left = frame_of(1920, 4, 0, &[0x11, 0x11, 0x11, 0xFF]);
        left.monitor.id = "DP-2".into();
        let mut right = frame_of(1920, 4, 0, &[0x99, 0x99, 0x99, 0xFF]);
        right.monitor.id = "DP-1".into();
        right.monitor.logical_x = 1920;

        let capture = Capture {
            frames: vec![left, right],
        };

        assert_eq!(capture.pixel_at(0, 0).unwrap().to_hex(), "#111111");
        assert_eq!(capture.pixel_at(1919, 0).unwrap().to_hex(), "#111111");
        assert_eq!(capture.pixel_at(1920, 0).unwrap().to_hex(), "#999999");
        assert_eq!(capture.frame_at(1920, 0).unwrap().monitor.id, "DP-1");
        assert!(capture.pixel_at(5000, 0).is_err());
        assert_eq!(capture.bounds(), Some((0, 0, 3840, 4)));
    }
}
