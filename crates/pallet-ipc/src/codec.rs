//! Length-prefixed JSON framing.

use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The largest message the protocol will read or write.
///
/// Messages here are a few hundred bytes; the cap exists so a corrupt or
/// hostile length prefix cannot make the reader allocate arbitrarily.
pub const MAX_MESSAGE: usize = 64 * 1024;

/// Why a message could not be exchanged.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The socket failed.
    #[error("ipc transport failed: {0}")]
    Io(#[from] std::io::Error),

    /// The payload was not valid JSON for the expected type.
    #[error("malformed ipc message: {0}")]
    Malformed(#[from] serde_json::Error),

    /// The declared length exceeded [`MAX_MESSAGE`].
    #[error("ipc message of {size} bytes exceeds the {MAX_MESSAGE} byte limit")]
    TooLarge {
        /// The declared size.
        size: usize,
    },

    /// The peer closed the connection before a whole message arrived.
    #[error("the connection closed mid-message")]
    Truncated,
}

/// Convenience alias for fallible IPC operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Write one length-prefixed JSON message.
pub fn write_message<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<()> {
    let payload = serde_json::to_vec(message)?;
    if payload.len() > MAX_MESSAGE {
        return Err(Error::TooLarge {
            size: payload.len(),
        });
    }

    let length = u32::try_from(payload.len()).expect("checked against MAX_MESSAGE above");
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Read one length-prefixed JSON message.
pub fn read_message<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<T> {
    let mut header = [0u8; 4];
    read_exact(reader, &mut header)?;

    let size = u32::from_be_bytes(header) as usize;
    if size > MAX_MESSAGE {
        return Err(Error::TooLarge { size });
    }

    let mut payload = vec![0u8; size];
    read_exact(reader, &mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}

/// Fill `buf`, distinguishing a clean close from a truncated message.
fn read_exact<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<()> {
    match reader.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(Error::Truncated),
        Err(e) => Err(Error::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{PickOptions, Request, Response};

    #[test]
    fn a_message_survives_the_round_trip() {
        let mut buffer = Vec::new();
        let sent = Request::Pick(PickOptions {
            zoom: Some(16),
            average_size: Some(5),
        });
        write_message(&mut buffer, &sent).unwrap();

        let got: Request = read_message(&mut buffer.as_slice()).unwrap();
        assert_eq!(got, sent);
    }

    #[test]
    fn several_messages_stream_back_in_order() {
        // The framing exists precisely because a stream socket has no message
        // boundaries; back-to-back writes must not run together.
        let mut buffer = Vec::new();
        write_message(&mut buffer, &Request::Ping).unwrap();
        write_message(&mut buffer, &Request::Shutdown).unwrap();
        write_message(&mut buffer, &Request::Pick(PickOptions::default())).unwrap();

        let mut cursor = buffer.as_slice();
        assert_eq!(
            read_message::<_, Request>(&mut cursor).unwrap(),
            Request::Ping
        );
        assert_eq!(
            read_message::<_, Request>(&mut cursor).unwrap(),
            Request::Shutdown
        );
        assert_eq!(
            read_message::<_, Request>(&mut cursor).unwrap(),
            Request::Pick(PickOptions::default())
        );
    }

    #[test]
    fn a_truncated_message_is_reported_as_such() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &Response::Cancelled).unwrap();
        buffer.truncate(buffer.len() - 1);

        let err = read_message::<_, Response>(&mut buffer.as_slice()).unwrap_err();
        assert!(matches!(err, Error::Truncated), "{err:?}");
    }

    #[test]
    fn a_clean_close_with_no_message_is_reported_as_truncated() {
        let empty: &[u8] = &[];
        assert!(matches!(
            read_message::<_, Response>(&mut { empty }).unwrap_err(),
            Error::Truncated
        ));
    }

    #[test]
    fn an_absurd_length_prefix_is_refused_before_allocating() {
        // Guards against a corrupt or hostile peer asking for a 4 GiB buffer.
        let mut framed = u32::MAX.to_be_bytes().to_vec();
        framed.extend_from_slice(b"{}");

        let err = read_message::<_, Response>(&mut framed.as_slice()).unwrap_err();
        assert!(matches!(err, Error::TooLarge { .. }), "{err:?}");
    }

    #[test]
    fn valid_json_of_the_wrong_shape_is_malformed_not_a_panic() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &serde_json::json!({"kind": "not_a_variant"})).unwrap();

        let err = read_message::<_, Request>(&mut buffer.as_slice()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)), "{err:?}");
    }
}
