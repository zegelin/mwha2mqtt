# mwha2mqtt

Monoprice/McLELLAND whole-home audio amplifier serial to MQTT bridge controller.

**This project is a work in progress.**

This project communicates with various models of multi-zone whole-home audio amplifiers via RS232 enabling status enquiry and remote control of these amplifiers via MQTT.

Zone status is polled periodically and when a zone attribute change, the value is reported on zone-specific MQTT topics.
Values can be written to zone-specific MQTT topics to adjust zone attributes.
See [Topics](#topics) below for details.

## Features

- Publishes zone status/attributes to zone-specific MQTT topics.
- Subscribes to zone-specific MQTT topics for modification of zone attributes.
- Automatic serial baud-rate detection and negotiation.
- Uses PySerial URLs for configuration, permitting serial-over-IP (RFC2217, etc.).

## Compatible Amplifiers

Manufacturer | Model | Compatibility | Links
--- | --- | --- | ---
Monoprice | MPR-6ZHMAUT (10761) | Compatible | [Product Page](https://www.monoprice.com/product?p_id=10761) / [Manual](https://downloads.monoprice.com/files/manuals/10761_Manual_131209.pdf)
McLELLAND  | MAP-1200HD | Unconfirmed | [Product Page](https://www.mclellandmusic.com/productdetail/7)
McLELLAND  | MAP-1200WE/MAP1200EW | Unconfirmed | [Product Page](https://www.mclellandmusic.com/productdetail/161)
DaytonAudio | DAX66 | Unconfirmed | [Product Page](https://www.daytonaudio.com/product/1252/dax66-6-source-6-zone-distributed-audio-system) / [RS232 Commands](https://www.daytonaudio.com/images/resources/300-585-dayton-audio-dax66-commands.pdf)
OSDAudio | NERO MAX12 | Unconfirmed | [Product Page](https://www.outdoorspeakerdepot.com/osd-audio-nero-max12-wifi-wireless-multi-channelmulti-zone-amplifier-wkey-pad-optional.html)
Texonic | A-M600 | Unconfirmed | [Product Page](https://www.texonic.ca/store/p27/6_Multi-zone_WiFi_Streaming_Audio_System_%28A-M600%29.html)
Rave Technology | RMC-66A | Unconfirmed | [Product Page](http://ravetechnology.com/product/rmc-66a-6-source-6-zone-audio-matrix-with-integrated-amplifier/)
Soundavo | WS66i | Unconfirmed | 

Amps listed as "unconfirmed" are those that I and other contributors have yet to validate as fully compatible with mwha2mqtt.
These amps list RS232 control codes in their instruction manuals that match the `MAP-1200HD` and/or have a similar appearance (same chassis, ports and zone keypads).
Pull requests to update this table would be appreciated.

## Topics

The topic names below have the prefix `mwha/`.
This prefix can be changed in the configuration file.

In the topic names below `<a>` represents a placeholder for value `a`.
The `<` and `>` characters are not present in the topic name.
For example `source-<i>` where `i` = 1 results in a topic name `source-1`.

Topic values are JSON encoded.
The data type clients should expect to send and receive is noted in the *Data Type* column.
`array[type]` represents a JSON array of values, where each value is of type `type`.

### Subscribe-only Topics

The following topics are for clients to receive updates & metadata about the configured amps &
zones, and the zone attribute states.

Publishing messages to these topics is handled by `mwha2mqttd`.
Any messages that clients publish will be ignored by `mwha2mqttd`.

Messages published to these topics will have their retain flag set. Messages will be published once when `mwha2mqttd` starts, then whenever a zone state changes.

Topic | JSON Type | Description
--- | --- | ---
`mwha/connected` | Number | `mwha2mqtt` connected status.<br/><br/>`0` = not connected/running.<br/>`2` = connected to MQTT & serial.
`mwha/status/source-<i>/...` | n/a | Source metadata. `i` is the source ID. `i` is `1` through `6` (inclusive).
`mwha/status/source-<i>/name` | String | Source `i`'s name.
`mwha/status/source-<i>/enabled` | Boolean | Source `i`'s enabled state. Sources can be marked as disabled in the config. This is used as a hint to clients that the source isn't available. How clients reflect this is up to the client. All sources can always be selected from zone keypads.
`mwha/status/amps` | Array[Number] | List of configured amp IDs. Values are `1` through `3` (inclusive). Only amps which have configured zones will be listed.
`mwha/status/amp-<a>/zones` | Array[Number] | List of configured zone IDs for amp `a`.
`mwha/status/amp-<a>/zone-<z>/name` | String | Zone `z` of amp `a`'s name.
`mwha/status/amp-<a>/zone-<z>/power` | Boolean | Zone `z` of amp `a`'s power status.<br/><br/>`true` = zone powered on.<br/>`false` = zone powered off.
`mwha/status/amp-<a>/zone-<z>/public_announcement` | Boolean | Zone `z` of amp `a`'s public announcement status.<br/><br/>`true` = zone is in PA state (the PA trigger is pulled high).<br/>`false` = zone is normal.
`mwha/status/amp-<a>/zone-<z>/mute` | Boolean | Zone `z` of amp `a`'s mute status.<br/><br/>`true` = zone is muted.<br/>`false` = zone is un-muted.
`mwha/status/amp-<a>/zone-<z>/do_not_disturb` | Boolean | Zone `z` of amp `a`'s do not disturb status.
`mwha/status/amp-<a>/zone-<z>/volume` | Number | Zone `z` of amp `a`'s volume.<br/><br/>Value ranges from `0` to `38`, inclusive, integers only.
`mwha/status/amp-<a>/zone-<z>/treble` | Number | Zone `z` of amp `a`'s treble adjustment.<br/><br/>Value ranges from `-7` (treble reduction) to `7` (treble boost), inclusive, integers only.
`mwha/status/amp-<a>/zone-<z>/bass` | Number | Zone `z` of amp `a`'s bass adjustment.<br/><br/>Value ranges from `-7` (bass reduction) to `7` (bass boost), inclusive, integers only.
`mwha/status/amp-<a>/zone-<z>/balance` | Number | Zone `z` of amp `a`'s balance adjustment.<br/><br/>Value ranges from `-10` (left) to `10` (right), inclusive, integers only.
`mwha/status/amp-<a>/zone-<z>/source` | Number | Zone `z` of amp `a`'s active source.<br/><br/>Value ranges from `1` to `6`, inclusive, integers only.<br/><br/>This value can be mapped to the source metadata topics (`source-<i>`) for source info.
`mwha/status/amp-<a>/zone-<z>/keypad_connected` | Boolean | Zone `z` of amp `a`'s keypad connected status.<br/><br/>`true` = zone keypad connected.<br/>`false` = zone keypad disconnected.

### Publish-only Topics

The following topics are for clients to alter the states of configured zones.

Publishing a message to an unconfigured zone is a no-op. Invalid values will be logged but otherwise are a no-op.

Topic | JSON Type | Description
--- | --- | ---
`mwha/set/amp-<a>/zone-<z>/power` | Boolean | Zone `z` of amp `a`'s power status.<br/><br/>`true` = zone powered on.<br/>`false` = zone powered off.
`mwha/set/amp-<a>/zone-<z>/mute` | Boolean | Zone `z` of amp `a`'s mute status.<br/><br/>`true` = zone is muted.<br/>`false` = zone is un-muted.
`mwha/set/amp-<a>/zone-<z>/do_not_disturb` | Boolean | Zone `z` of amp `a`'s do not disturb status.
`mwha/set/amp-<a>/zone-<z>/volume` | Number | Zone `z` of amp `a`'s volume.<br/><br/>Value ranges from `0` to `38`, inclusive, integers only.
`mwha/set/amp-<a>/zone-<z>/treble` | Number | Zone `z` of amp `a`'s treble adjustment.<br/><br/>Value ranges from `-7` (treble reduction) to `7` (treble boost), inclusive, integers only.
`mwha/set/amp-<a>/zone-<z>/bass` | Number | Zone `z` of amp `a`'s bass adjustment.<br/><br/>Value ranges from `-7` (bass reduction) to `7` (bass boost), inclusive, integers only.
`mwha/set/amp-<a>/zone-<z>/balance` | Number | Zone `z` of amp `a`'s balance adjustment.<br/><br/>Value ranges from `-10` (left) to `10` (right), inclusive, integers only.
`mwha/set/amp-<a>/zone-<z>/source` | Number | Zone `z` of amp `a`'s active source.<br/><br/>Value ranges from `0` to `6`, inclusive, integers only.<br/><br/>This value can be mapped to the source metadata topics (`source-<i>`) for source info.
