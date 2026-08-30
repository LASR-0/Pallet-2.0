//! A PNG contact sheet.

use crate::error::Result;
use crate::model::Palette;

/// Width of each swatch, in pixels.
const SWATCH: u32 = 160;
/// Height of the colour block above the label strip.
const BLOCK: u32 = 160;
/// Height of the strip carrying the hex.
const STRIP: u32 = 40;

/// Render a palette as a horizontal strip of swatches.
///
/// No text: drawing glyphs would mean bundling a rasteriser and a font for a
/// picture whose whole job is to show colours. The strip under each swatch
/// carries a readable tint of the colour instead, so the sheet still reads as
/// separate swatches rather than one band.
pub fn png(palette: &Palette) -> Result<Vec<u8>> {
    let count = palette.swatches.len().max(1) as u32;
    let width = SWATCH * count;
    let height = BLOCK + STRIP;

    let mut image = image::RgbImage::new(width, height);
    for (i, swatch) in palette.swatches.iter().enumerate() {
        let (r, g, b) = swatch.color.to_rgb();
        let x0 = i as u32 * SWATCH;

        for y in 0..height {
            for x in x0..(x0 + SWATCH).min(width) {
                let pixel = if y < BLOCK {
                    image::Rgb([r, g, b])
                } else {
                    // A darkened band, so adjacent light swatches stay distinct.
                    image::Rgb([
                        (u16::from(r) * 7 / 10) as u8,
                        (u16::from(g) * 7 / 10) as u8,
                        (u16::from(b) * 7 / 10) as u8,
                    ])
                };
                image.put_pixel(x, y, pixel);
            }
        }
    }

    let mut out = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}
