
// work in progress emulator

use std::{collections::HashMap, net::TcpListener};

use clap::{command, Subcommand, Parser, ArgAction};
use anyhow::Result;
use common::zone::{ZoneAttribute, ZoneAttributeDiscriminants, ZoneId};


// use clap::{Parser, Subcommand, ArgAction, CommandFactory};

// use strum_macros::{EnumDiscriminants, Display, EnumVariantNames, EnumIter};



mod emu {
    use anyhow::{Context, bail};
    use common::zone::MAX_ZONES_PER_AMP;
    use regex::Regex;

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
    
        pub fn zone_set(&mut self, zone: ZoneId, attribute: ZoneAttribute) {
            for zone in zone.to_zones() {
                match self.zones.get_mut(&zone) {
                    Some(zone) => zone.set(attribute),
                    None => todo!(),
                }
            }
        }

        pub fn zone_enquiry(&mut self, zone: ZoneId) -> Vec<(ZoneId, Zone)> {
            todo!()
            // zone.to_zones().into_iter().
        }
    
        pub fn set_pa_state(&mut self, pa: bool) {
            for zone in self.zones.values_mut() {
                zone.public_announcement = pa;
            } 
        }

        pub fn run<S: Read + Write>(&mut self, mut stream: S) -> Result<()> {
            enum Command {
                ZoneEnquriry(ZoneId),
                ZoneAttributeEnquiry(ZoneId, ZoneAttributeDiscriminants),
                ZoneSet(ZoneId, ZoneAttribute)
            }

            fn parse_command(buffer: &[u8]) -> Result<Option<Command>> {
                let cmd = str::from_utf8(&buffer[0..buffer.len() - 1])?;

                if cmd.len() == 0 { return Ok(None) }

                // TODO: convert to static
                let zone_enquiry_re = Regex::new(r"\?(\d\d)").unwrap();
                let zone_attr_enquiry_re = Regex::new(r"\?(\d\d)(\w\w)").unwrap();
                let zone_set_re = Regex::new(r"<(\d\d)(\w\w)(\d\d)").unwrap();
                let baud_set_re = Regex::new(r"<(\d+)").unwrap();


                let cmd = if let Some(captures) = zone_enquiry_re.captures(cmd) {
                    // zone enquiry
                    let zone = captures.get(1).expect("capture group 1").as_str()
                        .parse().context("expected a valid zone id")?;

                    Command::ZoneEnquriry(zone)

                } else if let Some(captures) = zone_attr_enquiry_re.captures(cmd) {
                    // zone attribute enquiry
                    let zone = captures.get(1).expect("capture group 1").as_str()
                        .parse().context("expected a valid zone id")?;

                    let attr = captures.get(2).expect("capture group 2").as_str();

                    let attr = match attr {
                        "PR" => ZoneAttributeDiscriminants::Power,
                        "MU" => ZoneAttributeDiscriminants::Mute,
                        "DT" => ZoneAttributeDiscriminants::DoNotDisturb,
                        "VO" => ZoneAttributeDiscriminants::Volume,
                        "TR" => ZoneAttributeDiscriminants::Treble,
                        "BS" => ZoneAttributeDiscriminants::Bass,
                        "BL" => ZoneAttributeDiscriminants::Balance,
                        "CH" => ZoneAttributeDiscriminants::Source,
                        _ => bail!("unknown attribute: {}", attr)
                    };

                    Command::ZoneAttributeEnquiry(zone, attr)

                } else if let Some(captures) = zone_set_re.captures(cmd) {
                    // zone set

                    let zone = captures.get(1).expect("capture group 1").as_str()
                        .parse().context("expected a valid zone id")?;

                    let attr = captures.get(2).expect("capture group 2").as_str();

                    let value: u8 = captures.get(3).expect("capture group 3").as_str()
                        .parse().context("expected a valid value")?;

                    // TODO: what happens on the real device if the value is out of range?

                    let attr = match attr {
                        "PR" => ZoneAttribute::Power(value != 0),
                        "MU" => ZoneAttribute::Mute(value != 0),
                        "DT" => ZoneAttribute::DoNotDisturb(value != 0),
                        "VO" => ZoneAttribute::Volume(value),
                        "TR" => ZoneAttribute::Treble(value),
                        "BS" => ZoneAttribute::Bass(value),
                        "BL" => ZoneAttribute::Balance(value),
                        "CH" => ZoneAttribute::Source(value),
                        _ => bail!("unknown attribute: {}", attr)
                    };

                    Command::ZoneSet(zone, attr)

                } else if let Some(captures) = baud_set_re.captures(cmd) {
                    let baud: u16 = captures.get(1).expect("capture group 1").as_str()
                        .parse().context("expected a valid baud rate")?;

                    todo!()

                } else {
                    bail!("unknown command: {}", cmd)
                };

                Ok(Some(cmd))
            }
            
            let mut buffer = Vec::with_capacity(256);

            loop {
                {
                    // read a byte, echo it back, and append it to the buffer.
                    // unless a CR was sent, keep reading
                    let mut ch = [0; 1];
                    stream.read(&mut ch)?;
                    stream.write(&ch)?; // echo the byte back

                    buffer.extend_from_slice(&ch);

                    if ch[0] != b'\r' { continue }

                    // CR always gets translated to CRLF -- last char sent was a CR (and was echo'd), send LF...
                    stream.write(b"\n")?;  // .write(b"\n#")
                }

                match parse_command(&buffer) {
                    Ok(cmd) => {
                        match cmd {
                            Some(Command::ZoneEnquriry(zone)) => {
                                for (id, zone) in self.zone_enquiry(zone) {
                                    write!(stream, ">{}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}{:02}\r\n",
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
                                for (id, zone) in self.zone_enquiry(zone) {
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

                                    write!(stream, ">{}{}{:02}\r\n", id, attr, value)?;
                                }
                            }
                            Some(Command::ZoneSet(zone, attribute)) => {
                                self.zone_set(zone, attribute)
                            },
                            None => {}
                        }

                        stream.write(b"#")?
                    },
                    Err(err) => {
                        let cmd = String::from_utf8_lossy(&buffer);
                        println!("error proccessing command \"{}\": {:#}", cmd, err);
                        
                        stream.write(b"\r\nCommand Error.")?
                    }
                };

                buffer.clear();
            }
        }
    }
}


mod repl {
    use std::ops::RangeInclusive;

    use super::*;
    
    use rustyline::{DefaultEditor, Editor, CompletionType, Completer};
    use rustyline::{Helper, Hinter, Validator, Highlighter};

    #[derive(Subcommand, Debug)]
    enum AdjustableAttributeCommand {
        // PA is ommitted bacuase on real hardware PA can only be toggled for all zones simultaneously

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
            #[arg(value_parser = clap::value_parser!(u8).range(0..=38))]  // ZoneAttributeDiscriminants::Volume.io_range().into()
            value: u8
        },
        #[command(visible_alias = "tr")]
        Treble {
            value: u8
        },
        #[command(visible_alias = "ba")]
        Bass {
            value: u8
        },
        #[command(visible_alias = "bl")]
        Balance {
            value: u8
        },
        #[command(visible_alias = "ch")]
        Source {
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
                str_cell(bar(zone.volume, ZoneAttributeDiscriminants::Volume.io_range())),
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

    pub fn main(amp: &mut emu::Amp) -> Result<()> {
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
                            println!("error: {}", e);
                        },
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


#[derive(Parser)]
struct Arguments {
    amps: u8
}


fn main() -> Result<()> {
    //let args = Arguments::parse();

    let mut amp = emu::Amp::new(1);

    let listener = TcpListener::bind("127.0.0.1:9955")?;

    for stream in listener.incoming() {
        let stream = stream?;
        println!("Got connection {:?}", stream.peer_addr());
        amp.run(stream)?;
    }

    Ok(())



    // repl::main(&amp)
    
}