use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::channel;

use client::Client;
use common::mqtt::{MqttConfig, MqttConnectionManager};
use anyhow::Result;
use anyhow::Context;
use simplelog::LevelFilter;
use simplelog::SimpleLogger;



fn connect_mqtt(config: &MqttConfig) -> Result<(rumqttc::Client, MqttConnectionManager, String)> {
    use rumqttc::Client;

    let mut options = common::mqtt::options_from_config(config, "mwha2mqttd")?;

    let topic_base = config.topic_base("mwha");

    let (client, connection) = Client::new(options, 10);

    let mgr = MqttConnectionManager::new(client.clone(), connection);

    mgr.wait_connected().with_context(|| format!("failed to connect to MQTT broker {}", config.url))?;

    Ok((
        client.clone(),
        mgr,
        topic_base
    ))
}

fn main() -> Result<()> {
    SimpleLogger::init(LevelFilter::Debug, simplelog::Config::default()).unwrap();


    let mqtt_config = MqttConfig {
        url: url::Url::parse("mqtt://localhost")?,
        srv_lookup: false,
        ca_certs: None,
        client_certs: None,
        client_key: None,
    };

    println!("Connecting to MQTT");
    let (mut mqtt_client, mut mqtt_cm, topic_base) = connect_mqtt(&mqtt_config).context("failed to establish MQTT connection")?;

    let mqtt_cm = Arc::new(Mutex::new(mqtt_cm));

    let client = Client::new();
    println!("Subscribing");
    client.setup_status_handlers(mqtt_cm);


    let (sender, receiver) = channel();

    receiver.recv()?;

    Ok(())
}
