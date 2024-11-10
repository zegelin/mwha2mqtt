use std::{collections::HashMap, sync::{mpsc::Sender, Arc, Mutex}, cmp::min};

use common::{ids::SourceId, mqtt::{MqttConnectionManager, PayloadDecodeError}, zone::{ZoneAttribute, ZoneId, ranges}};
use rumqttc::Publish;

use anyhow::Result;

use crate::{config::{SourceConfig, ZoneConfig, ShairportConfig}, AmpControlChannelMessage, amp::ZoneStatus};

// adjust the volume of any zone listening to source id to the equivalent of the airplay volume
fn adjust_listening_zones_volume(
    zones_config: &HashMap<ZoneId, ZoneConfig>, zones_status: Arc<Mutex<Vec<ZoneStatus>>>,
    source_id: &SourceId,
    airplay_volume: f32,
    control_chan_sender: &Sender<AmpControlChannelMessage>,
    shairport_config: &ShairportConfig
) -> Result<()> {
    let zones_status = zones_status.lock().expect("lock zones_status");

    for zone in zones_status.iter() {
        if !zone.matches(ZoneAttribute::Source(source_id.into())) {
            continue; // only zones listening to this AirPlay source get their volume adjusted
        }

        let Some(zone_config) = zones_config.get(&zone.zone_id) else {
            continue;
        };

        let change_zone_attr = |attr: ZoneAttribute| {
            control_chan_sender.send(AmpControlChannelMessage::ChangeZoneAttribute(zone.zone_id, attr))
        };

        match airplay_volume {
            -144.0 => {
                // AirPlay mute (according to Shairport docs)
                change_zone_attr(ZoneAttribute::Mute(true))?;
            },
            db @ -30.0..=0.0 => {
                let max_vol = zone_config.shairport.max_volume.unwrap_or(shairport_config.max_zone_volume) as f32;
                let vol_offset = zone_config.shairport.volume_offset.unwrap_or(shairport_config.zone_volume_offset) as f32;

                // 0.0 = max, -30.0 = min
                let mut vol = ((1.0 - (db / -30.0)) * max_vol + vol_offset) as u8;
                vol = min(vol, *ranges::VOLUME.end()); // clamp

                // unmute if muted
                {
                    let muted = zone.matches(ZoneAttribute::Mute(true));
                
                    if muted {
                        change_zone_attr(ZoneAttribute::Mute(false))?;
                    }
                }

                log::info!("zone {} on source {source_id}: adjusting volume to {vol}", zone.zone_id);

                change_zone_attr(ZoneAttribute::Volume(vol))?;
            },
            other => {
                log::error!("airplay_volume out of range: {other}")
            }
        }
    }

    Ok(())
}

pub fn install_source_shairport_handlers(
    shairport_config: &ShairportConfig, zones_config: &HashMap<ZoneId, ZoneConfig>, sources_config: &HashMap<SourceId, SourceConfig>,
    mqtt: &mut MqttConnectionManager, zones_status: Arc<Mutex<Vec<ZoneStatus>>>, control_chan_sender: Sender<AmpControlChannelMessage>
) -> Result<()> 
{
    for (source_id, source_config) in sources_config {
        // only handle sources that have a shairport volume_topic configured
        let Some(volume_topic) = &source_config.shairport.volume_topic else {
            continue;
        };

        mqtt.subscribe_utf8(volume_topic, rumqttc::QoS::AtLeastOnce, {
            let volume_topic = volume_topic.clone();
            let zones_config = zones_config.clone();
            let zones_status = zones_status.clone();
            let source_id = source_id.clone();
            let control_chan_sender = control_chan_sender.clone();
            let shairport_config = shairport_config.clone();

            move |_: &Publish, payload: Result<&str, PayloadDecodeError>| {
                match payload {
                    Ok(payload) => {
                        let mut fields = payload.split(',').map(str::parse::<f32>);

                        let airplay_volume = fields.next();

                        match airplay_volume {
                            Some(Ok(airplay_volume)) => {
                                log::info!("source {source_id}: AirPlay volume changed to {airplay_volume}");

                                // TODO: better error handling
                                adjust_listening_zones_volume(&zones_config, zones_status.clone(), &source_id, airplay_volume, &control_chan_sender, &shairport_config).expect("handle shairport");
                            },
                            Some(Err(e)) => log::error!("{volume_topic}: failed to parse AirPlay volume \"{payload}\": {e}"),
                            None => log::error!("{volume_topic}: failed to parse AirPlay volume \"{payload}\""),
                        }
                        
                    },
                    Err(e) => log::error!("{volume_topic}: {e}"),
                }
            }
        })?;
    }

    Ok(())
}