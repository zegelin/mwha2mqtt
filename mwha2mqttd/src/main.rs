mod config;
mod amp;
mod serial;
mod liquid_heck;

use std::collections::HashMap;
use std::collections::HashSet;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use amp::Amp;
use amp::Port;
use common::mqtt::MqttConnectionManager;
use common::zone::ZoneAttribute;
use common::zone::ZoneAttributeDiscriminants;

use clap::CommandFactory;
use clap::Parser;
use clap::builder::PathBufValueParser;
use clap::command;
use clap::builder::TypedValueParser;

use common::zone::ZoneId;
use config::AmpConfig;
use config::Config;
use config::MqttConfig;
use config::ZoneConfig;

use log::LevelFilter;
use log::debug;
use log::error;
use log::info;
use log::warn;
use regex::Regex;
use rumqttc::Client;
use rumqttc::Connection;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::LastWill;
use rumqttc::MqttOptions;
use rumqttc::Packet;
use rumqttc::Publish;
use rumqttc::RecvTimeoutError;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use serial::AmpSerialPort;
use serialport;
use serialport::SerialPort;

use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::Signals;
use simplelog::SimpleLogger;
use strum_macros::{EnumDiscriminants, Display, EnumVariantNames, EnumIter};
use strum::IntoEnumIterator;
use strum::VariantNames;

use heck::ToKebabCase;

use std::io;
use std::cmp;
use std::str;

use anyhow::{Context, Result};

use common::mqtt::PublishJson;

// use crate::ids::ZoneId;
// use crate::mqtt::MqttConnectionManager;
// use crate::mqtt::PublishJson;


const DEFAULT_CONFIG_FILE_PATH: &str = match option_env!("DEFAULT_CONFIG_FILE_PATH") {
    Some(v) => v,
    None => "config.toml"
};


#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg[long, default_value=DEFAULT_CONFIG_FILE_PATH]]
    config_file: PathBuf
}


fn connect_mqtt(config: &MqttConfig) -> Result<(Client, MqttConnectionManager), Box<dyn std::error::Error>> {
    let mut options = MqttOptions::parse_url(&config.url)?;

    options.set_last_will(LastWill::new("mwha/connected", "0", rumqttc::QoS::AtLeastOnce, true));

    let (client, connection) = Client::new(options, 10);

    Ok((
        client.clone(),
        MqttConnectionManager::new(client, connection)
    ))
}


/// establish a connection to the amp, via either serial or TCP
fn connect_amp(config: &Config) -> Result<Amp, Box<dyn std::error::Error>> {
    let port: Box<dyn Port> = if let Some(tcp) = &config.tcp {
        let stream = TcpStream::connect(&tcp.address)?;
        stream.set_read_timeout(Some(tcp.common.read_timeout))?;

        Box::new(stream)

    } else if let Some(serial) = &config.serial {
        Box::new(AmpSerialPort::new(&serial.device, serial.baud, serial.adjust_baud, serial.reset_baud, serial.common.read_timeout)?)

    } else {
        panic!("either serial or tcp port configuration required") // todo: replace panic with error
    };

    Ok(Amp::new(port)?)
}

enum ChannelMessage {
    ZoneStatusChanged(ZoneId, ZoneAttribute),
    Poison
}


/// install zone attribute mqtt subscriptons
fn install_zone_attribute_subscription_handers(zones_config: &HashMap<ZoneId, ZoneConfig>, mqtt: &mut MqttConnectionManager, send: Sender<ChannelMessage>) -> Result<()> {
    for (&zone_id, _) in zones_config {
        for attr in ZoneAttributeDiscriminants::iter() {
            // don't subscribe/install handlers for read-only attributes
            if attr.read_only() { continue };

            let topic = format!("mwha/set/zone/{}/{}", zone_id, attr.to_string().to_kebab_case());

            let handler = {
                let topic = topic.clone();
                let send = send.clone();

                move |publish: Publish| {
                    let payload = match str::from_utf8(&publish.payload) {
                        Ok(s) => s,
                        Err(err) => {
                            error!("{}: received payload is not valid UTF-8: {}", topic, err);
                            return;
                        },
                    };

                    let de_bool = || serde_json::from_str::<bool>(payload);
                    let de_u8 = || serde_json::from_str::<u8>(payload);

                    let attr = match attr {
                        ZoneAttributeDiscriminants::Power => de_bool().map(ZoneAttribute::Power),
                        ZoneAttributeDiscriminants::Mute => de_bool().map(ZoneAttribute::Mute),
                        ZoneAttributeDiscriminants::DoNotDisturb => de_bool().map(ZoneAttribute::DoNotDisturb),
                        ZoneAttributeDiscriminants::Volume => de_u8().map(ZoneAttribute::Volume),
                        ZoneAttributeDiscriminants::Treble => de_u8().map(ZoneAttribute::Treble),
                        ZoneAttributeDiscriminants::Bass => de_u8().map(ZoneAttribute::Bass),
                        ZoneAttributeDiscriminants::Balance => de_u8().map(ZoneAttribute::Balance),
                        ZoneAttributeDiscriminants::Source => de_u8().map(ZoneAttribute::Source),
                        _ => unreachable!("read-only attributes should never have subscription handlers")
                    };

                    let attr = match attr {
                        Ok(attr) => attr,
                        Err(err) => {
                            error!("{}: unable to decode payload: {}", topic, err);
                            return;
                        }
                    };

                    send.send(ChannelMessage::ZoneStatusChanged(zone_id, attr)).unwrap(); // todo: handle channel error?
                }
            };

            debug!("subscribibing to {}", topic);
            mqtt.subscribe(topic, rumqttc::QoS::AtLeastOnce, Box::new(handler))?;
        }
    }

    Ok(())
}

fn publish_metadata(mqtt: &mut Client, config: &Config) -> Result<()> {
    mqtt.publish("mwha/connected", rumqttc::QoS::AtLeastOnce, true, "2")?;

    // source metadata
    for (source_id, source_config) in &config.amp.sources {
        let topic_base = format!("mwha/status/source/{}", source_id);

        mqtt.publish_json(format!("{}/name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.name))?;
        mqtt.publish_json(format!("{}/enabled", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.enabled))?;
    }

    // list of active zones
    mqtt.publish_json("mwha/status/zones", rumqttc::QoS::AtLeastOnce, true, json!(config.amp.zones.keys().collect::<Vec<_>>()))?;

    // zone metadata
    for (zone_id, zone_config) in &config.amp.zones {
        let topic_base = format!("mwha/status/zone/{}", zone_id);

        mqtt.publish_json(format!("{}/name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(zone_config.name))?;
    }

    Ok(())
}

/// spawn a worker thread that processes incoming zone attribute adjustments and periodically polls the amp for status updates
fn spawn_amp_worker(config: &AmpConfig, mut amp: Amp, mqtt: rumqttc::Client, recv: Receiver<ChannelMessage>) -> JoinHandle<()> {
    let amp_ids = config.zones.keys().map(ZoneId::to_amp).collect::<HashSet<ZoneId>>();

    let poll_interval = config.poll_interval;

    let mut mqtt = mqtt.clone();

    thread::spawn(move || {
        let mut previous_statuses: HashMap<ZoneId, amp::ZoneStatus> = HashMap::new();

        loop {
            let mut adjustments = HashMap::new();

            // wait for an incoming zone attribute adjustment with a timeout.
            // if a timeout occurs do a zone status refresh anyway (poll the amp)
            match recv.recv_timeout(poll_interval) {
                Ok(ChannelMessage::ZoneStatusChanged(id, attr)) => { adjustments.insert((id, std::mem::discriminant(&attr)), (id, attr)); }
                Ok(ChannelMessage::Poison) => { return },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (), // timeout waiting for command, refresh zone status anyway
                Err(other) => panic!("got other {:?}", other)
            };

            // drain the channel.
            // mqtt can deliver faster than the serialport can handle and multiple adjustments may have come while processing the last request.
            // there is no point adjusting the same attribute multiple times.
            // newer attribute adjustments queued for the same zone overwrite earlier ones.
            // todo: any way to combine with above match? (probably not, given different return types of recv_timeout and try_recv)
            loop {
                match recv.try_recv() {
                    Ok(ChannelMessage::ZoneStatusChanged(id, attr)) => { adjustments.insert((id, std::mem::discriminant(&attr)), (id, attr)); },
                    Ok(ChannelMessage::Poison) => { return },
                    Err(e) if e == std::sync::mpsc::TryRecvError::Empty => break,
                    Err(other) => panic!("got other {:?}", other)
                }
            }

            // apply zone attribute adjustments, if any
            for (id, attr) in adjustments.values().into_iter() {
                debug!("adjust {} = {:?}", id, attr);
                amp.set_zone_attribute(*id, *attr).unwrap(); // TODO: handle error more gracefully
            }

            // get zone statuses for active amps
            let mut zone_statuses = Vec::new();
            for amp_id in &amp_ids {
                zone_statuses.extend(amp.zone_enquiry(*amp_id).unwrap()); // TODO: handle error more gracefully
            }
    
            for zone_status in zone_statuses {
                // todo: don't publish status updates for disabled zones
                // if config.amp.zones.keys().find(predicate)zone_status.id

                let previous_status = previous_statuses.get(&zone_status.id);

                for attr in &zone_status.attributes {
                    // don't publish if zone attribute hasn't changed
                    if previous_status.map_or(false, |ps| ps.attributes.iter().any(|pa| *pa == *attr)) {
                        continue;
                    }

                    let attr_name = ZoneAttributeDiscriminants::from(attr).to_string().to_kebab_case();
                    let topic = format!("mwha/status/zone/{}/{}", zone_status.id, attr_name);

                    // todo: is there a cleaner way to do this except putting #[serde(untagged)] on the enum?
                    let value = match attr {
                        ZoneAttribute::PublicAnnouncement(v) => json!(v),
                        ZoneAttribute::Power(v) => json!(v),
                        ZoneAttribute::Mute(v) => json!(v),
                        ZoneAttribute::DoNotDisturb(v) => json!(v),
                        ZoneAttribute::Volume(v) => json!(v),
                        ZoneAttribute::Treble(v) => json!(v),
                        ZoneAttribute::Bass(v) => json!(v),
                        ZoneAttribute::Balance(v) => json!(v),
                        ZoneAttribute::Source(v) => json!(v),
                        ZoneAttribute::KeypadConnected(v) => json!(v),
                    };

                    debug!("{} = {}", topic, value);
        
                    mqtt.publish_json(topic, rumqttc::QoS::AtLeastOnce, true, value).unwrap();
                }

                previous_statuses.insert(zone_status.id, zone_status);
            }
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::init(LevelFilter::Debug, simplelog::Config::default()).unwrap();

    let args = Args::parse();

    let config = config::load_config(&args.config_file)?;

    let (mut mqtt_client, mut mqtt_cm) = connect_mqtt(&config.mqtt)?;

    let amp = connect_amp(&config)?;

    // todo: better channel sender/receiver names
    let (send, recv) = mpsc::channel::<ChannelMessage>();

    install_zone_attribute_subscription_handers(&config.amp.zones, &mut mqtt_cm, send)?;

    let t = spawn_amp_worker(&config.amp, amp, mqtt_client.clone(), recv);

    publish_metadata(&mut mqtt_client, &config)?;

    let mut signals = Signals::new(TERM_SIGNALS)?;
    signals.forever().next(); // wait for a signal

    println!("Caught shutdown signal");

    mqtt_client.disconnect()?;

    t.join().unwrap();

    Ok(())
}