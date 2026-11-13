//! Config storage in two alternating flash slots at the end of flash.
//!
//! Each slot holds a sequence number (u32 LE) followed by an encoded config
//! (see `macropad_engine::codec`). The valid slot with the highest sequence
//! number is active. Saving writes the *other* slot and verifies it before
//! it counts, so losing power mid-write keeps the previous config.

use alloc::vec;
use alloc::vec::Vec;

use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash};
use embassy_rp::peripherals::FLASH;
use macropad_engine::Config;
use macropad_engine::codec::{self, HEADER_LEN};

pub const FLASH_SIZE: usize = 8 * 1024 * 1024;
/// Must match the region excluded from FLASH in memory.x.
const SLOT_SIZE: usize = 64 * 1024;
const SLOT_OFFSETS: [u32; 2] = [(FLASH_SIZE - 2 * SLOT_SIZE) as u32, (FLASH_SIZE - SLOT_SIZE) as u32];
const SEQ_LEN: usize = 4;
pub const MAX_CONFIG_LEN: usize = SLOT_SIZE - SEQ_LEN;

#[derive(Debug)]
pub enum StoreError {
    TooLarge,
    Flash,
    VerifyFailed,
}

pub struct Storage {
    flash: Flash<'static, FLASH, Blocking, FLASH_SIZE>,
    /// (slot index, sequence number) of the active slot.
    active: Option<(usize, u32)>,
}

impl Storage {
    pub fn new(flash: Flash<'static, FLASH, Blocking, FLASH_SIZE>) -> Self {
        let mut storage = Storage { flash, active: None };
        for slot in 0..SLOT_OFFSETS.len() {
            if let Some((seq, _)) = storage.read_slot(slot)
                && storage.active.is_none_or(|(_, best)| seq > best)
            {
                storage.active = Some((slot, seq));
            }
        }
        storage
    }

    /// Sequence number and encoded config of a slot, if it holds a valid one.
    fn read_slot(&mut self, slot: usize) -> Option<(u32, Vec<u8>)> {
        let offset = SLOT_OFFSETS[slot];
        let mut head = [0u8; SEQ_LEN + HEADER_LEN];
        self.flash.blocking_read(offset, &mut head).ok()?;
        let seq = u32::from_le_bytes(head[..SEQ_LEN].try_into().unwrap());
        if seq == u32::MAX {
            return None; // erased
        }
        let len = codec::framed_len(&head[SEQ_LEN..])?;
        if len > MAX_CONFIG_LEN {
            return None;
        }
        let mut bytes = vec![0u8; len];
        self.flash.blocking_read(offset + SEQ_LEN as u32, &mut bytes).ok()?;
        codec::decode(&bytes).ok()?;
        Some((seq, bytes))
    }

    /// The encoded active config.
    pub fn read_active(&mut self) -> Option<Vec<u8>> {
        let (slot, _) = self.active?;
        self.read_slot(slot).map(|(_, bytes)| bytes)
    }

    pub fn load(&mut self) -> Option<Config> {
        codec::decode(&self.read_active()?).ok()?.into_active_config()
    }

    /// Stores an encoded config (already validated by the caller). Blocks
    /// for the flash erase and write (tens of milliseconds).
    pub fn save(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        if bytes.len() > MAX_CONFIG_LEN {
            return Err(StoreError::TooLarge);
        }
        let (slot, seq) = match self.active {
            Some((active, seq)) => (1 - active, seq.wrapping_add(1) % u32::MAX),
            None => (0, 0),
        };
        let offset = SLOT_OFFSETS[slot];
        let mut data = Vec::with_capacity(SEQ_LEN + bytes.len());
        data.extend_from_slice(&seq.to_le_bytes());
        data.extend_from_slice(bytes);
        let erase_len = data.len().div_ceil(ERASE_SIZE) * ERASE_SIZE;
        self.flash.blocking_erase(offset, offset + erase_len as u32).map_err(|_| StoreError::Flash)?;
        self.flash.blocking_write(offset, &data).map_err(|_| StoreError::Flash)?;
        match self.read_slot(slot) {
            Some((s, stored)) if s == seq && stored == bytes => {
                self.active = Some((slot, seq));
                Ok(())
            }
            _ => Err(StoreError::VerifyFailed),
        }
    }
}
