//! Messages exchanged between the CLI, the app and the picker.

use serde::{Deserialize, Serialize};

/// The keys the loupe answers to.
///
/// Sent with the request rather than read by the picker, so the caller's
/// settings are the single source of truth and a resident picker does not have
/// to notice that a config file changed underneath it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoupeKeys {
    /// Take the colour.
    pub commit: String,
    /// Take it and keep it in the library.
    pub save: String,
    /// Abandon the pick.
    pub cancel: String,
}

impl Default for LoupeKeys {
    fn default() -> Self {
        Self {
            commit: "Return".into(),
            save: "S".into(),
            cancel: "Escape".into(),
        }
    }
}

/// One colour taken during a palette pass.
///
/// Carries the same provenance as a single pick: these go into the history and
/// the library like any other, and a wide-gamut colour that arrived as a bare
/// hex could never be told apart from an sRGB one afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TakenColour {
    /// Uppercase `#RRGGBB`.
    pub hex: String,
    /// Where it came from, in logical desktop coordinates.
    pub at: (i32, i32),
    /// The display profile it came from. `None` means sRGB.
    #[serde(default)]
    pub source_space: Option<String>,
}

/// A request to gather several colours in one pass.
///
/// The overlay stays up between picks rather than freezing the screen once per
/// colour: a palette is chosen by comparing its colours against each other,
/// which cannot be done if the screen is released and re-frozen in between.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteRequest {
    /// Colours the caller already holds, as `#RRGGBB`.
    ///
    /// Shown in the HUD's tray as slots that are already filled, so a palette
    /// resumed from the Build screen looks like one pass rather than a fresh
    /// start. They are not returned again.
    #[serde(default)]
    pub collected: Vec<String>,
    /// How many slots the tray shows in total.
    pub target: usize,
}

/// How a pick should behave, overriding the stored settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickOptions {
    /// Loupe magnification.
    pub zoom: Option<u32>,
    /// Width of the square averaged while Shift is held.
    pub average_size: Option<u32>,
    /// Remapped loupe keys. `None` uses the defaults.
    #[serde(default)]
    pub keys: Option<LoupeKeys>,
    /// Gather a whole palette in one pass instead of a single colour.
    ///
    /// `None` for an ordinary single pick, which hides the tray entirely: a
    /// "Palette 1 / 5" strip would promise a flow the user did not start.
    #[serde(default)]
    pub palette: Option<PaletteRequest>,
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
    /// The picker is alive, and reports what build it is running.
    Pong {
        /// The picker's crate version.
        version: String,
        /// When the running picker's executable was last written, in seconds
        /// since the epoch.
        ///
        /// The crate version does not change between builds, but a resident
        /// daemon outlives them: rebuild the workspace and the picker still
        /// serving requests is the one compiled before the change. Callers
        /// compare this with the binary on disk and restart a picker that has
        /// been left behind.
        #[serde(default)]
        build: u64,
    },
    /// A colour was picked.
    Picked {
        /// Uppercase `#RRGGBB`.
        hex: String,
        /// Where it came from, in logical desktop coordinates.
        at: (i32, i32),
        /// The display profile it came from. `None` means sRGB.
        source_space: Option<String>,
        /// The user asked for it to be kept in the library, not just copied.
        #[serde(default)]
        save: bool,
    },
    /// A palette pass finished, with everything it gathered.
    ///
    /// Sent for a [`PaletteRequest`] whether the tray filled or the user
    /// finished early, so long as at least one colour was taken.
    PickedSet {
        /// The colours, in the order they were picked.
        colours: Vec<TakenColour>,
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
                keys: None,
                palette: None,
            }),
            Request::Pick(PickOptions {
                zoom: None,
                average_size: None,
                keys: None,
                palette: Some(PaletteRequest {
                    collected: vec!["#F0A9A0".into(), "#C9736A".into()],
                    target: 5,
                }),
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
                build: 1_700_000_000,
            },
            Response::Cancelled,
            Response::PickedSet {
                colours: vec![TakenColour {
                    hex: "#A5236E".into(),
                    at: (10, 20),
                    source_space: Some("display-p3".into()),
                }],
            },
            Response::Picked {
                hex: "#A5236E".into(),
                at: (100, 200),
                source_space: Some("display-p3".into()),
                save: true,
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
    fn a_request_without_keys_still_parses() {
        // An older client, or one that does not care, omits the field; the
        // picker must fall back to its defaults rather than refuse the pick.
        let json = r#"{"kind":"pick","zoom":16,"average_size":5}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        match request {
            Request::Pick(options) => {
                assert_eq!(options.keys, None);
                assert_eq!(options.palette, None, "an old client shows no tray");
            }
            other => panic!("expected a pick, got {other:?}"),
        }
    }

    #[test]
    fn loupe_keys_round_trip() {
        let sent = Request::Pick(PickOptions {
            zoom: None,
            average_size: None,
            keys: Some(LoupeKeys {
                commit: "SPACE".into(),
                save: "K".into(),
                cancel: "Q".into(),
            }),
            palette: None,
        });
        let json = serde_json::to_string(&sent).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), sent);
    }

    #[test]
    fn a_pong_without_a_build_stamp_still_parses() {
        // An older picker predates the field; treat it as an unknown build
        // rather than refusing to talk to it.
        let json = r#"{"kind":"pong","version":"0.1.0"}"#;
        match serde_json::from_str::<Response>(json).unwrap() {
            Response::Pong { build, .. } => assert_eq!(build, 0),
            other => panic!("expected a pong, got {other:?}"),
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
