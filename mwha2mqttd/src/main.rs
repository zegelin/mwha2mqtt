mod config;
mod amp;
mod serial;

use std::collections::HashMap;
use std::collections::HashSet;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;

use amp::Amp;
use amp::Port;
use anyhow::bail;
use common::mqtt::MqttConfig;
use common::mqtt::MqttConnectionManager;
use common::zone::ZoneAttribute;
use common::zone::ZoneAttributeDiscriminants;

use clap::Parser;
use clap::command;

use config::AmpConfig;
use config::Config;
use config::ZoneConfig;

use config::ZoneId;
use log::LevelFilter;
use rumqttc::Client;
use rumqttc::LastWill;
use rumqttc::Publish;
use serde_json::json;
use serial::AmpSerialPort;

use signal_hook::consts::TERM_SIGNALS;
use signal_hook::iterator::Signals;
use simplelog::SimpleLogger;
use strum::IntoEnumIterator;

use heck::ToKebabCase;

use std::str;

use anyhow::{Context, Result};

use common::mqtt::PublishJson;


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

fn connect_mqtt(config: &MqttConfig) -> Result<(Client, MqttConnectionManager, String)> {
    let mut options = common::mqtt::options_from_config(config, "mwha2mqttd")?;

    let topic_base = config.topic_base("mwha");

    options.set_last_will(LastWill::new(format!("{}connected", topic_base), "0", rumqttc::QoS::AtLeastOnce, true));

    let (client, connection) = Client::new(options, 10);

    let mgr = MqttConnectionManager::new(client.clone(), connection);

    mgr.wait_connected().with_context(|| format!("failed to connect to MQTT broker {}", config.url))?;

    Ok((
        client.clone(),
        mgr,
        topic_base
    ))
}


/// establish a connection to the amp, via either serial or TCP
fn connect_amp(config: &Config) -> Result<Amp> {
    let port: Box<dyn Port> = if let Some(tcp) = &config.tcp {
        let stream = TcpStream::connect(&tcp.address)?;
        stream.set_read_timeout(Some(tcp.common.read_timeout))?;

        Box::new(stream)

    } else if let Some(serial) = &config.serial {
        let serial = AmpSerialPort::new(&serial.device, serial.baud, serial.adjust_baud, serial.reset_baud, serial.common.read_timeout)
            .context("failed to open serial port")?;

        Box::new(serial)

    } else {
        bail!("either serial or tcp port configuration required")
    };

    Ok(Amp::new(port)?)
}

enum ChannelMessage {
    ZoneStatusChanged(ZoneId, ZoneAttribute),
    Poison
}


/// install zone attribute mqtt subscriptons
fn install_zone_attribute_subscription_handers(zones_config: &HashMap<ZoneId, ZoneConfig>, mqtt: &mut MqttConnectionManager, topic_base: &str, send: Sender<ChannelMessage>) -> Result<()> {
    for (&zone_id, _) in zones_config {
        for attr in ZoneAttributeDiscriminants::iter() {
            // don't subscribe/install handlers for read-only attributes
            if attr.read_only() { continue };

            let topic = format!("{}/set/zone/{}/{}", topic_base, zone_id, attr.to_string().to_kebab_case());

            // {
            //     use ZoneAttributeDiscriminants::*;

            //     match attr {
            //         PublicAnnouncement => {
            //             mqtt.subscribe_json(topic, qos, handler)
            //         },
            //         Power => todo!(),
            //         Mute => todo!(),
            //         DoNotDisturb => todo!(),
            //         Volume => todo!(),
            //         Treble => todo!(),
            //         Bass => todo!(),
            //         Balance => todo!(),
            //         Source => todo!(),
            //         KeypadConnected => todo!(),
            //     }
            // }


            // todo: maybe invert this so the enum match is on the outside?
            let handler = {
                let topic = topic.clone();
                let send = send.clone();

                move |publish: &Publish| {
                    let payload = match str::from_utf8(&publish.payload) {
                        Ok(s) => s,
                        Err(err) => {
                            log::error!("{}: received payload is not valid UTF-8: {}", topic, err);
                            return;
                        },
                    };

                    let de_bool = || serde_json::from_str::<bool>(payload);
                    let de_u8 = || serde_json::from_str::<u8>(payload);

                    let attr = {
                        use ZoneAttributeDiscriminants::*;

                        match attr {
                            Power => de_bool().map(ZoneAttribute::Power),
                            Mute => de_bool().map(ZoneAttribute::Mute),
                            DoNotDisturb => de_bool().map(ZoneAttribute::DoNotDisturb),
                            Volume => de_u8().map(ZoneAttribute::Volume),
                            Treble => de_u8().map(ZoneAttribute::Treble),
                            Bass => de_u8().map(ZoneAttribute::Bass),
                            Balance => de_u8().map(ZoneAttribute::Balance),
                            Source => de_u8().map(ZoneAttribute::Source),
                            _ => unreachable!("read-only attributes should never have subscription handlers")
                        }
                    };

                    let attr = match attr {
                        Ok(attr) => attr,
                        Err(err) => {
                            log::error!("{}: unable to decode payload: {}", topic, err);
                            return;
                        }
                    };

                    send.send(ChannelMessage::ZoneStatusChanged(zone_id, attr)).unwrap(); // todo: handle channel send error?
                }
            };

            log::debug!("subscribibing to {}", topic);
            mqtt.subscribe(topic, rumqttc::QoS::AtLeastOnce, handler)?;
        }
    }

    Ok(())
}

fn publish_metadata(mqtt: &mut Client, config: &Config, topic_base: &str) -> Result<()> {
    mqtt.publish(format!("{}connected", topic_base), rumqttc::QoS::AtLeastOnce, true, "2")?;

    // source metadata
    for (source_id, source_config) in &config.amp.sources {
        let topic_base = format!("{}status/source/{}/", topic_base, source_id);

        mqtt.publish_json(format!("{}name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.name))?;
        mqtt.publish_json(format!("{}enabled", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.enabled))?;
    }

    // list of active zones
    mqtt.publish_json(format!("{}status/zones", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(config.amp.zones.keys().map(|z| z.to_string()).collect::<Vec<_>>()))?;

    // zone metadata
    for (zone_id, zone_config) in &config.amp.zones {
        let topic_base = format!("{}status/zone/{}/", topic_base, zone_id);

        mqtt.publish_json(format!("{}name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(zone_config.name))?;

        let zone_type = match zone_id {
            ZoneId::Zone {..} => "zone",
            ZoneId::Amp(_) => "amp",
            ZoneId::System => "system",
        };

        mqtt.publish_json(format!("{}type", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(zone_type))?;
    }

    Ok(())
}

/// spawn a worker thread that processes incoming zone attribute adjustments and periodically polls the amp for status updates
fn spawn_amp_worker(config: &AmpConfig, mut amp: Amp, mqtt: rumqttc::Client, topic_base: &str, recv: Receiver<ChannelMessage>) -> JoinHandle<()> {
    // get the zones specifically configured for publish (ignore amps and system)
    let zone_ids = config.zones.keys().filter_map(|z| match z {
        ZoneId::Zone { amp, zone } => Some(common::zone::ZoneId::Zone { amp: *amp, zone: *zone }),
        _ => None,
    }).collect::<HashSet<_>>();

    // coalesce zone ids into amp ids (for bulk query)
    let amp_ids = zone_ids.iter().map(common::zone::ZoneId::to_amp).collect::<HashSet<common::zone::ZoneId>>();

    let poll_interval = config.poll_interval;
    let topic_base = topic_base.to_string();

    let mut mqtt = mqtt.clone();

    thread::spawn(move || {
        let mut previous_statuses: HashMap<common::zone::ZoneId, amp::ZoneStatus> = HashMap::new();

        loop {
            let mut adjustments = HashMap::new();

            {
                // wait for an incoming zone attribute adjustment with a timeout.
                // if a timeout occurs do a zone status refresh anyway (poll the amp)
                let mut msg = match recv.recv_timeout(poll_interval) {
                    Ok(msg) => Some(msg),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None, // timeout waiting for command, refresh zone status anyway
                    Err(other) => panic!("got other {:?}", other)
                };

                // drain the channel.
                // mqtt can deliver faster than the serialport can handle and multiple adjustments may have come while processing the last request.
                // there is no point adjusting the same attribute multiple times.
                // newer attribute adjustments queued for the same zone overwrite earlier ones.
                loop {
                    match msg {
                        Some(ChannelMessage::ZoneStatusChanged(id, attr)) => { adjustments.insert((id, std::mem::discriminant(&attr)), (id, attr)); }
                        Some(ChannelMessage::Poison) => { return },
                        None => break
                    }

                    msg = match recv.try_recv() {
                        Ok(msg) => Some(msg),
                        Err(std::sync::mpsc::TryRecvError::Empty) => None,
                        Err(other) => panic!("got other {:?}", other)
                    };
                }
            }

            // apply zone attribute adjustments, if any
            for (id, attr) in adjustments.values().into_iter() {
                log::debug!("adjust {} = {:?}", id, attr);

                let ids = match *id {
                    ZoneId::Zone { amp, zone } => vec![common::zone::ZoneId::Zone { amp, zone }],
                    ZoneId::Amp(amp) => vec![common::zone::ZoneId::Amp(amp)],
                    ZoneId::System => vec![
                        common::zone::ZoneId::Amp(1),
                        common::zone::ZoneId::Amp(2),
                        common::zone::ZoneId::Amp(3)
                    ],
                };

                for id in ids {
                    amp.set_zone_attribute(id, *attr).unwrap(); // TODO: handle error more gracefully
                }
            }

            // get zone statuses for active amps
            let mut zone_statuses = Vec::new();
            for amp_id in &amp_ids {
                zone_statuses.extend(amp.zone_enquiry(*amp_id).unwrap()); // TODO: handle error more gracefully
            }
    
            for zone_status in zone_statuses {
                // don't publish status updates for disabled zones
                if !zone_ids.contains(&zone_status.id) {
                    continue;
                }

                let previous_status = previous_statuses.get(&zone_status.id);

                for attr in &zone_status.attributes {
                    // don't publish if zone attribute hasn't changed
                    if previous_status.map_or(false, |ps| ps.attributes.iter().any(|pa| *pa == *attr)) {
                        continue;
                    }

                    let attr_name = ZoneAttributeDiscriminants::from(attr).to_string().to_kebab_case();
                    let topic = format!("{}/status/zone/{}/{}", topic_base, zone_status.id, attr_name);

                    let value = {
                        use ZoneAttribute::*;

                        match attr {
                            PublicAnnouncement(b) | Power(b) | Mute(b) | DoNotDisturb(b) | KeypadConnected(b) => json!(b),
                            Volume(v) | Treble(v) | Bass(v) | Balance(v) | Source(v) => json!(v)
                        }
                    };

                    log::debug!("set {} = {}", topic, value);
        
                    mqtt.publish_json(topic, rumqttc::QoS::AtLeastOnce, true, value).unwrap(); // TODO: handle error more gracefully
                }

                previous_statuses.insert(zone_status.id, zone_status);
            }
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    SimpleLogger::init(LevelFilter::Info, simplelog::Config::default()).unwrap();

    let config = config::load_config(&args.config_file)?;

    let (mut mqtt_client, mut mqtt_cm, topic_base) = connect_mqtt(&config.mqtt).context("failed to establish MQTT connection")?;

    let amp = connect_amp(&config).context("failed to establish amp connection")?;

    // todo: better channel sender/receiver names
    let (send, recv) = mpsc::channel::<ChannelMessage>();

    install_zone_attribute_subscription_handers(&config.amp.zones, &mut mqtt_cm, &topic_base, send.clone())?;

    let amp_worker_thread = spawn_amp_worker(&config.amp, amp, mqtt_client.clone(), &topic_base, recv);

    publish_metadata(&mut mqtt_client, &config, &topic_base)?;



    let mut signals = Signals::new(TERM_SIGNALS)?;
    signals.forever().next(); // wait for a signal

    log::info!("caught shutdown signal");

    mqtt_client.disconnect()?;

    send.send(ChannelMessage::Poison)?;
    amp_worker_thread.join().unwrap();


    // exit due to: signal, mqtt error/disconnect, 

    Ok(())
}