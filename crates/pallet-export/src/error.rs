//! Errors from reading and writing palette files.

/// Why a palette could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file ended mid-structure.
    #[error("the file ended unexpectedly")]
    Truncated,

    /// The file is not ASE.
    #[error("not an ASE file")]
    NotAse,

    /// The file is not a GIMP palette.
    #[error("not a GIMP palette")]
    NotGpl,

    /// An ASE colour used a model Pallet does not read.
    #[error("unsupported colour model `{0}`")]
    UnsupportedColourModel(String),

    /// JSON could not be parsed.
    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// A colour value could not be parsed.
    #[error("malformed colour: {0}")]
    Colour(#[from] pallet_color::ParseError),

    /// Writing an image failed.
    #[error("could not render the palette: {0}")]
    Image(#[from] image::ImageError),
}

/// Convenience alias for fallible export operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;
