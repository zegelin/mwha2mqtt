use std::{collections::HashMap, sync::{Arc, Mutex}};

use common::{mqtt::MqttConnectionManager};
use rumqttc::{Publish, QoS};


enum Connected {

}

enum ZoneType {
    Zone,
    Amp,
    System
}

struct SourceStatus {
    name: Option<String>,

    enabled: Option<bool>
}

impl Default for SourceStatus {
    fn default() -> Self {
        Self {
            name: None,
            enabled: None
        }
    }
}

enum ZoneStatus {
    Zone {
        name: Option<String>,

        public_announcement: Option<bool>,
        power: Option<bool>,
        mute: Option<bool>,
        do_not_disturb: Option<bool>,
        volume: Option<u8>,
        treble: Option<u8>,
        bass: Option<u8>,
        balance: Option<u8>,
        source: Option<u8>,
        keypad_connected: Option<bool>
    },
    Amp {
        name: Option<String>
    },
    System {
        name: Option<String>
    }
}

struct SourceId(String);

struct ZoneId(String);

struct Status {
    connected: Option<Connected>,

    sources: HashMap<SourceId, SourceStatus>,
    zones: HashMap<ZoneId, ZoneStatus>
}

impl Default for Status {
    fn default() -> Self {
        //let default_sources = SourceId::all().map(|id| (id, SourceStatus::default())).collect();

        Self { 
            connected: None,
            sources: HashMap::new(),
            zones: HashMap::new()
        }
    }
}

pub struct Client {
    status: Arc<Mutex<Status>>
}


impl Client {
    pub fn new() -> Self {
        Client {
            status: Arc::new(Mutex::new(Status::default()))
        }
    }

    // pub fn set_zone_attribute(&self, )


    pub fn setup_status_handlers<'a>(&self, mqtt: Arc<Mutex<MqttConnectionManager>>) {
        let topic_base = "mwha/status/";

        // for source in SourceId::all() {
        //     mqtt.lock().unwrap().subscribe_json(format!("{}/source/{}/name", topic_base, source), QoS::AtLeastOnce, |publish: Publish, name: String| {

        //         self.status

        //         println!("{}: name: {}", source, name);

        //     });
    
        //     mqtt.subscribe_json(format!("{}/source/{}/enabled", topic_base, source), QoS::AtLeastOnce, |publish: Publish, enabled: bool| {
                
        //     });
        // }

        

        mqtt.lock().unwrap().subscribe_json(format!("{}zones", topic_base), QoS::AtLeastOnce, {
            let mqtt = mqtt.clone();

            move |publish: &Publish, zones: &Vec<String>| {
                // self.status.lock().unwrap().zones.keys()

                dbg!(zones);

                let mut mqtt = mqtt.lock().unwrap();

                for zone in zones {
                    let topic_base = format!("{}/zone/{}/", topic_base, zone);

                    mqtt.subscribe_json(format!("{}name", topic_base), QoS::AtLeastOnce, |publish: &Publish, name: &String| {
                        // self.status.lock().unwrap().zones.get(zone).unwrap().name

                    }).unwrap();

                    mqtt.subscribe_json(format!("{}type", topic_base), QoS::AtLeastOnce, |publish: &Publish, zone_type: &String| {



                    }).unwrap();



                    
                }
            }
        }).unwrap();

        // handle out-of-order zones:  status/zones contains list of active zones, however we may get messages
        // about zones we dont care about. how to handle?
        // doesn't matter -- we only install handlers for zones after we get the zone list
        //  the initial subscibe will only register handlers to get values for zones we care about
        //  later, if the zone list changes, we can delete items from the zone list
        //  handlers therefor should never add to the zone list -- it's an error to do so

    }
}

