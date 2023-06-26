use std::{sync::{Arc, Mutex}, collections::HashMap, thread::{self, JoinHandle}};

use log::{warn, error, info};
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
    topic_handlers: Arc<Mutex<HashMap<String, HandlerFn>>>,
    handler_thread: JoinHandle<()>
}

impl MqttConnectionManager {
    pub fn new(client: Client, connection: Connection) -> MqttConnectionManager {
        let topic_handlers = Arc::new(Mutex::new(HashMap::new()));

        let handler_thread = MqttConnectionManager::spawn_handler_thread(connection, topic_handlers.clone());

        MqttConnectionManager {
            client,
            topic_handlers: topic_handlers,
            handler_thread
        }
    }

    fn spawn_handler_thread(mut connection: Connection, topic_handlers: Arc<Mutex<HashMap<String, HandlerFn>>>) -> JoinHandle<()> {
        thread::Builder::new()
            .name("MQTT notification handler".to_string())
            .spawn(move || {
                for notification in connection.iter() {
                    match notification {
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            // todo: condvar to notify start
                        },
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            // incoming message for a subscription

                            match topic_handlers.lock().expect("lock topic_handlers").get(&publish.topic) {
                                Some(handler) => handler(publish),
                                None => warn!("received MQTT Publish packet for unknown subscription. topic = {}", publish.topic),
                            }
                        },
                        Ok(Event::Outgoing(rumqttc::Outgoing::Disconnect)) => {
                            return
                        },
                        Err(e) => {
                            error!("MQTT error occured: {}", e);
                            panic!();
                        },
                        _ => ()
                    }
                }
            }).expect("spawn MQTT notification handler thread")
    }

    pub fn subscribe(&mut self, topic: String, qos: rumqttc::QoS, handler: HandlerFn) -> Result<(), rumqttc::ClientError> {
        self.topic_handlers.lock().unwrap().insert(topic.clone(), handler);
        self.client.subscribe(&topic, qos)
    }
}