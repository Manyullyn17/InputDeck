//! Binary config format, used for flash storage and transfer.
//!
//! Layout: a 16-byte header followed by a postcard-encoded [`Bundle`].
//!
//! | offset | size | field |
//! | --- | --- | --- |
//! | 0 | 4 | magic `MPOS` |
//! | 4 | 2 | format version (little endian) |
//! | 6 | 2 | reserved, 0 |
//! | 8 | 4 | payload length (little endian) |
//! | 12 | 4 | CRC-32 of the payload (little endian) |
//!
//! Postcard is not self-describing, so any change to the config types
//! needs a new [`FORMAT_VERSION`]. The device only reads its own version;
//! migrating older files is the desktop tooling's job.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::config::{Config, ConfigError};

pub const MAGIC: [u8; 4] = *b"MPOS";
pub const FORMAT_VERSION: u16 = 4;
pub const HEADER_LEN: usize = 16;

/// Everything stored on the device. Several profiles fit the format; only
/// the active one is used for now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub active: u8,
    pub profiles: Vec<Config>,
}

impl Bundle {
    pub fn single(config: Config) -> Self {
        Bundle { active: 0, profiles: alloc::vec![config] }
    }

    pub fn active_config(&self) -> Option<&Config> {
        self.profiles.get(self.active as usize)
    }

    pub fn into_active_config(mut self) -> Option<Config> {
        let i = self.active as usize;
        (i < self.profiles.len()).then(|| self.profiles.swap_remove(i))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u16),
    BadLength,
    BadChecksum,
    Malformed,
    NoProfiles,
    Invalid(ConfigError),
}

pub fn encode(bundle: &Bundle) -> Vec<u8> {
    let payload = postcard::to_allocvec(bundle).expect("serializing to a Vec cannot fail");
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Total length (header + payload) announced by a header, if it is one.
pub fn framed_len(header: &[u8]) -> Option<usize> {
    if header.len() < HEADER_LEN || header[..4] != MAGIC {
        return None;
    }
    let len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    Some(HEADER_LEN + len)
}

/// Decodes and validates a bundle. Trailing bytes after the payload (e.g.
/// erased flash) are ignored.
pub fn decode(bytes: &[u8]) -> Result<Bundle, DecodeError> {
    if bytes.len() < HEADER_LEN {
        return Err(DecodeError::TooShort);
    }
    if bytes[..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != FORMAT_VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let crc = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let payload = bytes.get(HEADER_LEN..HEADER_LEN + len).ok_or(DecodeError::BadLength)?;
    if crc32(payload) != crc {
        return Err(DecodeError::BadChecksum);
    }
    let bundle: Bundle = postcard::from_bytes(payload).map_err(|_| DecodeError::Malformed)?;
    if bundle.profiles.is_empty() || bundle.active as usize >= bundle.profiles.len() {
        return Err(DecodeError::NoProfiles);
    }
    for config in &bundle.profiles {
        config.validate().map_err(DecodeError::Invalid)?;
    }
    Ok(bundle)
}

/// CRC-32 (IEEE 802.3, as used by zip/png).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::example;

    fn sample() -> Bundle {
        let mut layers = example();
        layers[1].name = "Nav".into();
        Bundle::single(Config::new(layers))
    }

    #[test]
    fn roundtrip() {
        let bundle = sample();
        let bytes = encode(&bundle);
        assert_eq!(framed_len(&bytes), Some(bytes.len()));
        assert_eq!(decode(&bytes), Ok(bundle));
    }

    #[test]
    fn trailing_erased_flash_is_ignored() {
        let mut bytes = encode(&sample());
        bytes.extend_from_slice(&[0xFF; 64]);
        assert!(decode(&bytes).is_ok());
    }

    #[test]
    fn corruption_is_detected() {
        let bytes = encode(&sample());
        let mut flipped = bytes.clone();
        *flipped.last_mut().unwrap() ^= 1;
        assert_eq!(decode(&flipped), Err(DecodeError::BadChecksum));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Err(DecodeError::BadLength));
        assert_eq!(decode(&[0xFF; 32]), Err(DecodeError::BadMagic));
        let mut version = bytes.clone();
        version[4] = 99;
        assert_eq!(decode(&version), Err(DecodeError::UnsupportedVersion(99)));
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut bundle = sample();
        bundle.profiles[0].layers[0].parent = Some(crate::config::LayerId(1));
        assert!(matches!(decode(&encode(&bundle)), Err(DecodeError::Invalid(_))));
    }

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn size_is_small() {
        // Five layers with defaults should stay well under one 4 KB flash sector.
        assert!(encode(&sample()).len() < 2048, "{}", encode(&sample()).len());
    }
}
