use std::{sync::{Arc, Mutex}, collections::HashMap, thread::{self, JoinHandle}, fs::File, io::{BufReader}, env, path::{Path, PathBuf}, any, str::Utf8Error, fmt::Display};
use std::str;
use anyhow::{bail, Context};
use bytes::Bytes;
use crossbeam_channel::{Sender, Receiver, select};
use log::{warn, error, info};
use rumqttc::{Client, Publish, Connection, Event, Packet, MqttOptions, tokio_rustls::rustls::{RootCertStore, Certificate, ClientConfig, PrivateKey}, ConnectionError, Subscribe};
use serde_json::Value;
use serde::{Deserialize, de::DeserializeOwned};
use figment::value::magic::RelativePathBuf;


pub trait PublishJson {
    fn publish_json<S>(&mut self, topic: S, qos: rumqttc::QoS, retain: bool, value: Value) -> Result<(), rumqttc::ClientError> where 
        S: Into<String>;
    
}

impl PublishJson for Client {
    fn publish_json<S>(&mut self, topic: S, qos: rumqttc::QoS, retain: bool, value: Value) -> Result<(), rumqttc::ClientError> where
        S: Into<String>
    {
        self.publish(topic, qos, retain, value.to_string())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PayloadDecodeError {
    Utf8Error {
        topic: String,
        payload: Bytes,
        source: Utf8Error
    },
    // {}:  \"{}\" is not valid UTF-8: {}

    JsonError {
        topic: String,
        payload: Bytes,
        source: serde_json::Error
    }
}

impl Display for PayloadDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn printable_payload<'A>(p: &Bytes) -> String {
            let mut p = String::from_utf8_lossy(p);

            // if p.len() > 50 {
            //     p = 
            // }

            p.escape_default().to_string()
        }

        match self {
            PayloadDecodeError::Utf8Error { topic, payload, source } => {
                let payload = printable_payload(payload);
                write!(f, "{topic}: received payload \"{payload}\" is not valid UTF-8: {source}")
            }
            PayloadDecodeError::JsonError { topic, payload, source } => {
                let payload = printable_payload(payload);
                write!(f, "{topic}: received payload \"{payload}\" is not valid JSON: {source}")
            },
        }
    }
}

type HandlerFn = Box<dyn Fn(&Publish) + Send>;

type CoHashMap<A, B> = Arc<Mutex<HashMap<A, B>>>;

/// handles MQTT notifications and topic subscriptions, delegating incoming packets to regestered topic handlers 
pub struct MqttConnectionManager {
    client: Client,
    outgoing_topic_handlers_send: Sender<(String, HandlerFn)>,
    topic_handlers: CoHashMap<String, HandlerFn>,
    handler_thread: JoinHandle<()>,
    connected_recv: Receiver<()>,
    errors_recv: Receiver<ConnectionError>
}

impl MqttConnectionManager {
    pub fn new(client: Client, connection: Connection) -> MqttConnectionManager {
        let (outgoing_topic_handlers_send, outgoing_topic_handlers_recv) = crossbeam_channel::unbounded();
        let topic_handlers = Arc::new(Mutex::new(HashMap::new()));

        let (connected_send, connected_recv) = crossbeam_channel::bounded(1);
        let (errors_send, errors_recv) = crossbeam_channel::bounded(1);

        let handler_thread = MqttConnectionManager::spawn_handler_thread(
            connection,
            outgoing_topic_handlers_recv,
            topic_handlers.clone(),
            connected_send,
            errors_send
        );

        MqttConnectionManager {
            client,
            outgoing_topic_handlers_send,
            topic_handlers,
            handler_thread,
            connected_recv,
            errors_recv
        }
    }

    fn spawn_handler_thread(mut connection: Connection,
        outgoing_topic_handlers_recv: Receiver<(String, HandlerFn)>,
        topic_handlers: CoHashMap<String, HandlerFn>,
        connected_send: Sender<()>,
        errors_send: Sender<ConnectionError>
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("MQTT notification handler".to_string())
            .spawn(move || {
                let mut pending_topic_handlers = HashMap::new();

                for notification in connection.iter() {
                    log::debug!("mqtt notif: {:?}", notification);

                    match notification {
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            connected_send.send(()).expect("send on connected_send");
                        },
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            // incoming message for a subscription

                            // todo: handle wildcards
                            match topic_handlers.lock().expect("lock topic_handlers").get(&publish.topic) {
                                Some(handler) => handler(&publish),
                                None => log::warn!("received MQTT Publish packet for unknown subscription. topic = {}", publish.topic),
                            }
                        },
                        Ok(Event::Outgoing(rumqttc::Outgoing::Disconnect)) => {
                            // TODO: notify anyone waiting for disconnect
                            return
                        },

                        // deferred topic handler registration on suback
                        Ok(Event::Outgoing(rumqttc::Outgoing::Subscribe(pkid))) => {
                            let handler = outgoing_topic_handlers_recv.recv().expect("recv from outgoing_topic_handlers_recv");

                            pending_topic_handlers.insert(pkid, handler);
                        },
                        Ok(Event::Incoming(Packet::SubAck(suback))) => {
                            // TODO: handle suback.return_codes

                            let handler = pending_topic_handlers.remove(&suback.pkid);

                            match handler {
                                Some((topic, handler_fn)) => {
                                    topic_handlers.lock().expect("lock topic_handlers")
                                        .insert(topic, handler_fn);
                                },
                                None => log::warn!("received MQTT SubAck packet for unknown subscription"),
                            }
                        }

                        Ok(_) => {},
                        Err(e) => {
                            log::error!("mqtt error: {}", e);
                            errors_send.send(e).expect("send on errors_send");
                        },
                    }
                }
            }).expect("spawn MQTT notification handler thread")
    }

    pub fn wait_connected(&self) -> anyhow::Result<()> {
        // wait for a established connection or a connection error
        select! {
            recv(self.connected_recv) -> msg => Ok(msg?),
            recv(self.errors_recv) -> err => Err(err?.into())
        }
    }

    pub fn wait_disconnected(&self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn subscribe<F, S>(&mut self, topic: S, qos: rumqttc::QoS, handler: F) -> anyhow::Result<(), rumqttc::ClientError>
    where
        F: Fn(&Publish) + Send + 'static,
        S: Into<String>
    {
        let topic = topic.into();

        log::info!("subscribing to MQTT topic {}", topic);

        self.outgoing_topic_handlers_send.send((topic.clone(), Box::new(handler))).expect("send on outgoing_topic_handlers_send");
        self.client.subscribe(topic, qos)
    }

    pub fn subscribe_utf8<F, S>(&mut self, topic: S, qos: rumqttc::QoS, handler: F) -> Result<(), rumqttc::ClientError>
    where
        F: Fn(&Publish, Result<&str, PayloadDecodeError>) + Send + 'static,
        S: Into<String>
    {
        let topic = topic.into();

        let handler = {
            let topic = topic.clone();

            move |publish: &Publish|  {
                let payload = str::from_utf8(&publish.payload).map_err(|err| {
                    PayloadDecodeError::Utf8Error {
                        topic: topic.clone(),
                        payload: publish.payload.clone(),
                        source: err
                    }
                });

                handler(publish, payload)
            }
        };

        self.subscribe(topic, qos, handler)
    }

    pub fn subscribe_json<T, F, S>(&mut self, topic: S, qos: rumqttc::QoS, handler: F) -> Result<(), rumqttc::ClientError>
    where
        T: DeserializeOwned,
        F: Fn(&Publish, Result<T, PayloadDecodeError>) + Send + 'static,
        S: Into<String>
    {
        let topic = topic.into();

        let handler = {
            let topic = topic.clone();

            move |publish: &Publish|  {
                let payload = serde_json::from_slice(&publish.payload[..]).map_err(|err| {
                    PayloadDecodeError::JsonError {
                        topic: topic.clone(),
                        payload: publish.payload.clone(),
                        source: err
                    }
                });

                handler(publish, payload);
            }
        };
        
        self.subscribe(topic, qos, handler)
    }

    pub fn unsubscribe<S>(&mut self, topic: S) -> Result<(), rumqttc::ClientError>
    where
        S: Into<String>
    {
        todo!();
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct MqttConfig {
    pub url: url::Url,

    #[serde(default = "MqttConfig::default_srv_lookup")]
    pub srv_lookup: bool,

    pub ca_certs: Option<RelativePathBuf>,

    pub client_certs: Option<RelativePathBuf>,
    pub client_key: Option<RelativePathBuf>,
}

impl MqttConfig {
    fn default_srv_lookup() -> bool { false }

    pub fn topic_base(&self) -> Option<String> {
        match self.url.path() {
            "" => None,
            other => {
                Some(other.strip_prefix("/").unwrap_or(other).to_string())
            }
        }
    }
}

fn resolve_credentials_path(path: &RelativePathBuf) -> anyhow::Result<PathBuf> {
    let path = path.relative();

    if let Ok(path) = path.strip_prefix("$CREDENTIALS_DIRECTORY") {
        let var = env::var("CREDENTIALS_DIRECTORY")
            .with_context(|| format!("failed to expand $CREDENTIALS_DIRECTORY in path '{}'", path.display()))?;

        Ok(Path::new(&var).join(path))

    } else {
        Ok(path)
    }
}

pub fn options_from_config(config: &MqttConfig, default_client_id: &str) -> anyhow::Result<MqttOptions> {
    let mut url = if config.srv_lookup {
        todo!("srv support!");
        
        /*
        let Some(host) = config.url.host_str() else {
            bail!("a hostname is required for SRV lookups")
        };
        
        let name = match config.url.scheme() {
            "mqtt" => "_mqtt._tcp",
            "mqtts" => "_secure-mqtt._tcp",
            scheme => bail!("only 'mqtt' and 'mqtts' URL schemes are supported for SRV lookup (got: '{}')", scheme)
        };

        let name = format!("{}.{}", name, host);

        let url = config.url.clone();

        url
        */

    } else {
        config.url.clone()

    };

    {
        let mut query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();

        // set a default client id, unless specified in the config
        if !query.contains_key("client_id") {
            query.insert("client_id".to_string(), default_client_id.to_string());
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

        // load root CA certs into root store 
        {
            if let Some(ca_certs_path) = &config.ca_certs {
                let ca_certs_path = resolve_credentials_path(ca_certs_path).context("failed to locate ca_certs file")?;

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

        // configure client auth
        let tls_config = if let Some(client_certs_path) = &config.client_certs {
            let client_certs_path = resolve_credentials_path(client_certs_path).context("failed to locate client_certs file")?;

            let mut client_certs = Vec::new();
            let mut client_key = None;

            // load client certs (and optional private key)
            {
                let mut rd = File::open(&client_certs_path)
                    .map(BufReader::new)
                    .with_context(|| format!("failed to open client_certs file {}", &client_certs_path.display()))?;

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
            }

            // try to load a separate client key if no key was included in the certs file
            let client_key = match &config.client_key {
                Some(client_key_path) => {
                    let client_key_path = resolve_credentials_path(client_key_path).context("failed to locate client_key file")?;

                    let mut keys = File::open(&client_key_path)
                        .map(BufReader::new)
                        .and_then(|mut r| rustls_pemfile::pkcs8_private_keys(&mut r))
                        .with_context(|| format!("failed to open client_key file {}", client_key_path.display()))?;

                    match keys.len() {
                        0 => bail!("no private keys found in client_key file {}", client_key_path.display()),
                        1 => PrivateKey(keys.remove(0)),
                        _ => bail!("multiple private keys found in client_key file {}", client_key_path.display()),
                    }
                },
                None => {
                    match client_key {
                        Some(client_key) => PrivateKey(client_key),
                        None => bail!("client_cert ({}) doesn't contain a private key and client_key is unset", &client_certs_path.display()),
                    }
                }
            };

            tls_cfg_builder.with_single_cert(client_certs, client_key)
                .context("invalid client certificate chain and/or private key")?

        } else {

            tls_cfg_builder.with_no_client_auth()
        };

        options.set_transport(rumqttc::Transport::Tls(tls_config.into()));
    };

    Ok(options)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_credentials_path() {
        assert_eq!(resolve_credentials_path(&RelativePathBuf::from(Path::new("credentials"))).unwrap(), PathBuf::from("credentials"));

        assert_eq!(resolve_credentials_path(&RelativePathBuf::from(Path::new("$CREDENTIALS_DIRECTORY/credentials"))).is_err(), true);

        temp_env::with_var("CREDENTIALS_DIRECTORY", Some("/creds/"), || {
            assert_eq!(resolve_credentials_path(&RelativePathBuf::from(Path::new("$CREDENTIALS_DIRECTORY/credentials"))).unwrap(), PathBuf::from("/creds/credentials"));
        });
    }

    #[test]
    fn test_config_topic_base() {
        fn config_with_url(url: &str) -> MqttConfig {
            MqttConfig {
                url: url::Url::parse(url).unwrap(),
                srv_lookup: false,
                ca_certs: None,
                client_certs: None,
                client_key: None,
            }
        }

        assert_eq!(config_with_url("mqtt://localhost").topic_base(), None);
        assert_eq!(config_with_url("mqtt://localhost/").topic_base(), Some("".to_string()));
        assert_eq!(config_with_url("mqtt://localhost/base").topic_base(), Some("base".to_string()));
        assert_eq!(config_with_url("mqtt://localhost/base/").topic_base(), Some("base/".to_string()));
        assert_eq!(config_with_url("mqtt://localhost//base/").topic_base(), Some("/base/".to_string()));
    }
}