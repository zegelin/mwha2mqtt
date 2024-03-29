# mwha2mqtt
Monoprice/McLELLAND whole-home audio amplifier serial to MQTT bridge controller.

The main component of this project is `mwha2mqttd`, a background daemon that communicates with various models of multi-zone whole-home audio amplifiers via RS232,
enabling status enquiry and remote control of these amplifiers via MQTT.

`mwha2mqttd` periodically polls the connected amp(s) for zone status.
When zone attributes (e.g., volume) change the new values are published to MQTT topics.
Clients can adjust zone attributes by publishing values to MQTT topics.
`mwha2mqttd` subscribes to these topics and will communicate with the amp to adjust the zone(s).
See [Topics](#topics) below for details.

The project has been rewritten in Rust!
The old Python version can be found on the `python` branch.

## Features
- Zone attribute published and adjustable over MQTT.
- Communication via physical TTY or COM port (such as a USB<->RS232 adapter) or raw serial-over-TCP (RFC2217 not supported).
- [Shairport Sync](https://github.com/mikebrady/shairport-sync) (AirPlay) volume control integration.

## Features yet to be implemented
- A basic cli client tool (`mwha-cli`).
- A GUI mixer client.
- Automatic HomeKit and HomeAssistant integration.
- Automatic serial baud-rate detection and negotiation (for physical ports) (code is there, but doesn't work).
- MQTT SRV support.
- Example systemd service files.
- Packaging/install scripts for various distros.

## Compatible Amplifiers
| Manufacturer    | Model                | Compatibility | Notes                                                                                                                                                                                                            |
|-----------------|----------------------|---------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Monoprice       | MPR-6ZHMAUT (10761)  | Compatible    | Equivalent to MAP-1200HD.<br><br>[Product Page](https://www.monoprice.com/product?p_id=10761), [Manual](https://downloads.monoprice.com/files/manuals/10761_Manual_131209.pdf)                                                                   |
| McLELLAND       | MAP-1200HD           | Unconfirmed   | OEM.<br><br>[Product Page](https://www.mclellandmusic.com/productdetail/7)                                                                                                                                                   |
| McLELLAND       | MAP-1200WE/MAP1200EW | Unconfirmed   | OEM.<br><br>[Product Page](https://www.mclellandmusic.com/productdetail/161)                                                                                                                                                 |
| DaytonAudio     | DAX66                | Unconfirmed   | [Product Page](https://www.daytonaudio.com/product/1252/dax66-6-source-6-zone-distributed-audio-system),  [RS232 Commands](https://www.daytonaudio.com/images/resources/300-585-dayton-audio-dax66-commands.pdf) |
| OSDAudio        | NERO MAX12           | Unconfirmed   | [Product Page](https://www.outdoorspeakerdepot.com/osd-audio-nero-max12-wifi-wireless-multi-channelmulti-zone-amplifier-wkey-pad-optional.html)                                                                  |
| Texonic         | A-M600               | Unconfirmed   | [Product Page](https://www.texonic.ca/store/p27/6_Multi-zone_WiFi_Streaming_Audio_System_%28A-M600%29.html)                                                                                                      |
| Rave Technology | RMC-66A              | Unconfirmed   | [Product Page](http://ravetechnology.com/product/rmc-66a-6-source-6-zone-audio-matrix-with-integrated-amplifier/), [Manual](https://ravetechnology.com/downloads/RMC-66A-Manual-3.24.2020.pdf) |
| Soundavo        | WS66i                | Unconfirmed   | [Product Page](https://www.soundavo.com/products/ws66i-amp-only-audio-distribution-network-controller-matrix-with-streamer-app-control), [Manual](https://cdn.shopify.com/s/files/1/0119/8034/1307/files/Soundavo_WS66I_Manual_Production_website.pdf) |

Amps listed as "unconfirmed" are those that haven't yet been validated as fully compatible with `mwha2mqtt`.
These amps list RS232 control codes in their instruction manuals that match the `MAP-1200HD` and/or have a similar appearance (same chassis, ports and zone keypads).
Pull requests to update this table would be appreciated once `mwha2mqtt` is confirmed working with these amps.

## Configuration
`mwha2mqttd` has various settings that are set by a TOML configuration file.

The default config file shows all the available settings and documentation is provided as comments. 

The location and name of this config file varies depending on how _mwha2mqtt_ is installed.

### Linux
On most Linux-based systems, packaged versions of `mwha2mqttd` reads its configuration from `/etc/mwha2mqttd.conf`.



## Topics
The topic names below are shown with the prefix `mwha/`.
This prefix can be changed in the `mwha2mqttd` configuration file (see the `mqtt.url` option)

In the topic names below `<a>` represents a placeholder for value `a` (the `<` and `>` characters are not present in the topic name).
For example `source/<i>` where `i` = 1 results in a topic name `source/1`.

Messages sent on topics are JSON encoded.
The JSON data type of the message clients should expect to send/receive on a particular topic is noted in the *Data Type* column.
`Integer` is a JSON `Number` that only has an integer component.

### Subscribe-only Topics
The following topics are for clients to receive metadata and updates about the configured amps, zones and sources.

Publishing messages to these topics is handled by `mwha2mqttd`.
Any messages published by other clients will be ignored by `mwha2mqttd`

Messages published to these topics by `mwha2mqttd` will have their retain flag set.
A message to each topic will be published once when `mwha2mqttd` starts, then whenever a zone attribute changes (after a successful 
status query to the amp).

| Topic | Data Type | Description |
|-------|-----------|-------------|
| `mwha/connected` | Integer | `mwha2mqttd` connected status.<br/><br/>`0` = not connected/not running.<br/>`2` = connected to MQTT & serial.                                                                                                                                          |
| `mwha/status/amp/model` | String | Amplifier model, as defined in the config. |
| `mwha/status/amp/manufacturer` | String | Amplifier manufacturer, as defined in the config. |
| `mwha/status/amp/serial` | String | Amplifier serial number, as defined in the config. |
| `mwha/status/source/<source-id>/<attribute>` | _Various_ | Source status and metadata.<br><br>See [Source Attribute Topics](#source-attribute-toptics) below for details. |
| `mwha/status/zones` | String array | An array of configured zone IDs.<br><br>Clients can use this to determine which zone topics are valid. |
| `mwha/status/zone/<zone-id>/<attribute>`| _Various_ | Zone status and metadata.<br><br>See [Zone Attribute Topics](#zone-attribute-topics)below for details. 

### Publish-only Topics
The following topics are for clients to alter the attributes of configured zones.

Publishing a message to topic for an unconfigured zone is a no-op. Invalid values will be logged but are otherwise a no-op.

| Topic | Data Type | Description |
|-------|-----------|-------------|
| `mwha/set/zone/<zone-id>/<attribute>`| _Various_ | Zone adjustment.<br><br>See [Zone Attribute Topics](#zone-attribute-topics) below for details. 


### Source Attribute Topics
Source metadata and attribute updates are published by `mwha2mqttd` to the `mwha/status/source/<source-id>/<attribute>` topics.

`source-id` in the topic is the source ID. Valid source IDs are `1` through `6` (inclusive).

`attribute` in the topic is a source attribute name from the table below. 

| Attribute | Data Type | Description |
|-----------|-----------|-------------|
| `name` | String | Source name, as defined in the config.
| `enabled` | Boolean | Source enabled state.<br><br>Sources can be marked as disabled in the config. This is used as a hint to clients that the source isn't available. How clients reflect this is up to the client. All sources can always be selected from zone keypads.


### Zone Attribute Topics
Zone metadata and attribute updates are published by `mwha2mqttd` to the `mwha/status/zone/<zone-id>/<attribute>` topics.

Clients can adjust certain zone attributes by publishing messages to the `mwha/set/zone/<zone-id>/<attribute>` topics.

For the meaning of `<zone-id>` and `<attribute>`, see [Zone IDs](#zone-ids) and [Zone Attributes](#zone-attributes).

**Note**: Zones must be first configured in the config file before `mwha2mqttd` will publish status and handle adjustments for them.
Sending adjustments to an unconfigured zone is a no-op.
The `mwha/status/zones` topic will contain a list of configured zone IDs.
It is recommended that clients subscribe to this topic to discover the list of configured/active zones.

#### Zone IDs

`<zone-id>` in the topic is a 2-digit zone identifier in the format _AZ_.<br>
The first digit _A_ is the amplifier number, and valid values are `1` through `3` (inclusive), or `0` (see below).<br>
The second digit _Z_ is the zone number on amplifier _A_, and valid values are `1` through `6` (inclusive), or `0` (see below).

See the table below for a full list of valid zone IDs.

Zone IDs refer to physical zones unless their ID contains a `0` which instead refers to a virtual zone.

ID `00` is a virtual zone representing all zones (aka "the system").
No attribute status execpt `name` will be reported for this zone. However, adjustments sent to this zone will adjust all zones on every amp simultaniously.

IDs `10`, `20`, and `30` are virtual zones representing all zones on amp `1`, `2` and `3` (respectively).
No attribute status execpt `name` will be reported for these zones. However, adjustments sent to these zones will adjust all zones on amp _A_ simultaniously.

| ID | Zone Type | Attr. Status Updates | Description |
|----|-----------|----------------------|-------------|
| `11` .. `16` | Physical | All attributes | Zones on amp `1` (_Master_ position on the selector switch on the rear of the amp). |
| `21` .. `26` | Physical | All attributes | Zones on amp `2` (_Slave 1_ position on the selector switch on the rear of the amp). |
| `31` .. `36` | Physical | All attribute | Zones on amp `3` (_Slave 2_ position on the selector switch on the rear of the amp). |
| `00` | Virtual | `name` only | "System" zone. Adjusts all zones on all amps. |
| `10` | Virtual | `name` only | Amp `1` zone, adjusts all zones on amp 1. |
| `20` | Virtual | `name` only | Amp `2` zone, adjusts all zones on amp 2. |
| `30` | Virtual | `name` only | Amp `3` zone, adjusts all zones on amp 3. |


#### Zone Attributes

`<attribute>` in the topic is a zone attribute name from the table below.

In the table below, attributes marked as _RO_ cannot be adjusted.
Updates for these attributes will be published on the `status/` topic. However, trying to adjust them via the `set/` topic is a no-op.

Attributed marked _R/W_ can be adjusted via the `set/` topic.

| Attribute | Data Type | | Details |
|-----------|-----------|-|---------|
| `name` | String | RO | Zone name, as defined in the config. |
| `public-announcement` | Boolean | RO | Zone public announcement status.<br><br>When a zone is in PA mode it will play audio from source 1.<br/><br/>`true` = zone is in PA mode (the PA 12V trigger is pulled high).<br/>`false` = zone is normal.
| `power` | Boolean | R/W | Zone power status.<br/><br/>`true` = zone powered on.<br/>`false` = zone powered off. |
| `mute` | Boolean | R/W | Zone mute status.<br/><br/>`true` = zone is muted.<br/>`false` = zone is un-muted. | 
| `do-not-disturb` | Boolean | R/W | Zone do not disturb status.<br/><br/>`true` = DND enabled.<br/>`false` = zone is un-muted.
| `volume` | Integer | R/W | Zone volume.<br/><br/>Value ranges from `0` to `38`, inclusive.
| `treble` | Integer | R/W | Zone treble adjustment.<br/><br/>Value ranges from `0` to `14`, inclusive.<br><br>`0` = maximum treble reduction.<br>`7` = flat (no adjustment).<br>`14` = maximum treble boost. |
| `bass` | Integer | R/W | Zone bass adjustment.<br/><br/>Value ranges from `0` to `14`, inclusive.<br><br>`0` = maximum bass reduction.<br>`7` = flat (no adjustment).<br>`14` = maximum bass boost. |
| `balance` | Integer | R/W | Zone balance adjustment.<br/><br/>Value ranges from `0` to `20`, inclusive<br><br>`0` = 100% left.<br>`7` = centre (no adjustment).<br>`14` = 100% right. |
| `source` | Integer | R/W | Zone active source.<br/><br/>Value ranges from `1` to `6`, inclusive.<br/><br/>This value can be mapped to the source metadata topics (`source/<i>`) for source info. |
| `keypad-connected` | Boolean | RO | Zone keypad connected status.<br/><br/>`true` = zone keypad connected.<br/>`false` = zone keypad disconnected. |


## Shairport Sync Integration

`mwha2mqttd` can integrate with Shairport Sync via MQTT, which allows for volume control of zone(s) from AirPlay clients (such as iOS or macOS). When one or more zones are listening to a designated Shairport Sync source, volume control commands from AirPlay client(s) of that source change the volume of the zones listening to that source.

For example: a PC running Shairport Sync has its S/PDIF optical out connected to the _Source 6_ input on the back of the master amp, and this source is configured in the `mwha2mqttd` configuration as a Shairport Sync source (`shairport.volume_topic` is specified). _Zones 1_ and _4_ are listening to _Source 6_ and an iOS client plays music over AirPlay to the Shairport Sync speaker, hence _Zones 1_ and _4_ hear the music playing from the iOS device. The user adjusts the speaker volume on the iOS device and Shairport Sync publishes volume change metadata MQTT messages. `mwha2mqttd` receives these messages and adjusts the volume of _Zones 1_ and _4_ to match the iOS clients' specified volume.

To use this feature, Shairport Sync must be compiled with MQTT support, and MQTT metadata must be enabled in the `shairport-sync.conf` configuration file. When enabled, Shairport Sync will publish metadata about the current AirPlay stream, including volume level, onto MQTT topics.

### Basic Configuration Steps

1. Connect the hardware, so that the audio output of a device running Shairport Sync is connected to a source on the amp.

2. Compile Shairport Sync with MQTT support and configure it to publish metadata over MQTT.

    See the [Shairport Sync MQTT documenation](https://github.com/mikebrady/shairport-sync/blob/master/MQTT.md) for more info.

    Either the `publish_raw` or `publish_parsed` (or both) option may be used in the Shairport Sync config, as the same volume metadata is published for either option, but note that the volume topic name differs. See below.

    Hint: It's recommened to set the Shairport Sync `ignore_volume_control` option to `"yes"` to disable the built-in software mixing and hardware audio device volume control. Explicitly set the hardware device volume to 100% (0 dB) and let the amp do the volume adjustment.

    Note: if you're running multiple instances of Shairport Sync, ensure that the `topic` configuration option is unique for each instance. 


    Example partial `shairport-sync.conf`:
    ```
    general = {
        ⋮
        name = "Whole-home Audio (Source 6)";
        ignore_volume_control = "yes";
        ⋮
    }

    mqtt = {
        enabled = "yes";
        topic = "shairport-ch6"
        publish_parsed = "yes";
        ⋮
    }
    ```


3. Configure source(s) in `mwha2mqttd.conf` to have the appropriate `shairport.volume_topic` to match the Shairport and hardware configuration.

    When Shairport Sync is configured with:
    - `publish_parsed = "yes"`, then the `volume_topic` should be of the form `<shairport-topic>/volume`.
    - `publish_raw = "yes"`, then the `volume_topic` should be of the form `<shairport-topic>/pvol`.

    (where `<shairport-topic>` is the value of `mqtt.topic` in the Shairport config)

    Example partial `mwha2mqttd.conf`:
    ```
    [amp.sources]
    ⋮
    6 = { name = "AirPlay", shairport.volume_topic = "shairport-ch6/volume" }
    ```
