
use std::ascii::escape_default;
use std::io::Read;
use std::io::Write;

use std::net::TcpStream;
use std::str;

use anyhow::bail;
use itertools::Itertools;
use log::debug;

use anyhow::{Context, Result};

use common::zone::ZoneId;
use common::zone::ZoneAttribute;



pub trait Port: Read + Write + Send {}

impl Port for TcpStream {}


pub struct ZoneStatus {
    pub zone_id: ZoneId,
    pub attributes: Vec<ZoneAttribute>
}


pub struct Amp {
	port: Box<dyn Port>
}

fn escape(s: &String) -> String {
    String::from_utf8(
        s.bytes()
            .flat_map(|b| std::ascii::escape_default(b))
            .collect::<Vec<u8>>(),
    )
    .unwrap()
}

pub fn print_buffer(buffer: &[u8]) {
    let foo = &buffer.iter()
            .flat_map(|b| escape_default(*b))
            .collect::<Vec<u8>>();

        let s = String::from_utf8_lossy(
            &foo
        );
        print!("{}, {:?}", s, buffer);
}

impl Amp {
    const END_OF_RESPONSE_MARKER: &[u8] = b"\r\n#";

	pub fn new(port: Box<dyn Port>) -> Result<Self> {
        let mut amp = Self {
			port
		};

        amp.resync().context("failed to resync amp connection")?;

		Ok( amp )
	}

    fn read_until(&mut self, marker: &[u8]) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(256);
		
        // maybe switch to a BufReader?
        // (but this is 9600 baud serial, performance isn't really an issue!)
        while !buffer.ends_with(marker) {
            let mut ch = [0; 1];

            self.port.read(&mut ch)
                .context("failed to read from port")?;
            
            buffer.extend_from_slice(&ch);
        }

        Ok(buffer)
    }

    fn read_command_response(&mut self) -> Result<Vec<u8>> {
        let mut buffer = self.read_until(Self::END_OF_RESPONSE_MARKER)?;

        buffer.truncate(buffer.len() - Self::END_OF_RESPONSE_MARKER.len());

        if buffer == b"\r\nCommand Error." {
            bail!("amp responded with command error while executing command.");
        }

        Ok(buffer)
    }

	fn exec_command(&mut self, command: &[u8], expected_responses: usize) -> Result<Vec<Vec<u8>>> {
		// write command
        self.port.write(command)?;
		self.port.write(b"\r")?;
		self.port.flush()?;
		
        // read echoback
		let echo = self.read_command_response()?;
        if echo != command {
            bail!("serial echoback was not the expected value. got = {:?}, expected = {:?}", str::from_utf8(&echo), str::from_utf8(command));
        }

        // read responses
        let mut responses = Vec::with_capacity(expected_responses);
        for _i in 0..expected_responses {
            responses.push(self.read_command_response()?);
        }

		Ok(responses)
	}

    /// Resyncronise the serial stream.
    /// 
    /// A unique marker is written to the serial port and then the port read buffer is consumed until the echo-back
    /// contains the unique marker, skipping any old or unexpected received data.
    /// It is then assumed that the next write can issue a valid command and expect a vaild response.
    fn resync(&mut self) -> Result<()> {
        debug!("resyncing serial connection...");

        use rand::distributions::{Alphanumeric, DistString};
        let marker = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
        let marker = format!("resync{}", marker);

        let cmd = format!("{}\r", marker);
        let reply = format!("{}\r\n#\r\nCommand Error.\r\n#", marker);

        println!("cmd: '{}', expected reply: '{}'", escape(&cmd), escape(&reply));

        self.port.write(cmd.as_bytes())?;
        self.read_until(reply.as_bytes())?;

        Ok(())
    }

    pub fn zone_enquiry(&mut self, id: ZoneId) -> Result<Vec<ZoneStatus>> {
        if let ZoneId::System = id {
            return id.to_amps().into_iter()
                .map(|amp| self.zone_enquiry(amp))
                .flatten_ok()
                .collect();
        }

        let (amp, zone, expected_responses) = match id {
            ZoneId::Zone { amp, zone } => (amp, zone, 1),
            ZoneId::Amp(amp) => (amp, 0, 6),
            ZoneId::System => unreachable!()
        };

        let cmd = format!("?{:}{:}", amp, zone);

        self.exec_command(cmd.as_bytes(), expected_responses)?
            .into_iter()
            .map(|resp| -> Result<ZoneStatus> {
            let values = resp[1..] // skip leading '>'
                .chunks_exact(2)
                .map(|c| -> Result<u8> {
                    let s = str::from_utf8(c).context("response string not valid UTF-8")?;

                    Ok(str::parse::<u8>(s).context("failed to parse u8")?)
                })
                .collect::<Result<Vec<_>>>()?;

            {
                use ZoneAttribute::*;

                Ok(ZoneStatus {
                    zone_id: ZoneId::try_from(values[0]).context("invalid zone id received from amp")?,
                    attributes: vec![
                        PublicAnnouncement(values[1] != 0),
                        Power(values[2] != 0),
                        Mute(values[3] != 0),
                        DoNotDisturb(values[4] != 0),
                        Volume(values[5]),
                        Treble(values[6]),
                        Bass(values[7]),
                        Balance(values[8]),
                        Source(values[9]),
                        KeypadConnected(values[10] != 0)
                    ] 
                })
            }
        }).collect()
    }

    pub fn set_zone_attribute(&mut self, id: ZoneId, attr: ZoneAttribute) -> Result<()> {
        if let ZoneId::System = id {
            return id.to_amps().into_iter()
                .map(|amp| self.set_zone_attribute(amp, attr))
                .collect();
        }

        attr.validate()?;

        let (attr, val) = {
            use ZoneAttribute::*;

            match attr {
                Power(v) => ("PR", v as u8),
                Mute(v) => ("MU", v as u8),
                DoNotDisturb(v) => ("DT", v as u8),
                Volume(v) => ("VO", v),
                Treble(v) => ("TR", v),
                Bass(v) => ("BS", v),
                Balance(v) => ("BL", v),
                Source(v) => ("CH", v),
                attr => bail!("{} cannot be changed", attr)
            }
        };


        let cmd = format!("<{}{}{:02}", id, attr, val);

        self.exec_command(cmd.as_bytes(), 0)?;

        Ok(())
    }
}