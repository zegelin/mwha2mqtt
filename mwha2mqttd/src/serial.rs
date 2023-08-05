use std::{io::{self, Read, Write}, time::Duration};

use log::{debug, info};
use serialport::SerialPort;

use delegate::delegate;

use anyhow::{Context, Result, bail};

use crate::amp::Port;

pub const BAUD_RATES: &'static [u32] = &[9600, 19200, 38400, 57600, 115200, 230400];


#[derive(Clone, Copy, Debug)]
pub enum BaudConfig {
    Rate(u32),
    Auto,
}

#[derive(Clone, Copy, Debug)]
pub enum AdjustBaudConfig {
    Rate(u32),
    Max,
    Off
}

pub struct AmpSerialPort {
    port: Box<dyn SerialPort>,

    previous_baud: Option<u32>
}

const BAUD_DETECT_TEST_DATA: &[u8] = b"baudrate detect\r";

impl AmpSerialPort {
    pub fn new(path: &str, baud_config: BaudConfig, adjust_baud: AdjustBaudConfig, reset_baud: bool, timeout: Duration) -> Result<Self> {
        let default_baud = match baud_config {
            BaudConfig::Rate(baud) => baud,
            BaudConfig::Auto => 9600,
        };

        let mut port = serialport::new(path, default_baud)
                .timeout(timeout)
                .open().with_context(|| format!("failed to open serial port {}", path))?;

        // detect the baud rate
        let detected_baud = match baud_config {
            BaudConfig::Rate(baud) => baud,
            BaudConfig::Auto => AmpSerialPort::detect_baud(&mut port).context("failed to detect baud")?,
        };

        // adjust the baud rate
        match adjust_baud {
            AdjustBaudConfig::Rate(baud) => AmpSerialPort::set_baud(&mut port, baud)?,
            AdjustBaudConfig::Max => AmpSerialPort::set_baud(&mut port, BAUD_RATES[BAUD_RATES.len()-1])?,
            AdjustBaudConfig::Off => (),
        }
        
        let p = AmpSerialPort {
            port,
            previous_baud: if reset_baud { Some(detected_baud) } else { None }, // todo: dont "reset" baud if no adjustment occured
        };

        Ok(p)
    }

    /// Detect the current baud rate of the amp.
    /// 
    /// Sets the baud rate of the serial port to each of the supported values and then
    /// writes a known string and compares the echo readback. If the echoed value is identical
    /// the baud rate is correct. 
    ///
    fn detect_baud(port: &mut Box<dyn SerialPort>) -> Result<u32> {
        let mut response_buffer = [0; BAUD_DETECT_TEST_DATA.len()];

        for &rate in BAUD_RATES {
            port.clear(serialport::ClearBuffer::All)?;

            debug!("Trying baud rate {}", rate);
            port.set_baud_rate(rate)?;

            port.write_all(BAUD_DETECT_TEST_DATA)?;
            match port.read_exact(&mut response_buffer) {
                Ok(_) => {
                    if response_buffer == BAUD_DETECT_TEST_DATA {
                        info!("Baud rate detected as {}", rate);
                        return Ok(rate)
                    }
                },
                Err(error) => match error.kind() {
                    io::ErrorKind::TimedOut => continue, // wrong baud possibly means less bytes read than expected and a timeout occurs
                    _ => return Err(error.into())
                },
            }
        }

        bail!("Unable to detect current baud rate.") // todo: custom error
    }

    fn set_baud(port: &mut Box<dyn SerialPort>, baud_rate: u32) -> Result<(), io::Error> {
        info!("Adjusting baud rate to {}", baud_rate);

        let cmd = format!("<{}\r", baud_rate);
        port.write_all(cmd.as_bytes())?;

        // As soon as the amp receives the '\r' of the command it switches baud.
        // There's no way to sync switching local baud with the amp (to my knowledge), esp. over IP.
        // Hence, even though baud set commands return "#Done." on success, the response is almost always corrupted.
        // Instead, drain the input buffer. A resync after changing baud...

        port.set_baud_rate(baud_rate)?;

        port.clear(serialport::ClearBuffer::All)?;

        Ok(())
    }
}

impl Drop for AmpSerialPort {
    fn drop(&mut self) {
        if let Some(baud) = self.previous_baud {
            AmpSerialPort::set_baud(&mut self.port, baud).unwrap(); // TODO: handle error better -- shouldn't panic, just log
        }
    }
}

impl Read for AmpSerialPort {
    delegate! {
        to self.port {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
        }
    }
}

impl Write for AmpSerialPort {
    delegate! {
        to self.port {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
            fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize>;
            //fn is_write_vectored(&self) -> bool;
            fn flush(&mut self) -> std::io::Result<()>;
            fn write_all(&mut self, mut buf: &[u8]) -> std::io::Result<()>;
            //fn write_all_vectored(&mut self, mut bufs: &mut [IoSlice<'_>]) -> std::io::Result<()>;
            fn write_fmt(&mut self, fmt: std::fmt::Arguments<'_>) -> std::io::Result<()>;
        }
    }
}

impl Port for AmpSerialPort {}