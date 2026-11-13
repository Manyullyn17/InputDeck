//! USB serial endpoint for config tools (see `macropad_engine::protocol`).

use alloc::format;
use alloc::vec::Vec;

use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::EndpointError;
use macropad_engine::codec::{self, FORMAT_VERSION};
use macropad_engine::protocol::*;

use crate::storage::{StoreError, Storage};

enum After {
    Nothing,
    Reboot,
    Bootloader,
}

pub async fn run<'d>(class: &mut CdcAcmClass<'d, Driver<'d, USB>>, storage: &mut Storage) -> ! {
    loop {
        class.wait_connection().await;
        let mut reader = FrameReader::new();
        let mut packet = [0u8; 64];
        'connection: loop {
            let Ok(n) = class.read_packet(&mut packet).await else {
                break;
            };
            reader.push(&packet[..n]);
            loop {
                let (status, payload, after) = match reader.next_frame() {
                    ReadResult::Pending => break,
                    ReadResult::TooLarge => (STATUS_TOO_LARGE, Vec::new(), After::Nothing),
                    ReadResult::Frame { kind, payload } => handle(kind, &payload, storage),
                };
                if send(class, &frame(status, &payload)).await.is_err() {
                    break 'connection;
                }
                match after {
                    After::Nothing => {}
                    After::Reboot => {
                        Timer::after_millis(50).await;
                        cortex_m::peripheral::SCB::sys_reset();
                    }
                    After::Bootloader => {
                        Timer::after_millis(50).await;
                        embassy_rp::rom_data::reset_to_usb_boot(0, 0);
                    }
                }
            }
        }
    }
}

fn handle(kind: u8, payload: &[u8], storage: &mut Storage) -> (u8, Vec<u8>, After) {
    match kind {
        CMD_PING => {
            let mut info = Vec::from(PING_MAGIC);
            info.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
            info.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
            (STATUS_OK, info, After::Nothing)
        }
        CMD_PUT_CONFIG => {
            if let Err(e) = codec::decode(payload) {
                return (STATUS_BAD_CONFIG, format!("{e:?}").into_bytes(), After::Nothing);
            }
            match storage.save(payload) {
                Ok(()) => (STATUS_OK, Vec::new(), After::Reboot),
                Err(StoreError::TooLarge) => (STATUS_TOO_LARGE, Vec::new(), After::Nothing),
                Err(e) => (STATUS_STORAGE_ERROR, format!("{e:?}").into_bytes(), After::Nothing),
            }
        }
        CMD_GET_CONFIG => match storage.read_active() {
            Some(bytes) => (STATUS_OK, bytes, After::Nothing),
            None => (STATUS_NO_CONFIG, Vec::new(), After::Nothing),
        },
        CMD_BOOTLOADER => (STATUS_OK, Vec::new(), After::Bootloader),
        _ => (STATUS_UNKNOWN_COMMAND, Vec::new(), After::Nothing),
    }
}

/// Writes in max-size packets; a final short (possibly empty) packet ends
/// the transfer on the host side.
async fn send<'d>(class: &mut CdcAcmClass<'d, Driver<'d, USB>>, bytes: &[u8]) -> Result<(), EndpointError> {
    let max = class.max_packet_size() as usize;
    for chunk in bytes.chunks(max) {
        class.write_packet(chunk).await?;
    }
    if bytes.len().is_multiple_of(max) {
        class.write_packet(&[]).await?;
    }
    Ok(())
}
