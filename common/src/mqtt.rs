use std::{sync::{Arc, Mutex}, collections::HashMap, thread};

use log::warn;
use rumqttc::{Client, Publish, Connection, Event, Packet};
use serde_json::Value;


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

type HandlerFn = Box<dyn Fn(Publish) + Send>;

/// handles MQTT notifications and topic subscriptions, delegating incoming packets to regestered topic handlers 
pub struct MqttConnectionManager {
    client: Client,
    topic_handlers: Arc<Mutex<HashMap<String, HandlerFn>>>
}

impl MqttConnectionManager {
    pub fn new(client: Client, connection: Connection) -> MqttConnectionManager {
        let manager = MqttConnectionManager {
            client,
            topic_handlers: Arc::new(Mutex::new(HashMap::new()))
        };

        MqttConnectionManager::spawn_handler_thread(connection, manager.topic_handlers.clone());

        manager
    }

    fn spawn_handler_thread(mut connection: Connection, topic_handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(Publish) + Send>>>>) {
        thread::Builder::new()
            .name("MQTT notification handler".to_string())
            .spawn(move || {
                for notification in connection.iter() {
                    match notification {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            // incoming message for a subscription

                            match topic_handlers.lock().expect("lock topic_handlers").get(&publish.topic) {
                                Some(handler) => handler(publish),
                                None => warn!("Received MQTT Publish packet for unknown subscription. topic = {}", publish.topic),
                            }
                        },
                        Err(_) => todo!(),
                        _ => () // todo: are there other notification types that need handling?
                    }
                }
            }).expect("spawn MQTT notification handler thread");
    }

    pub fn subscribe(&mut self, topic: String, qos: rumqttc::QoS, handler: HandlerFn) -> Result<(), rumqttc::ClientError> {
        self.topic_handlers.lock().unwrap().insert(topic.clone(), handler);
        self.client.subscribe(&topic, qos)
    }
}