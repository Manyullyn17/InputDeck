//! Talking to the device over its USB serial port.

use std::io::{Read, Write};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use inputdeck_engine::protocol::*;
use serialport::{SerialPort, SerialPortType};

const VID: u16 = 0x1209;
const PID: u16 = 0x0001;

pub struct Device {
    port: Box<dyn SerialPort>,
}

pub struct Info {
    pub format_version: u16,
    pub protocol_version: u16,
}

fn find_port() -> Result<String> {
    let ports = serialport::available_ports().context("listing serial ports")?;
    ports
        .into_iter()
        .find(|p| matches!(&p.port_type, SerialPortType::UsbPort(u) if u.vid == VID && u.pid == PID))
        .map(|p| p.port_name)
        .ok_or_else(|| anyhow!("no InputDeck device found (is it plugged in and running InputDeck firmware?)"))
}

impl Device {
    pub fn open(port: Option<&str>) -> Result<Self> {
        let name = match port {
            Some(p) => p.to_string(),
            None => find_port()?,
        };
        let mut port = serialport::new(&name, 115_200)
            .timeout(Duration::from_secs(5))
            .open()
            .with_context(|| format!("opening {name}"))?;
        // The device only talks once the host signals it is ready.
        port.write_data_terminal_ready(true).ok();
        Ok(Device { port })
    }

    fn request(&mut self, command: u8, payload: &[u8]) -> Result<(u8, Vec<u8>)> {
        self.port.write_all(&frame(command, payload)).context("writing to device")?;
        self.port.flush().ok();
        let mut reader = FrameReader::new();
        let mut buf = [0u8; 256];
        loop {
            match reader.next_frame() {
                ReadResult::Frame { kind, payload } => return Ok((kind, payload)),
                ReadResult::TooLarge => bail!("device sent an oversized response"),
                ReadResult::Pending => {}
            }
            let n = self.port.read(&mut buf).context("waiting for device response")?;
            reader.push(&buf[..n]);
        }
    }

    fn expect_ok(status: u8, payload: &[u8]) -> Result<()> {
        let detail = String::from_utf8_lossy(payload);
        match status {
            STATUS_OK => Ok(()),
            STATUS_BAD_CONFIG => bail!("device rejected the config: {detail}"),
            STATUS_STORAGE_ERROR => bail!("device failed to store the config: {detail}"),
            STATUS_TOO_LARGE => bail!("config too large for the device"),
            STATUS_NO_CONFIG => bail!("no config stored on the device (it is using the built-in demo)"),
            STATUS_UNKNOWN_COMMAND => bail!("device does not support this command (update the firmware)"),
            other => bail!("unexpected device status 0x{other:02x}"),
        }
    }

    pub fn info(&mut self) -> Result<Info> {
        let (status, p) = self.request(CMD_PING, &[])?;
        Self::expect_ok(status, &p)?;
        if p.len() < 8 || p[..4] != PING_MAGIC {
            bail!("unexpected ping response");
        }
        Ok(Info {
            format_version: u16::from_le_bytes([p[4], p[5]]),
            protocol_version: u16::from_le_bytes([p[6], p[7]]),
        })
    }

    /// Uploads an encoded config. The device stores it and reboots.
    pub fn put_config(&mut self, bytes: &[u8]) -> Result<()> {
        let (status, p) = self.request(CMD_PUT_CONFIG, bytes)?;
        Self::expect_ok(status, &p)
    }

    pub fn get_config(&mut self) -> Result<Vec<u8>> {
        let (status, p) = self.request(CMD_GET_CONFIG, &[])?;
        Self::expect_ok(status, &p)?;
        Ok(p)
    }

    pub fn bootloader(&mut self) -> Result<()> {
        let (status, p) = self.request(CMD_BOOTLOADER, &[])?;
        Self::expect_ok(status, &p)
    }
}
