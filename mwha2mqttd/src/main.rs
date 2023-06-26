mod config;
mod amp;
mod serial;
mod liquid_heck;

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
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
use anyhow::bail;
use common::mqtt::MqttConnectionManager;
use common::zone::ZoneAttribute;
use common::zone::ZoneAttributeDiscriminants;

use clap::CommandFactory;
use clap::Parser;
use clap::builder::PathBufValueParser;
use clap::command;
use clap::builder::TypedValueParser;

use config::AmpConfig;
use config::Config;
use config::MqttConfig;
use config::ZoneConfig;

use config::ZoneId;
use figment::value::magic::RelativePathBuf;
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
use rumqttc::tokio_rustls::rustls::Certificate;
use rumqttc::tokio_rustls::rustls::ClientConfig;
use rumqttc::tokio_rustls::rustls::PrivateKey;
use rumqttc::tokio_rustls::rustls::RootCertStore;
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
    let mut url = if config.srv_lookup {
        let Some(host) = config.url.host_str() else {
            bail!("a hostname is required for SRV lookups")
        };
        
        let name = match config.url.scheme() {
            "mqtt" => "_mqtt._tcp",
            "mqtts" => "_secure-mqtt._tcp",
            scheme => bail!("only 'mqtt' and 'mqtts' URL schemes are supported for SRV lookup (got: '{}')", scheme)
        };

        let name = format!("{}.{}", name, host);

        todo!("srv support!");

        let url = config.url.clone();
        // url.set_host(Some("foo"))?;
        // url.set_port(Some(1883))?;

        url

    } else {
        config.url.clone()

    };

    {
        let mut query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();

        // set a default client id, unless specified in the config
        if !query.contains_key("client_id") {
            query.insert("client_id".to_string(), "mwha2mqttd".to_string());
        }

        // overwrite the URL query string
        url.query_pairs_mut()
            .clear()
            .extend_pairs(query);
    }

    let mut options = MqttOptions::try_from(url)?;

    // configure TLS
    if let rumqttc::Transport::Tls(_) = options.transport() {
        let mut root_store = RootCertStore::empty();
        {
            if let Some(ca_certs_path) = &config.ca_certs {
                let ca_certs_path = ca_certs_path.relative();

                let certs = File::open(&ca_certs_path)
                    .map(BufReader::new)
                    .and_then(|mut r| rustls_pemfile::certs(&mut r))
                    .with_context(|| format!("failed to open ca_certs file {}", ca_certs_path.display()))?;

                if certs.len() == 0 {
                    bail!("no certificates found in ca_certs file {}", &ca_certs_path.display());
                }

                for (i, cert) in certs.into_iter().enumerate() {
                    root_store.add(&Certificate(cert))
                        .with_context(|| format!("failed to load certificate {} from ca_certs file {}", i, &ca_certs_path.display()))?;
                }

            } else {
                // use system trust store
                for cert in rustls_native_certs::load_native_certs().context("could not load platform certs")? {
                    root_store.add(&Certificate(cert.0)).unwrap();
                }
            }
        }

        let tls_cfg_builder = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store);

        let tls_config = if let Some(client_certs_path) = &config.client_certs {
            let client_certs_path = client_certs_path.relative();

            let mut rd = File::open(&client_certs_path)
                .map(BufReader::new)
                .with_context(|| format!("failed to open client_certs file {}", &client_certs_path.display()))?;

            let mut client_certs = Vec::new();
            let mut client_key = None;

            loop {
                match rustls_pemfile::read_one(&mut rd)? {
                    None => break,
                    Some(rustls_pemfile::Item::X509Certificate(cert)) => client_certs.push(Certificate(cert)),
                    Some(rustls_pemfile::Item::PKCS8Key(key)) => {
                        if let Some(_) = client_key {
                            bail!("multiple private keys found in client_certs file {}", client_certs_path.display());

                        } else {
                            client_key = Some(key)
                        }
                    }, 
                    _ => {}
                }
            }

            let client_key = match client_key {
                Some(client_key) => PrivateKey(client_key),
                None => {
                    if let Some(client_key_path) = &config.client_key {
                        let client_key_path = client_key_path.relative();

                        let mut keys = File::open(&client_key_path)
                            .map(BufReader::new)
                            .and_then(|mut r| rustls_pemfile::pkcs8_private_keys(&mut r))
                            .with_context(|| format!("failed to open client_key file {}", client_key_path.display()))?;
    
                        match keys.len() {
                            0 => bail!("no private keys found in client_key file {}", client_key_path.display()),
                            1 => PrivateKey(keys.remove(0)),
                            _ => bail!("multiple private keys found in client_key file {}", client_key_path.display()),
                        }
                    } else {
                        bail!("client_cert ({}) doesn't contain a private key and client_key is unset", &client_certs_path.display());
                    }
                }
            };

            tls_cfg_builder.with_single_cert(client_certs, client_key)
                .context("invalid client certificate chain and/or private key")?

        } else {

            tls_cfg_builder.with_no_client_auth()
        };

        options.set_transport(rumqttc::Transport::Tls(tls_config.into()));
    }

    let topic_base = match config.url.path() {
        "" => "mwha/".to_string(),
        "/" => "".to_string(),
        other => {
            let base = other.strip_prefix("/").unwrap_or(other);

            if base.ends_with("/") {
                base.to_string()
            } else {
                format!("{}/", base)
            }
        }
    };

    options.set_last_will(LastWill::new(format!("{}connected", topic_base), "0", rumqttc::QoS::AtLeastOnce, true));

    let (client, connection) = Client::new(options, 10);

    Ok((
        client.clone(),
        MqttConnectionManager::new(client, connection),
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
        Box::new(AmpSerialPort::new(&serial.device, serial.baud, serial.adjust_baud, serial.reset_baud, serial.common.read_timeout)?)

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

            let topic = format!("{}set/zone/{}/{}", topic_base, zone_id, attr.to_string().to_kebab_case());

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

                    send.send(ChannelMessage::ZoneStatusChanged(zone_id, attr)).unwrap(); // todo: handle channel send error?
                }
            };

            debug!("subscribibing to {}", topic);
            mqtt.subscribe(topic, rumqttc::QoS::AtLeastOnce, Box::new(handler))?;
        }
    }

    Ok(())
}

fn publish_metadata(mqtt: &mut Client, config: &Config, topic_base: &str) -> Result<()> {
    mqtt.publish(format!("{}connected", topic_base), rumqttc::QoS::AtLeastOnce, true, "2")?;

    // source metadata
    for (source_id, source_config) in &config.amp.sources {
        let topic_base = format!("{}status/source/{}", topic_base, source_id);

        mqtt.publish_json(format!("{}/name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.name))?;
        mqtt.publish_json(format!("{}/enabled", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(source_config.enabled))?;
    }

    // list of active zones
    mqtt.publish_json("{}/status/zones", rumqttc::QoS::AtLeastOnce, true, json!(config.amp.zones.keys().map(|z| z.into()).collect::<Vec<u8>>()))?;

    // zone metadata
    for (zone_id, zone_config) in &config.amp.zones {
        let topic_base = format!("{}status/zone/{}", topic_base, zone_id);

        mqtt.publish_json(format!("{}/name", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(zone_config.name))?;

        let zone_type = match zone_id {
            ZoneId::Zone {..} => "zone",
            ZoneId::Amp(_) => "amp",
            ZoneId::System => "system",
        };

        mqtt.publish_json(format!("{}/type", topic_base), rumqttc::QoS::AtLeastOnce, true, json!(zone_type))?;
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
                    let topic = format!("{}status/zone/{}/{}", topic_base, zone_status.id, attr_name);

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
    SimpleLogger::init(LevelFilter::Info, simplelog::Config::default()).unwrap();

    let args = Args::parse();

    let config = config::load_config(&args.config_file)?;


    let (mut mqtt_client, mut mqtt_cm, topic_base) = connect_mqtt(&config.mqtt)?;

    let amp = connect_amp(&config)?;

    // todo: better channel sender/receiver names
    let (send, recv) = mpsc::channel::<ChannelMessage>();

    install_zone_attribute_subscription_handers(&config.amp.zones, &mut mqtt_cm, &topic_base, send.clone())?;

    let amp_worker_thread = spawn_amp_worker(&config.amp, amp, mqtt_client.clone(), &topic_base, recv);

    publish_metadata(&mut mqtt_client, &config, &topic_base)?;

    let mut signals = Signals::new(TERM_SIGNALS)?;
    signals.forever().next(); // wait for a signal

    info!("caught shutdown signal");

    mqtt_client.disconnect()?;

    send.send(ChannelMessage::Poison)?;
    amp_worker_thread.join().unwrap();

    Ok(())
}