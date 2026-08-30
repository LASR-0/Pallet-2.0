//! Adobe Swatch Exchange.
//!
//! A binary format, big-endian throughout, with names in UTF-16BE. Adobe never
//! published a specification; this follows the layout every implementation
//! agrees on:
//!
//! ```text
//! "ASEF"                     4 bytes
//! version                    u16 major, u16 minor
//! block count                u32
//! blocks:
//!   type                     u16   0x0001 colour, 0xC001 group open, 0xC002 close
//!   length                   u32   bytes that follow
//!   name length              u16   UTF-16 units, including the terminating NUL
//!   name                     UTF-16BE, NUL-terminated
//!   model                    4 bytes  "RGB ", "CMYK", "LAB ", "Gray"
//!   components               f32 BE each, 3 for RGB
//!   colour type              u16   0 global, 1 spot, 2 normal
//! ```
//!
//! Colour components are floats in 0..=1, not bytes. Round-tripping an 8-bit
//! colour through them is exact — every value `n/255` is representable in f32
//! and returns the same byte — which is what makes a byte-identical round trip
//! possible at all.

use pallet_color::Color;

use crate::error::{Error, Result};
use crate::model::{Palette, Swatch};

const SIGNATURE: &[u8; 4] = b"ASEF";
const BLOCK_COLOUR: u16 = 0x0001;
const BLOCK_GROUP_OPEN: u16 = 0xC001;
const BLOCK_GROUP_CLOSE: u16 = 0xC002;
const COLOUR_TYPE_NORMAL: u16 = 2;

/// Encode a name as UTF-16BE with the trailing NUL the format expects.
fn encode_name(name: &str) -> (u16, Vec<u8>) {
    let mut units: Vec<u16> = name.encode_utf16().collect();
    units.push(0);
    let mut bytes = Vec::with_capacity(units.len() * 2);
    for unit in &units {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    (units.len() as u16, bytes)
}

/// Serialise a palette as ASE.
///
/// The palette becomes a named group so its name survives; a bare list of
/// colours would lose it, and every application that reads ASE shows groups.
pub fn write(palette: &Palette) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());

    // One block to open the group, one per colour, one to close it.
    let blocks = palette.swatches.len() as u32 + 2;
    out.extend_from_slice(&blocks.to_be_bytes());

    let (count, name) = encode_name(&palette.name);
    out.extend_from_slice(&BLOCK_GROUP_OPEN.to_be_bytes());
    out.extend_from_slice(&((2 + name.len()) as u32).to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&name);

    for swatch in &palette.swatches {
        let (count, name) = encode_name(&swatch.label());
        let (r, g, b) = swatch.color.to_rgb();

        // name length + name + model + three floats + colour type
        let length = 2 + name.len() + 4 + 12 + 2;
        out.extend_from_slice(&BLOCK_COLOUR.to_be_bytes());
        out.extend_from_slice(&(length as u32).to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        out.extend_from_slice(&name);
        out.extend_from_slice(b"RGB ");
        for channel in [r, g, b] {
            out.extend_from_slice(&(f32::from(channel) / 255.0).to_be_bytes());
        }
        out.extend_from_slice(&COLOUR_TYPE_NORMAL.to_be_bytes());
    }

    out.extend_from_slice(&BLOCK_GROUP_CLOSE.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

/// A cursor that refuses to read past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(Error::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
}

/// A float component back to an 8-bit channel.
fn to_channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Parse an ASE file.
pub fn read(bytes: &[u8]) -> Result<Palette> {
    let mut r = Reader { bytes, at: 0 };

    if r.take(4)? != SIGNATURE {
        return Err(Error::NotAse);
    }
    let _major = r.u16()?;
    let _minor = r.u16()?;
    let blocks = r.u32()?;

    let mut palette = Palette::new("Imported", Vec::new());
    let mut named = false;

    for _ in 0..blocks {
        let kind = r.u16()?;
        let length = r.u32()? as usize;
        let body = r.take(length)?;

        if kind == BLOCK_GROUP_CLOSE {
            continue;
        }

        let mut b = Reader { bytes: body, at: 0 };
        let units = b.u16()? as usize;
        let mut name = String::new();
        let mut utf16 = Vec::with_capacity(units);
        for _ in 0..units {
            utf16.push(b.u16()?);
        }
        // Drop the terminating NUL before decoding.
        if utf16.last() == Some(&0) {
            utf16.pop();
        }
        name.push_str(&String::from_utf16_lossy(&utf16));

        match kind {
            BLOCK_GROUP_OPEN => {
                // The first group names the palette; nested groups are
                // flattened, since Pallet has no notion of a palette inside a
                // palette.
                if !named {
                    palette.name = name;
                    named = true;
                }
            }
            BLOCK_COLOUR => {
                let model = b.take(4)?;
                let color = match model {
                    b"RGB " => Color::new(
                        to_channel(b.f32()?),
                        to_channel(b.f32()?),
                        to_channel(b.f32()?),
                    ),
                    b"Gray" => {
                        let v = to_channel(b.f32()?);
                        Color::new(v, v, v)
                    }
                    b"CMYK" => {
                        let (c, m, y, k) = (b.f32()?, b.f32()?, b.f32()?, b.f32()?);
                        Color::new(
                            to_channel((1.0 - c) * (1.0 - k)),
                            to_channel((1.0 - m) * (1.0 - k)),
                            to_channel((1.0 - y) * (1.0 - k)),
                        )
                    }
                    other => {
                        return Err(Error::UnsupportedColourModel(
                            String::from_utf8_lossy(other).into_owned(),
                        ));
                    }
                };
                palette.swatches.push(Swatch {
                    color,
                    name: (!name.is_empty()).then_some(name),
                });
            }
            _ => {}
        }
    }

    Ok(palette)
}
