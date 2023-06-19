
use std::ascii::escape_default;
use std::io::Read;
use std::io::Write;

use std::net::TcpStream;
use std::str;

use anyhow::bail;
use log::debug;

use anyhow::{Context, Result};

use common::zone::ZoneId;
use common::zone::ZoneAttribute;
use common::zone::ZoneAttributeDiscriminants;



pub trait Port: Read + Write + Send {}

impl Port for TcpStream {}


pub struct ZoneStatus {
    pub id: ZoneId,
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

        amp.resync()?;

		Ok( amp )
	}

    fn read_until(&mut self, marker: &[u8]) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(256);
		
        // maybe switch to a BufReader, but this is 9600 baud serial, performance isn't really an issue.
        while !buffer.ends_with(marker) {
            let mut ch = [0; 1];
            
            if let Err(e) = self.port.read(&mut ch) {
                print!("read err, expecting: ");
                print_buffer(&marker);
                print!("buffer: ");
                print_buffer(&buffer);
                println!();
                return Err(e.into());
            }
            
            buffer.extend_from_slice(&ch);
        }

        //print_buffer(&buffer);

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
            bail!("Serial echoback was not the expected value. got = {:?}, expected = {:?}", str::from_utf8(&echo), str::from_utf8(command));
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

        println!("cmd: '{}' reply: '{}'", escape(&cmd), escape(&reply));

        self.port.write(cmd.as_bytes())?;
        self.read_until(reply.as_bytes())?;

        Ok(())
    }

    pub fn zone_enquiry(&mut self, id: ZoneId) -> Result<Vec<ZoneStatus>> {
        let (amp, zone, expected_responses) = match id {
            ZoneId::Zone { amp, zone } => (amp, zone, 1),
            ZoneId::Amp(amp) => (amp, 0, 6),
        };

        let cmd = format!("?{:}{:}", amp, zone);

        //let mut statuses = Vec::with_capacity(expected_responses);

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

            Ok(ZoneStatus {
                id: ZoneId::try_from(values[0]).context("valid zone id")?,
                attributes: vec![
                    ZoneAttribute::PublicAnnouncement(values[1] != 0),
                    ZoneAttribute::Power(values[2] != 0),
                    ZoneAttribute::Mute(values[3] != 0),
                    ZoneAttribute::DoNotDisturb(values[4] != 0),
                    ZoneAttribute::Volume(values[5]),
                    ZoneAttribute::Treble(values[6]),
                    ZoneAttribute::Bass(values[7]),
                    ZoneAttribute::Balance(values[8]),
                    ZoneAttribute::Source(values[9]),
                    ZoneAttribute::KeypadConnected(values[10] != 0)
                ] 
            })
        }).collect::<Result<Vec<ZoneStatus>>>()
    }

    pub fn set_zone_attribute(&mut self, id: ZoneId, attr: ZoneAttribute) -> anyhow::Result<()> {
        let range = ZoneAttributeDiscriminants::from(attr).io_range();

        let (attr, val) = match attr {
            ZoneAttribute::Power(v) => ("PR", v as u8),
            ZoneAttribute::Mute(v) => ("MU", v as u8),
            ZoneAttribute::DoNotDisturb(v) => ("DT", v as u8),
            ZoneAttribute::Volume(v) => ("VO", v),
            ZoneAttribute::Treble(v) => ("TR", v),
            ZoneAttribute::Bass(v) => ("BS", v),
            ZoneAttribute::Balance(v) => ("BL", v),
            ZoneAttribute::Source(v) => ("CH", v),
            _ => bail!("return error for unchangable attributes")
        };

        if !range.contains(&val) {
            panic!("out of range"); // todo: return IO error
        }

        let cmd = format!("<{}{}{:02}", id, attr, val);

        self.exec_command(cmd.as_bytes(), 0)?;

        Ok(())
    }
}