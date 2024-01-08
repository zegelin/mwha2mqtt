use std::{collections::HashMap, sync::{Arc, Mutex}, str::FromStr, error::Error};

use common::{mqtt::MqttConnectionManager, ids::SourceId, zone::{ZoneId, ZoneAttribute, ZoneIdError}};
use crossbeam_channel::Sender;
use rumqttc::{Publish, QoS};

#[derive(Debug)]
pub enum Connected {

}

#[derive(Debug)]
pub enum SourceMeta {
    Name(String)
}

#[derive(Debug)]
pub enum ZoneMeta {
    Name(String)
}

#[derive(Debug)]
pub enum StatusUpdate {
    Connected(Connected),
    AvailableZones(Vec<ZoneId>),
    ZoneMeta(ZoneId, ZoneMeta),
    ZoneAttribute(ZoneId, ZoneAttribute),
    Error()
}




// enum ZoneType {
//     Zone,
//     Amp,
//     System
// }

// struct SourceStatus {
//     name: Option<String>,

//     enabled: Option<bool>
// }

// impl Default for SourceStatus {
//     fn default() -> Self {
//         Self {
//             name: None,
//             enabled: None
//         }
//     }
// }

// enum ZoneStatus {
//     Zone {
//         name: Option<String>,

//         public_announcement: Option<bool>,
//         power: Option<bool>,
//         mute: Option<bool>,
//         do_not_disturb: Option<bool>,
//         volume: Option<u8>,
//         treble: Option<u8>,
//         bass: Option<u8>,
//         balance: Option<u8>,
//         source: Option<u8>,
//         keypad_connected: Option<bool>
//     },
//     Amp {
//         name: Option<String>
//     },
//     System {
//         name: Option<String>
//     }
// }


// struct Status {
//     connected: Option<Connected>,

//     sources: HashMap<SourceId, SourceStatus>,
//     zones: HashMap<ZoneId, ZoneStatus>
// }

// impl Default for Status {
//     fn default() -> Self {
//         //let default_sources = SourceId::all().map(|id| (id, SourceStatus::default())).collect();

//         Self { 
//             connected: None,
//             sources: HashMap::new(),
//             zones: HashMap::new()
//         }
//     }
// }

pub struct Client {
}


impl Client {
    pub fn new() -> Self {
        Client {
        }
    }

    // pub fn set_zone_attribute(&self, )


    pub fn setup_status_handlers<>(&self, mqtt: Arc<Mutex<MqttConnectionManager>>, updates_send: Sender<StatusUpdate>) {
        let topic_base = "mwha/status/";

        // for source in SourceId::all() {
        //     mqtt.lock().unwrap().subscribe_json(format!("{}/source/{}/name", topic_base, source), QoS::AtLeastOnce, |publish: Publish, name: String| {

        //         self.status

        //         println!("{}: name: {}", source, name);

        //     });
    
        //     mqtt.subscribe_json(format!("{}/source/{}/enabled", topic_base, source), QoS::AtLeastOnce, |publish: Publish, enabled: bool| {
                
        //     });
        // }

        

        // mqtt.lock().unwrap().subscribe_json(format!("{}zones", topic_base), QoS::AtLeastOnce, {
        //     let mqtt = mqtt.clone();

        //     move |publish: &Publish, zones: Vec<String>| {
        //         let zones = zones.into_iter()
        //             .map(|zone| ZoneId::from_str(&zone))
        //             .collect::<Result<Vec<ZoneId>, ZoneIdError>>();

        //         let zones = match zones {
        //             Ok(zones) => zones,
        //             Err(e) => {
        //                 log::error!("{}: {}", publish.topic, e);
        //                 updates_send.send(StatusUpdate::Error()).expect("send on updates_send");
        //                 return;
        //             }
        //         };

        //         updates_send.send(StatusUpdate::AvailableZones(zones.clone())).expect("send on updates_send");

        //         // TODO: implement unsubscribe for zones that are no longer in the available zones list
                

        //         let mut mqtt = mqtt.lock().unwrap();

        //         for zone in zones {
        //             dbg!(zone);
        //             let topic_base = format!("{}zone/{}/", topic_base, zone);

        //             mqtt.subscribe_json(format!("{}name", topic_base), QoS::AtLeastOnce, {
        //                 let updates_send = updates_send.clone();

        //                 move |_publish: &Publish, name: String| {
        //                     updates_send.send(StatusUpdate::ZoneMeta(zone, ZoneMeta::Name(name)))
        //                         .expect("send on updates_send");
        //                 }
        //             }).unwrap();

        //             // System and Amp zones don't receive attribute status updates
        //             // is there a way to do if-let-or? or something better
        //             if let ZoneId::Zone { amp: _, zone: _ } = zone {
        //             } else {
        //                 continue;
        //             }

        //             mqtt.subscribe_json(format!("{}public-announcement", topic_base), QoS::AtLeastOnce, {
        //                 let updates_send = updates_send.clone();

        //                 move |_publish: &Publish, pa: bool| {
        //                     updates_send.send(StatusUpdate::ZoneAttribute(zone, ZoneAttribute::PublicAnnouncement(pa)))
        //                         .expect("send on updates_send");
        //                 }
        //             }).unwrap();

        //             mqtt.subscribe_json(format!("{}volume", topic_base), QoS::AtLeastOnce, {
        //                 let updates_send = updates_send.clone();

        //                 move |_publish: &Publish, volume: u8| {
        //                     updates_send.send(StatusUpdate::ZoneAttribute(zone, ZoneAttribute::Volume(volume)))
        //                         .expect("send on updates_send");
        //                 }
        //             }).unwrap();
        //         }

                
        //     }
        // }).unwrap();

        // handle out-of-order zones:  status/zones contains list of active zones, however we may get messages
        // about zones we dont care about. how to handle?
        // doesn't matter -- we only install handlers for zones after we get the zone list
        //  the initial subscibe will only register handlers to get values for zones we care about
        //  later, if the zone list changes, we can delete items from the zone list
        //  handlers therefor should never add to the zone list -- it's an error to do so

    }
}

