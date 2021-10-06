import argparse
from typing import Any

import paho.mqtt.client as paho_client

import mqtt
from config import config_file, CliConfig


def main():
    parser = argparse.ArgumentParser('mwhactl')
    parser.add_argument('-c', '--config-file', dest='config', type=config_file(CliConfig),
                        help='Path to the %(prog)s config file (default: %(default))', default='/etc/mwha2mqtt.toml')

    # subparsers = parser.add_subparsers(help='sub-command help')
    #
    # parser_foo = subparsers.add_parser('status')
    # parser_foo.add_argument('-x', type=int, default=1)
    # parser_foo.add_argument('y', type=float)
    # parser_foo.set_defaults(func=foo)

    args = parser.parse_args()
    config: CliConfig = args.config

    topic_base = config.mqtt.topic_base()

    mqtt_client = paho_client.Client()

    mqtt_client.subscribe(f'{topic_base}connected')
    mqtt_client.subscribe(f'{topic_base}status/#')

    def callback(client: paho_client.Client, userdata: Any, message: paho_client.MQTTMessage):
        print(f'{message.topic}: {message.payload}')

    mqtt_client.on_message = callback

    mqtt_client.message_callback_add(f'{topic_base}connected', callback)

    mqtt.connect(mqtt_client, config.mqtt)

    # mqtt_client.loop()
    mqtt_client.loop_forever()


if __name__ == '__main__':
    main()
