//! Errors from parsing colour notations.

/// Why a string could not be read as a colour.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The string contained a character that is not a hex digit.
    #[error("`{input}` is not a hex colour")]
    NotHex {
        /// The offending input, as given.
        input: String,
    },

    /// The digit count was neither 3 nor 6.
    #[error("`{input}` must have 3 or 6 hex digits")]
    BadLength {
        /// The offending input, as given.
        input: String,
    },
}
