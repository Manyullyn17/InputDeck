//! Framing for the USB serial protocol between config tools and the device.
//!
//! Every message, in both directions, is a frame: `kind: u8`,
//! `len: u32` (little endian), then `len` payload bytes. Requests use a
//! command as `kind`; responses use a status.

use alloc::vec::Vec;

/// Returns `PING_MAGIC`, the codec format version and the protocol version.
pub const CMD_PING: u8 = 0x01;
/// Payload: an encoded config (see [`crate::codec`]). Saved and applied
/// immediately (held keys, toggles and macros are released first).
pub const CMD_PUT_CONFIG: u8 = 0x02;
/// Returns the stored encoded config.
pub const CMD_GET_CONFIG: u8 = 0x03;
/// Reboots into the UF2 bootloader (after responding).
pub const CMD_BOOTLOADER: u8 = 0x04;

pub const STATUS_OK: u8 = 0x00;
pub const STATUS_UNKNOWN_COMMAND: u8 = 0x01;
/// Payload: a short text description.
pub const STATUS_BAD_CONFIG: u8 = 0x02;
pub const STATUS_STORAGE_ERROR: u8 = 0x03;
pub const STATUS_NO_CONFIG: u8 = 0x04;
pub const STATUS_TOO_LARGE: u8 = 0x05;

pub const PING_MAGIC: [u8; 4] = *b"MPOS";
pub const PROTOCOL_VERSION: u16 = 1;
pub const FRAME_HEADER_LEN: usize = 5;
/// Largest accepted payload.
pub const MAX_PAYLOAD: usize = 64 * 1024;

pub fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Reassembles frames from a byte stream arriving in arbitrary chunks.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadResult {
    /// More bytes are needed.
    Pending,
    Frame { kind: u8, payload: Vec<u8> },
    /// The announced length exceeds [`MAX_PAYLOAD`]; the reader was reset.
    TooLarge,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Takes the next complete frame, if any. Call repeatedly until it
    /// returns `Pending`.
    pub fn next_frame(&mut self) -> ReadResult {
        if self.buf.len() < FRAME_HEADER_LEN {
            return ReadResult::Pending;
        }
        let len = u32::from_le_bytes(self.buf[1..5].try_into().unwrap()) as usize;
        if len > MAX_PAYLOAD {
            self.buf.clear();
            return ReadResult::TooLarge;
        }
        if self.buf.len() < FRAME_HEADER_LEN + len {
            return ReadResult::Pending;
        }
        let kind = self.buf[0];
        let payload = self.buf[FRAME_HEADER_LEN..FRAME_HEADER_LEN + len].to_vec();
        self.buf.drain(..FRAME_HEADER_LEN + len);
        ReadResult::Frame { kind, payload }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembles_split_and_joined_frames() {
        let mut bytes = frame(CMD_PING, &[]);
        bytes.extend(frame(CMD_PUT_CONFIG, &[1, 2, 3]));
        let mut r = FrameReader::new();
        let mut frames = Vec::new();
        for chunk in bytes.chunks(2) {
            r.push(chunk);
            while let ReadResult::Frame { kind, payload } = r.next_frame() {
                frames.push((kind, payload));
            }
        }
        assert_eq!(frames, [(CMD_PING, vec![]), (CMD_PUT_CONFIG, vec![1, 2, 3])]);
    }

    #[test]
    fn rejects_oversized_frames() {
        let mut r = FrameReader::new();
        r.push(&[CMD_PUT_CONFIG, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.next_frame(), ReadResult::TooLarge);
        assert_eq!(r.next_frame(), ReadResult::Pending);
    }
}
