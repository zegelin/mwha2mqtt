use std::{collections::HashMap, sync::{mpsc::Sender, Arc, Mutex}};

use common::{ids::SourceId, mqtt::{MqttConnectionManager, PayloadDecodeError}, zone::{ZoneAttribute, ZoneId}};
use rumqttc::Publish;

use anyhow::Result;

use crate::{config::{SourceConfig, ZoneConfig}, ChannelMessage, amp::ZoneStatus};





pub fn install_source_shairport_handlers(zones_config: &HashMap<ZoneId, ZoneConfig>, sources_config: &HashMap<SourceId, SourceConfig>,
                                         mqtt: &mut MqttConnectionManager, zones_status: Arc<Mutex<Vec<ZoneStatus>>>, send: Sender<ChannelMessage>) -> Result<()>
{
    for (source_id, config) in sources_config {
        match &config.shairport_topic_prefix {
            Some(topic_prefix) => {
                let volume_topic = format!("{topic_prefix}volume");

                let handler = {
                    let volume_topic = volume_topic.clone();
                    let source_id = source_id.clone();
                    let zones_status = zones_status.clone();
                    let zones_config = zones_config.clone();
                    let send = send.clone();
    
                    move |_publish: &Publish, payload: Result<&str, PayloadDecodeError>| {
                        match payload {
                            Ok(payload) => {
                                let mut fields = payload.split(',').map(str::parse::<f32>);
    
                                let airplay_volume = fields.next();
    
                                match airplay_volume {
                                    Some(Ok(airplay_volume)) => {
                                        log::info!("source {source_id}: AirPlay volume changed to {airplay_volume}");

                                        for zone in zones_status.lock().expect("lock zone_statuses").iter() {
                                            let send_attr = |attr: ZoneAttribute| {
                                                send.send(ChannelMessage::ChangeZoneAttribute(zone.zone_id, attr)).unwrap(); // TODO: handler error
                                            };

                                            if !zone.matches(ZoneAttribute::Source((&source_id).into())) {
                                                 continue; // only zones listening to this AirPlay source get their volume adjusted
                                            }
    
                                            let muted = zone.matches(ZoneAttribute::Mute(true));
    
                                            let zone_config = zones_config.get(&zone.zone_id);

                                            if let Some(zone_config) = zone_config {
                                                match airplay_volume {
                                                    db if db == -144.0 => {
                                                        send_attr(ZoneAttribute::Mute(true));
                                                    },
                                                    db if db >= -30.00 && db <= 0.0 => {
                                                        let mut vol = ((1.0 - (db / -30.0)) * (zone_config.shairport.max_volume as f32)) as u8;
        
                                                        //vol = vol + zone_config.shairport.volume_offset;

                                                        if muted {
                                                            send_attr(ZoneAttribute::Mute(false))
                                                        }

                                                        log::info!("zone {} on source {source_id}: adjusting volume to {vol}", zone.zone_id);
            
                                                        send_attr(ZoneAttribute::Volume(vol));
                                                    },
                                                    other_db => {
                                                        log::error!("airplay_volume out of range: {other_db}")
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Some(Err(e)) => log::error!("{volume_topic}: failed to parse AirPlay volume \"{payload}\": {e}"),
                                    None => log::error!("{volume_topic}: failed to parse AirPlay volume \"{payload}\""),
                                }
                                
                            },
                            Err(e) => log::error!("{volume_topic}: {e}"),
                        }
                    }
                };

                mqtt.subscribe_utf8(volume_topic, rumqttc::QoS::AtLeastOnce, handler)?;
            },
            None => continue,
        }
    }

    Ok(())
}