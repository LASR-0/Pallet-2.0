//! Colour science for Pallet.
//!
//! Pure and free of I/O, so every rule here is testable without a display
//! server. [`Color`] is the storage type — 8-bit sRGB, exactly what a screen
//! pixel is — and richer spaces are derived on demand.
//!
//! Derived colours (harmony, ramps) are computed in Oklch by default rather
//! than HSL. See [`Space`] for why that matters.
//!
//! ```
//! use pallet_color::{Color, Harmony, Space, ramp};
//!
//! let picked = Color::parse_hex("#A5236E").unwrap();
//! assert_eq!(picked.to_hex(), "#A5236E");
//!
//! let complement = Harmony::Complementary.swatches(picked, Space::Oklch);
//! assert_eq!(complement.len(), 2);
//! assert_eq!(complement[0], picked);
//!
//! assert_eq!(ramp::ramp(picked, Space::Oklch).len(), 9);
//! ```

#![warn(missing_docs)]

pub mod color;
pub mod contrast;
pub mod error;
pub mod facets;
pub mod harmony;
pub mod naming;
pub mod ramp;
pub mod space;

pub use color::Color;
pub use error::ParseError;
pub use facets::{Facet, Sort};
pub use harmony::Harmony;
pub use space::Space;
