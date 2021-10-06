import argparse
import itertools
import json
import logging
import signal
import sys
import threading
from collections import defaultdict, ChainMap
from dataclasses import dataclass
from functools import cache
from typing import Any, Dict

import paho.mqtt.client as paho_client

import mqtt
from amp import ZoneId, AmpId, SETTABLE_ZONE_ATTRIBUTES, ZoneEnquiryResponseType, MwhaAmpConnection
from config import DaemonConfig, config_file

logging.basicConfig(level=logging.DEBUG)


def sigterm_shutdown_handler(sig, stack):
    sys.exit(0)


signal.signal(signal.SIGTERM, sigterm_shutdown_handler)
signal.signal(signal.SIGINT, sigterm_shutdown_handler)


@dataclass(frozen=True)
class Source:
    name: str
    enabled: bool


class MwhaMqttBridge:
    def __init__(self, mqtt_client: paho_client.Client, topic_base: str, amp_connection: MwhaAmpConnection,
                 configured_zones: Dict[ZoneId, str], sources: Dict[int, Source], poll_interval: float):
        self.logger = logging.getLogger(MwhaMqttBridge.__name__)

        self.amp_connection = amp_connection

        self.configured_zones = {ZoneId(zid): name for zid, name in configured_zones.items()}
        self.sources = sources
        assert len(sources) == 6

        self.poll_interval = poll_interval

        self.configured_amps = defaultdict(list)
        for zone_id in configured_zones.keys():
            self.configured_amps[AmpId.for_zone(zone_id)].append(zone_id)

        self.mqtt_client = mqtt_client

        self.topic_base = topic_base
        self.status_topic_base = f'{topic_base}status'
        self.set_topic_base = f'{topic_base}set'

        self.mqtt_client.will_set(f'{topic_base}connected', 0, retain=True)

        self.publish_cv = threading.Condition()

        self.subscribe_setters()

    def _publish_metadata(self):
        # list of configured amps
        amp_ids_json = json.dumps([id.amp for id in self.configured_amps.keys()])
        self.mqtt_client.publish(f'{self.status_topic_base}/amps', amp_ids_json, retain=True)

        # configured source names
        for source_id, source in self.sources.items():
            self.mqtt_client.publish(f'{self.status_topic_base}/sources/{source_id}/name', json.dumps(source.name),
                                     retain=True)
            self.mqtt_client.publish(f'{self.status_topic_base}/sources/{source_id}/enabled',
                                     json.dumps(source.enabled), retain=True)

        # list of configured zones per amp
        for amp_id, zone_ids in self.configured_amps.items():
            zones_json = json.dumps([z.zone for z in zone_ids])
            self.mqtt_client.publish(f'{self.status_topic_base}/{amp_id.topic_fragment()}/zones', zones_json,
                                     retain=True)

        # zone names
        for zone_id, name in self.configured_zones.items():
            self.mqtt_client.publish(f'{self.status_topic_base}/{zone_id.topic_fragment()}/name', json.dumps(name),
                                     retain=True)

    @cache  # cache = only-once
    def _publish_connected(self):
        self.mqtt_client.publish(f'{self.topic_base}connected', 2, retain=True)

    def subscribe_setters(self):
        self.mqtt_client.subscribe(f'{self.set_topic_base}/#')

        # attach subscriptions for each zone/amp command channel
        for zone_id in itertools.chain(self.configured_zones.keys(), self.configured_amps):
            for attr in SETTABLE_ZONE_ATTRIBUTES:
                def make_callback(zone_id=zone_id, attr=attr):
                    def setter_callback(client: paho_client.Client, userdata: Any, message: paho_client.MQTTMessage):
                        self.logger.debug('Received set command for %s.%s (payload=%s).', zone_id, attr.name,
                                          message.payload)
                        try:
                            value = json.loads(message.payload)
                            self.amp_connection.zone_set(zone_id, attr, value)

                            # publish updated zone status
                            with self.publish_cv:
                                # trigger another run of the publish loop
                                self.publish_cv.notify_all()

                        except:
                            self.logger.exception('Exception occurred while processing set message for %s.%s.', zone_id,
                                                  attr.name)

                    return setter_callback

                topic = f'{self.set_topic_base}/{zone_id.topic_fragment()}/{attr.name}'
                self.logger.debug(f'Subscribing to %s.', topic)
                self.mqtt_client.message_callback_add(topic, make_callback())

    def run_publish_loop(self):
        """periodically poll zone status and publish any changes"""

        self._publish_metadata()

        self.logger.info('Watching and publishing zone statuses...')
        cached_attributes = {}

        while True:
            # send enquiry for each configured amp (returning 6 zone statuses per amp)
            statuses: ZoneEnquiryResponseType = dict(
                ChainMap(*[self.amp_connection.zone_enquiry(a) for a in self.configured_amps]))

            # remove statuses for unconfigured zones
            unconfigured_zones = set(statuses.keys()) - set(self.configured_zones.keys())
            for zone_id in unconfigured_zones:
                del statuses[zone_id]

            for zone_id, attributes in statuses.items():
                previous_attributes = cached_attributes.get(zone_id, None)

                for attr, value in attributes.items():
                    previous_value = previous_attributes[attr] if previous_attributes is not None else None

                    if value == previous_value:
                        continue

                    topic = f'{self.status_topic_base}/{zone_id.topic_fragment()}/{attr.name}'

                    self.logger.debug('%s.%s value changed to %s (was %s), publishing update to %s.', zone_id, attr.name,
                                      value, previous_value, topic)

                    json_value = json.dumps(value)
                    self.mqtt_client.publish(topic, json_value, retain=True)

            cached_attributes = statuses

            self._publish_connected()

            with self.publish_cv:
                self.publish_cv.wait(self.poll_interval)





def main():
    parser = argparse.ArgumentParser('mwha2mqttd')
    parser.add_argument('-c', '--config-file', dest='config', type=config_file(DaemonConfig),
                        help='Path to the %(prog)s config file (default: %(default))', default='/etc/mwha2mqttd.toml')

    args = parser.parse_args()

    config: DaemonConfig = args.config

    sources = {i: Source(config.sources.get(i, f'Source {i}'), i not in config.amp.disabled_sources)
               for i in range(1, 7)}

    mqtt_client = paho_client.Client()

    with MwhaAmpConnection(port=config.serial.port, baud=config.serial.baud, adjust_baud=config.serial.adjust_baud,
                           reset_baud=config.serial.reset_baud, read_timeout=config.serial.read_timeout) as amp_connection:

        mwha_bridge = MwhaMqttBridge(mqtt_client, config.mqtt.topic_base(), amp_connection, config.zones, sources, config.amp.poll_interval)

        mqtt.connect(mqtt_client, config.mqtt)

        mqtt_client.loop_start()

        mwha_bridge.run_publish_loop()


if __name__ == '__main__':
    main()
