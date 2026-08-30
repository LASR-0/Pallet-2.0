//! Reading and writing palettes in the formats designers actually exchange.
//!
//! Text formats are lossy in one direction only: they carry colours and names
//! but nothing else, so a palette exported and re-imported keeps everything
//! Pallet stores about it. ASE and GPL are the interchange formats; JSON is the
//! one that round-trips exactly.

#![warn(missing_docs)]

pub mod ase;
pub mod error;
pub mod model;
pub mod sheet;
pub mod text;

pub use error::{Error, Result};
pub use model::{Palette, Swatch};

/// A format Pallet can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// CSS custom properties.
    CssVars,
    /// A Tailwind colour object.
    Tailwind,
    /// SCSS variables.
    Scss,
    /// Pallet's own JSON.
    Json,
    /// A GIMP palette.
    Gpl,
    /// Adobe Swatch Exchange.
    Ase,
    /// A PNG contact sheet.
    Png,
}

impl Format {
    /// Every format, in the order the Build screen lists them.
    pub const ALL: [Format; 7] = [
        Format::CssVars,
        Format::Tailwind,
        Format::Ase,
        Format::Json,
        Format::Png,
        Format::Scss,
        Format::Gpl,
    ];

    /// The stable identifier used by the UI and the CLI.
    pub fn id(self) -> &'static str {
        match self {
            Format::CssVars => "css",
            Format::Tailwind => "tailwind",
            Format::Scss => "scss",
            Format::Json => "json",
            Format::Gpl => "gpl",
            Format::Ase => "ase",
            Format::Png => "png",
        }
    }

    /// The label the prototype's chips show.
    pub fn label(self) -> &'static str {
        match self {
            Format::CssVars => "CSS vars",
            Format::Tailwind => "Tailwind",
            Format::Scss => "SCSS",
            Format::Json => "JSON",
            Format::Gpl => "GPL",
            Format::Ase => "ASE",
            Format::Png => "PNG",
        }
    }

    /// The file extension, without a dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::CssVars => "css",
            Format::Tailwind => "js",
            Format::Scss => "scss",
            Format::Json => "json",
            Format::Gpl => "gpl",
            Format::Ase => "ase",
            Format::Png => "png",
        }
    }

    /// Parse the identifier the UI sends.
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    /// Whether Pallet can read this format back.
    pub fn readable(self) -> bool {
        matches!(self, Format::Json | Format::Gpl | Format::Ase)
    }
}

/// Serialise a palette.
pub fn write(palette: &Palette, format: Format) -> Result<Vec<u8>> {
    Ok(match format {
        Format::CssVars => text::css_vars(palette).into_bytes(),
        Format::Tailwind => text::tailwind(palette).into_bytes(),
        Format::Scss => text::scss(palette).into_bytes(),
        Format::Json => text::json(palette)?.into_bytes(),
        Format::Gpl => text::gpl(palette).into_bytes(),
        Format::Ase => ase::write(palette),
        Format::Png => sheet::png(palette)?,
    })
}

/// Parse a palette from bytes.
pub fn read(bytes: &[u8], format: Format) -> Result<Palette> {
    match format {
        Format::Json => text::read_json(&String::from_utf8_lossy(bytes)),
        Format::Gpl => text::read_gpl(&String::from_utf8_lossy(bytes)),
        Format::Ase => ase::read(bytes),
        other => Err(Error::UnsupportedColourModel(other.label().into())),
    }
}
