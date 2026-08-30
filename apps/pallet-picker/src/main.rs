//! The resident picker.
//!
//! Runs for the life of the session holding a warm GPU context and an open
//! compositor connection, so a pick costs only the ~31 ms to capture the screen
//! rather than the ~250 ms a cold start needs. Kept in its own process because
//! Tauri's event loop and the overlay's cannot share a main thread, and because
//! an overlay panic must not take the colour library down with it.
//!
//! One request is served at a time: the overlay grabs the keyboard exclusively,
//! so two concurrent picks are not a thing that can meaningfully happen.

use std::io::ErrorKind;
use std::os::unix::net::{UnixListener, UnixStream};

use anyhow::{Context as _, Result};
use pallet_core::logging;
use pallet_ipc::{Request, Response, read_message, transport, write_message};

fn main() -> Result<()> {
    logging::init("info");

    let socket = transport::ensure_socket_dir().context("preparing the socket directory")?;

    // A socket file left behind by a crash would otherwise block binding
    // forever. Only remove it if nothing is listening, so a running picker is
    // never displaced by a second one starting.
    if socket.exists() {
        match UnixStream::connect(&socket) {
            Ok(_) => anyhow::bail!("a picker is already running at {}", socket.display()),
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                tracing::info!("removing a stale socket from a previous run");
                std::fs::remove_file(&socket).context("removing the stale socket")?;
            }
            Err(e) => return Err(e).context("probing the existing socket"),
        }
    }

    let listener =
        UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;

    // The expensive part, paid once.
    let started = std::time::Instant::now();
    let context = pallet_overlay::context().context("building the picker's GPU context")?;
    let info = context.adapter_info();
    tracing::info!(
        ms = started.elapsed().as_millis(),
        gpu = %info.name,
        backend = ?info.backend,
        "picker ready"
    );

    let mut capture = pallet_capture::open().context("opening a capture backend")?;

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("dropped a connection: {e}");
                continue;
            }
        };

        let request: Request = match read_message(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("unreadable request: {e}");
                continue;
            }
        };

        let response = match request {
            Request::Ping => Response::Pong {
                version: env!("CARGO_PKG_VERSION").into(),
            },
            Request::Shutdown => {
                let _ = write_message(
                    &mut stream,
                    &Response::Pong {
                        version: env!("CARGO_PKG_VERSION").into(),
                    },
                );
                tracing::info!("shutting down on request");
                break;
            }
            Request::Pick(options) => serve_pick(&context, capture.as_mut(), &options),
        };

        if let Err(e) = write_message(&mut stream, &response) {
            tracing::warn!("could not reply: {e}");
        }
    }

    // The socket is ours; leaving it behind would make the next start think a
    // picker is running.
    let _ = std::fs::remove_file(&socket);
    Ok(())
}

/// Capture the desktop and run one pick.
fn serve_pick(
    context: &pallet_overlay::Context,
    capture: &mut dyn pallet_capture::ScreenCapture,
    options: &pallet_ipc::PickOptions,
) -> Response {
    let t = std::time::Instant::now();
    let shot = match capture.capture_all() {
        Ok(shot) if !shot.frames.is_empty() => shot,
        Ok(_) => {
            return Response::Error {
                message: "no displays are connected".into(),
            };
        }
        Err(e) => {
            return Response::Error {
                message: format!("capture failed: {e}"),
            };
        }
    };

    tracing::info!(capture_ms = t.elapsed().as_millis(), "warm pick: captured");
    let zoom = options.zoom.unwrap_or(16);
    let average = options.average_size.unwrap_or(5);

    match pallet_overlay::run(context, shot, zoom, average) {
        Ok(pallet_overlay::Outcome::Picked {
            color,
            at,
            source_space,
            save,
        }) => Response::Picked {
            hex: color.to_hex(),
            at,
            source_space,
            save,
        },
        Ok(pallet_overlay::Outcome::Cancelled) => Response::Cancelled,
        Err(e) => Response::Error {
            message: e.to_string(),
        },
    }
}
