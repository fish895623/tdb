use std::io::{ErrorKind, Read};

use crate::MAX_FRAME_LEN;
use crate::error::ProtocolError;

/// The body length a frame header announces, checked against
/// [`MAX_FRAME_LEN`] before the caller reads or allocates anything.
///
/// A frame is a 4-byte little-endian body length, then the body.
pub fn frame_len(header: [u8; 4]) -> Result<usize, ProtocolError> {
    frame_len_within(header, MAX_FRAME_LEN)
}

/// Like [`frame_len`], with a limit of `max` bytes: for the frames that are
/// known to be small, such as the first one ([`MAX_HELLO_LEN`](crate::MAX_HELLO_LEN)).
pub fn frame_len_within(header: [u8; 4], max: u32) -> Result<usize, ProtocolError> {
    let len = u32::from_le_bytes(header);
    if len > max {
        return Err(ProtocolError::FrameTooLarge { len: len.into() });
    }
    Ok(len as usize)
}

/// Read one frame's body from `reader`. `Ok(None)` is a clean end of stream
/// between frames; an end in the middle of one is
/// [`ProtocolError::Truncated`].
///
/// The body buffer grows as bytes arrive, so a peer that announces a large
/// frame and sends nothing costs nothing.
pub fn read_payload<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < header.len() {
        match reader.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(ProtocolError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let len = frame_len(header)?;
    let mut payload = Vec::new();
    reader.take(len as u64).read_to_end(&mut payload)?;
    if payload.len() < len {
        return Err(ProtocolError::Truncated);
    }
    Ok(Some(payload))
}
