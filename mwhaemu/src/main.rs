
use std::{net::TcpListener, thread, sync::{Arc, Mutex}};

use clap::{command, Subcommand, Parser, ArgAction};
use anyhow::Result;
use common::zone::{ZoneAttribute, ZoneAttributeDiscriminants, ZoneId};


mod emu {
    use common::zone::MAX_ZONES_PER_AMP;

    use super::*;
    use std::{collections::HashMap, io::{Read, Write}, str};

    #[derive(Debug, Default)]
    pub struct Zone {
        pub public_announcement: bool,
        pub power: bool,
        pub mute: bool,
        pub do_not_disturb: bool,
        pub volume: u8,
        pub treble: u8,
        pub bass: u8,
        pub balance: u8,
        pub source: u8,
        pub keypad_connected: bool
    }

    impl Zone {
        fn set(&mut self, attribute: ZoneAttribute) {
            match attribute {
                ZoneAttribute::PublicAnnouncement(b) => self.public_announcement = b,
                ZoneAttribute::Power(b) => self.power = b,
                ZoneAttribute::Mute(b) => self.mute = b,
                ZoneAttribute::DoNotDisturb(b) => self.do_not_disturb = b,
                ZoneAttribute::Volume(v) => self.volume = v,
                ZoneAttribute::Treble(v) => self.treble = v,
                ZoneAttribute::Bass(v) => self.bass = v,
                ZoneAttribute::Balance(v) => self.balance = v,
                ZoneAttribute::Source(v) => self.source = v,
                ZoneAttribute::KeypadConnected(b) => self.keypad_connected = b,
            }
        }
    }

    pub struct Amp {
        pub zones: HashMap<ZoneId, Zone>
    }

    impl Amp {
        pub fn new(amps: u8) -> Self {
            // create the zones -- 6 zones per amp
            let mut zones = Vec::with_capacity((amps * 6).into());
            {
                for amp in 1..=amps {
                    for zone in 1..=MAX_ZONES_PER_AMP {
                        zones.push((ZoneId::Zone { amp, zone }, Zone::default()))
                    }
                }
            }
            
            Self {
                zones: zones.into_iter().collect()
            }
        }
    
        /// set the attributes of one or more zones. nop if a zone doesn't exist.
        pub fn zone_set(&mut self, zone: ZoneId, attribute: ZoneAttribute) {
            for zone in zone.to_zones() {
                if let Some(zone) = self.zones.get_mut(&zone) {
                    zone.set(attribute)
                }
            }
        }

        /// get the staus of one or more zones. nop if a zone doesn't exist.
        pub fn zone_enquiry(&mut self, zone: ZoneId) -> Vec<(ZoneId, &Zone)> {
            zone.to_zones().into_iter().filter_map(|id| {
                self.zones.get(&id).map(|zone| (id, zone))
            }).collect()
        }
    
        pub fn set_pa_state(&mut self, pa: bool) {
            for zone in self.zones.values_mut() {
                zone.public_announcement = pa;
            } 
        }
    }
}


mod repl {
    use super::*;
    
    use std::ops::{RangeInclusive};
    
    use rustyline::{DefaultEditor, Editor, CompletionType, Completer};
    use rustyline::{Helper, Hinter, Validator, Highlighter};

    use common::zone::ranges;

    fn cast_range(range: RangeInclusive<u8>) -> RangeInclusive<i64> {
        RangeInclusive::new(*range.start() as i64, *range.end() as i64)
    }

    #[derive(Subcommand, Debug)]
    enum AdjustableAttributeCommand {
        // PA is ommitted bacuase on real hardware PA can only be toggled for all zones simultaneously
        // which is exposed as a separate command

        #[command(visible_alias = "pr")]
        Power {
            #[arg(action = ArgAction::Set)]
            value: bool
        },
        #[command(visible_alias = "mu")]
        Mute {
            #[arg(action = ArgAction::Set)]
            value: bool
        },
        #[command(visible_alias = "dt")]
        DoNotDisturb {
            #[arg(action = ArgAction::Set)]
            value: bool
        },
        #[command(visible_alias = "vo")]
        Volume {
            #[arg(value_parser = clap::value_parser!(u8).range(cast_range(ranges::VOLUME)))]
            value: u8
        },
        #[command(visible_alias = "tr")]
        Treble {
            #[arg(value_parser = clap::value_parser!(u8).range(cast_range(ranges::TREBLE)))]
            value: u8
        },
        #[command(visible_alias = "ba")]
        Bass {
            #[arg(value_parser = clap::value_parser!(u8).range(cast_range(ranges::BASS)))]
            value: u8
        },
        #[command(visible_alias = "bl")]
        Balance {
            #[arg(value_parser = clap::value_parser!(u8).range(cast_range(ranges::BALANCE)))]
            value: u8
        },
        #[command(visible_alias = "ch")]
        Source {
            #[arg(value_parser = clap::value_parser!(u8).range(cast_range(ranges::SOURCE)))]
            value: u8
        },
        #[command(visible_alias = "kp")]
        KeypadConnected {
            #[arg(action = ArgAction::Set)]
            value: bool
        },
    }

    impl Into<ZoneAttribute> for AdjustableAttributeCommand {
        fn into(self) -> ZoneAttribute {
            match self {
                AdjustableAttributeCommand::Power { value } => ZoneAttribute::Power(value),
                AdjustableAttributeCommand::Mute { value } => ZoneAttribute::Mute(value),
                AdjustableAttributeCommand::DoNotDisturb { value } => ZoneAttribute::DoNotDisturb(value),
                AdjustableAttributeCommand::Volume { value } => ZoneAttribute::Volume(value),
                AdjustableAttributeCommand::Treble { value } => ZoneAttribute::Treble(value),
                AdjustableAttributeCommand::Bass { value } => ZoneAttribute::Bass(value),
                AdjustableAttributeCommand::Balance { value } => ZoneAttribute::Balance(value),
                AdjustableAttributeCommand::Source { value } => ZoneAttribute::Source(value),
                AdjustableAttributeCommand::KeypadConnected { value } => ZoneAttribute::KeypadConnected(value),
            }
        }
    }

    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None, multicall = true)]
    #[command(propagate_version = true)]
    #[command(name = "")]
    enum ReplCommands {
        /// Print zone status
        Status,

        /// Adjust zone attributes
        #[command(name = "set", subcommand_value_name = "ATTRIBUTE", subcommand_help_heading = "Attributes")]
        AdjustZone {
            zone: ZoneId,
            #[command(subcommand)]
            attribute: AdjustableAttributeCommand
        },

        /// Set public announcement state
        #[command(name = "pa")]
        PublicAnnouncement {
            #[arg(action = ArgAction::Set)]
            state: bool
        }
    }

    #[derive(Helper, Highlighter, Validator, Hinter, Completer)]
    struct ReplHelper {}

    // impl rustyline::completion::Completer for ReplHelper {
    //     type Candidate = String;

    //     fn complete(
    //         &self,
    //         line: &str,
    //         pos: usize,
    //         ctx: &rustyline::Context<'_>,
    //     ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
    //         let _ = (line, pos, ctx);

    //         let binding = ReplCommands::command();
    //         let subcommands = binding.get_subcommands();

    //         let names = subcommands.map(|c| c.get_name().to_string()).collect();

    //         Ok((0, names))
    //     }
    // }

    // impl rustyline::hint::Hinter for ReplHelper {
    //     type Hint = String;

    //     fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
    //         let _ = (line, pos, ctx);

    //         // let binding = ReplCli::command();
    //         // let mut subcommands = binding.get_subcommands();

    //         // let hint = subcommands.find_map(|c| {
    //         //     let name = c.render_usage().to_string();

    //         //     if name.starts_with(line) {
    //         //         Some(name[pos..].to_string())
    //         //     } else {
    //         //         None
    //         //     }
    //         // });

    //         None
    //     }
    // }

    fn status(amp: &emu::Amp) {
        use stybulate::{Table, Style, Cell, Headers};

        let mut zone_ids = amp.zones.keys().collect::<Vec<_>>();
        zone_ids.sort();

        fn bar(value: u8, range: RangeInclusive<u8>) -> String {
            format!("[{}{}] ({}/{})", "█".repeat(value.into()), "░".repeat((range.end() - value).into()), value, range.end())
        }

        fn slider(value: u8, range: RangeInclusive<u8>, offset: u8) -> String {
            fn bar(l: usize) -> String {"─".repeat(l)}
            format!("[{}◉{}] ({}/{})", bar((value - 1).into()), bar((value - 1).into()), value, range.end())
        }

        let cells = zone_ids.iter().map(|id| {
            fn str_cell<'a, T: ToString>(v: T) -> Cell<'a> {
                Cell::from(v.to_string().as_str())
            }

            fn int_cell<'a, T: Into<i32>>(v: T) -> Cell<'a> {
                Cell::Int(v.into())
            }

            let zone = amp.zones.get(id).expect("known key not found");

            vec![
                str_cell(id),
                str_cell(zone.public_announcement),
                str_cell(zone.power),
                str_cell(zone.mute),
                str_cell(zone.do_not_disturb),
                str_cell(bar(zone.volume, common::zone::ranges::VOLUME)),
                //str_cell(slider(zone.treble + 7, ZoneAttributeDiscriminants::Treble.io_range()))
                //int_cell(zone.volume)

            ]
        }).collect();

        println!("{}", Table::new(
            Style::Plain,
            cells,
            Some(Headers::from(vec!["Zone", "P.A.", "Power", "Mute", "D.N.D.", "Volume"]))
        ).tabulate());
    }

    pub fn main(amp: Arc<Mutex<emu::Amp>>) -> Result<()> {
        let config = rustyline::Config::builder()
            .auto_add_history(true)
            .completion_type(CompletionType::List)
            .build();

        let mut editor: Editor<ReplHelper, rustyline::history::FileHistory> = Editor::with_config(config)?;
        editor.set_helper(Some(ReplHelper {}));

        loop {
            let line = editor.readline("amp> ");
            match line {
                Ok(line) => {
                    let cmd = ReplCommands::try_parse_from(line.split(" "));

                    {
                        let mut amp = amp.lock().unwrap();

                        match cmd {
                            Ok(cmd) => {
                                match cmd {
                                    ReplCommands::Status => status(&amp),
                                    ReplCommands::AdjustZone { zone, attribute } => amp.zone_set(zone, attribute.into()),
                                    ReplCommands::PublicAnnouncement { state } => amp.set_pa_state(state),
                                    _ => todo!()
                                }
                            },
                            Err(e) => {
                                println!("{e}");
                            },
                        }
                    }

                },
                Err(_) => {
                    println!("readline error...");
                    break;
                }
            }
        }

        Ok(())
    }
}

mod serial {
    use super::*;

    use anyhow::{Context, bail};

    use regex::Regex;

    use std::{io::{Read, Write}, str};

    pub fn run<S: Read + Write>(amp: Arc<Mutex<emu::Amp>>, mut stream: S) -> Result<()> {
        enum Command {
            ZoneEnquriry(ZoneId),
            ZoneAttributeEnquiry(ZoneId, ZoneAttributeDiscriminants),
            ZoneSet(ZoneId, ZoneAttribute)
        }

        fn parse_command(buffer: &[u8]) -> Result<Option<Command>> {
            let cmd = str::from_utf8(buffer)?.to_uppercase();

            if cmd.len() == 0 { return Ok(None) }

            // TODO: convert to static
            let zone_enquiry_re = Regex::new(r"\?(\d\d)").unwrap();
            let zone_attr_enquiry_re = Regex::new(r"\?(\d\d)(\w\w)").unwrap();
            let zone_set_re = Regex::new(r"<(\d\d)(\w\w)(\d\d)").unwrap();
            let baud_set_re = Regex::new(r"<(\d+)").unwrap();

            macro_rules! capture_group {
                ( $captures:ident, $i:expr ) => {
                    $captures.get($i).expect(concat!("capture group ", $i)).as_str()
                }
            }

            fn zone_id(captures: &regex::Captures) -> Result<ZoneId> {
                let zone = capture_group!(captures, 1)
                    .parse().context("expected a valid zone id")?;

                if let ZoneId::System = zone {
                    bail!("system zone not supported")
                }

                Ok(zone)
            }

            let cmd = if let Some(captures) = zone_enquiry_re.captures(&cmd) {
                // zone enquiry
                let zone = zone_id(&captures)?;

                Command::ZoneEnquriry(zone)

            } else if let Some(captures) = zone_attr_enquiry_re.captures(&cmd) {
                // zone attribute enquiry
                let zone = zone_id(&captures)?;

                let attr = capture_group!(captures, 2);

                let attr = match attr {
                    "PR" => ZoneAttributeDiscriminants::Power,
                    "MU" => ZoneAttributeDiscriminants::Mute,
                    "DT" => ZoneAttributeDiscriminants::DoNotDisturb,
                    "VO" => ZoneAttributeDiscriminants::Volume,
                    "TR" => ZoneAttributeDiscriminants::Treble,
                    "BS" => ZoneAttributeDiscriminants::Bass,
                    "BL" => ZoneAttributeDiscriminants::Balance,
                    "CH" => ZoneAttributeDiscriminants::Source,
                    _ => return Ok(None) // unknown attribute results in a nop
                };

                Command::ZoneAttributeEnquiry(zone, attr)

            } else if let Some(captures) = zone_set_re.captures(&cmd) {
                // zone set
                let zone = zone_id(&captures)?;

                let attr = capture_group!(captures, 2);

                let value: u8 = capture_group!(captures, 3)
                    .parse().context("expected a valid value")?;

                let attr = match attr {
                    "PR" | "MU" | "DT" => {
                        let value = match value {
                            0 => false,
                            1 => true,
                            _ => return Ok(None) // invalid bool results in a nop
                        };

                        match attr {
                            "PR" => ZoneAttribute::Power(value),
                            "MU" => ZoneAttribute::Mute(value),
                            "DT" => ZoneAttribute::DoNotDisturb(value),
                            _ => unreachable!()
                        }
                    },
                    "VO" => ZoneAttribute::Volume(value),
                    "TR" => ZoneAttribute::Treble(value),
                    "BS" => ZoneAttribute::Bass(value),
                    "BL" => ZoneAttribute::Balance(value),
                    "CH" => ZoneAttribute::Source(value),
                    _ => return Ok(None) // unknown attribute results in a nop
                };

                if let Err(err) = attr.validate() {
                    // out of range values result in a nop
                    log::warn!("serial command \"{}\": warning: {}. nop.", cmd, err);
                    return Ok(None)
                }

                Command::ZoneSet(zone, attr)

            } else if let Some(captures) = baud_set_re.captures(&cmd) {
                let baud: u16 = capture_group!(captures, 1)
                    .parse().context("expected a valid baud rate")?;

                // todo
                bail!("baud rate change unimplemented.");
                //return Ok(None)

            } else {
                bail!("unknown command: {}", cmd)
            };

            Ok(Some(cmd))
        }
        
        let mut cmd_buffer = Vec::with_capacity(256);

        loop {
            loop {
                let mut ch = [0; 1];
                let n = stream.read(&mut ch)?;

                if n == 0 {
                    return Ok(());
                }

                match ch[0] {
                    // printable ASCII
                    0x20..=0x7F => {
                        // echo the byte back and append to buffer
                        stream.write(&ch)?; 
                        cmd_buffer.extend_from_slice(&ch);

                        if cmd_buffer.len() == 70 {
                            cmd_buffer.clear();
                            break
                        }
                    },

                    // Backspace
                    0x08 => {
                        // delete a byte from the cmd buffer and write control chars
                        if cmd_buffer.len() > 0 {
                            stream.write(b"\x08\x20\x08")?;
                            cmd_buffer.pop();
                        }
                    }

                    // CR
                    0x0D => break, // handle command

                    // ESC
                    0x1B => {
                        // clear the cmd buffer and handle (will result in a nop)
                        cmd_buffer.clear();
                        break
                    }

                    _ => {}  // ignore
                }
            }

            {
                let mut amp = amp.lock().unwrap();

                match parse_command(&cmd_buffer) {
                    Ok(cmd) => {
                        match cmd {
                            Some(Command::ZoneEnquriry(zone)) => {
                                for (id, zone) in amp.zone_enquiry(zone) {
                                    write!(stream, "\r\n#>{}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}",
                                        id,
                                        zone.public_announcement as u8,
                                        zone.power as u8,
                                        zone.mute as u8,
                                        zone.do_not_disturb as u8,
                                        zone.volume,
                                        zone.treble,
                                        zone.bass,
                                        zone.balance,
                                        zone.source,
                                        zone.keypad_connected as u8
                                    )?
                                }
                            },
                            Some(Command::ZoneAttributeEnquiry(zone, attr)) => {
                                for (id, zone) in amp.zone_enquiry(zone) {
                                    let (attr, value) = match attr {
                                        ZoneAttributeDiscriminants::PublicAnnouncement => ("PA", zone.public_announcement as u8),
                                        ZoneAttributeDiscriminants::Power => ("PR", zone.power as u8),
                                        ZoneAttributeDiscriminants::Mute => ("MU", zone.mute as u8),
                                        ZoneAttributeDiscriminants::DoNotDisturb => ("DT", zone.do_not_disturb as u8),
                                        ZoneAttributeDiscriminants::Volume => ("VO", zone.volume),
                                        ZoneAttributeDiscriminants::Treble => ("TR", zone.treble),
                                        ZoneAttributeDiscriminants::Bass => ("BA", zone.bass),
                                        ZoneAttributeDiscriminants::Balance => ("BL", zone.balance),
                                        ZoneAttributeDiscriminants::Source => ("CH", zone.source),
                                        ZoneAttributeDiscriminants::KeypadConnected => ("LS", zone.keypad_connected as u8),
                                    };

                                    write!(stream, "\r\n#>{}{}{:02}", id, attr, value)?;
                                }
                            }
                            Some(Command::ZoneSet(zone, attribute)) => {
                                amp.zone_set(zone, attribute)
                            },
                            None => {}
                        }
                    },
                    Err(err) => {
                        let cmd = String::from_utf8_lossy(&cmd_buffer);
                        println!("serial command \"{}\": error: {:#}", cmd, err);
                        
                        stream.write(b"\r\n#\r\nCommand Error.")?;
                    }
                };
            }

            cmd_buffer.clear();

            stream.write(b"\r\n#")?;
        }
    }
}


#[derive(Parser)]
struct Arguments {
    /// address to listen on for "serial" commands 
    #[arg(default_value = "0.0.0.0:9955")]
    address: String,

    /// number of amplifiers to emulate [1..3]
    #[arg(long, default_value_t = 1)]
    #[arg(value_parser = clap::value_parser!(u8).range(1..3))]
    amps: u8
}


fn main() -> Result<()> {
    let args = Arguments::parse();

    let amp = Arc::new(Mutex::new(emu::Amp::new(args.amps)));

    thread::spawn({
        let amp = amp.clone();

        move || {
            let listener = TcpListener::bind(args.address).unwrap();

            for stream in listener.incoming() {
                let stream = stream.unwrap();
                let addr = stream.peer_addr();

                log::info!("got connection from {:?}", addr);

                if let Err(err) = serial::run(amp.clone(), stream) {
                    log::error!("error handling request for {:?}: {}", addr, err);
                }
            }
        }
    });

    repl::main(amp.clone())
}