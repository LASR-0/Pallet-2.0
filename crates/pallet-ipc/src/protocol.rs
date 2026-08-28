//! Messages exchanged between the CLI, the app and the picker.

use serde::{Deserialize, Serialize};

/// How a pick should behave, overriding the stored settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickOptions {
    /// Loupe magnification.
    pub zoom: Option<u32>,
    /// Width of the square averaged while Shift is held.
    pub average_size: Option<u32>,
}

/// A request to the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    /// Check the picker is alive and reachable.
    Ping,
    /// Freeze the screen and pick a colour.
    Pick(PickOptions),
    /// Ask the picker to exit.
    Shutdown,
}

/// A reply from the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    /// The picker is alive, and reports its version.
    Pong {
        /// The picker's crate version.
        version: String,
    },
    /// A colour was picked.
    Picked {
        /// Uppercase `#RRGGBB`.
        hex: String,
        /// Where it came from, in logical desktop coordinates.
        at: (i32, i32),
        /// The display profile it came from. `None` means sRGB.
        source_space: Option<String>,
    },
    /// The user abandoned the pick.
    Cancelled,
    /// The picker could not do what was asked.
    Error {
        /// A human-readable explanation.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        for request in [
            Request::Ping,
            Request::Shutdown,
            Request::Pick(PickOptions::default()),
            Request::Pick(PickOptions {
                zoom: Some(32),
                average_size: Some(5),
            }),
        ] {
            let json = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        }
    }

    #[test]
    fn responses_round_trip() {
        for response in [
            Response::Pong {
                version: "0.1.0".into(),
            },
            Response::Cancelled,
            Response::Picked {
                hex: "#A5236E".into(),
                at: (100, 200),
                source_space: Some("display-p3".into()),
            },
            Response::Error {
                message: "no displays".into(),
            },
        ] {
            let json = serde_json::to_string(&response).unwrap();
            assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), response);
        }
    }

    #[test]
    fn the_wire_format_is_tagged_and_readable() {
        // A future version must be able to tell messages apart, and a human
        // reading a socket dump should not have to guess.
        let json = serde_json::to_string(&Request::Ping).unwrap();
        assert!(json.contains("\"kind\":\"ping\""), "{json}");
    }
}
