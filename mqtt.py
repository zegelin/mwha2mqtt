import paho.mqtt.client as paho_client

from config import MqttConfig


def connect(mqtt_client: paho_client.Client, config: MqttConfig):
    if config.url.scheme == 'mqtts':
        default_mqtt_port = 8883
        mqtt_client.tls_set(ca_certs=config.ca_certs,
                            certfile=config.client_cert, keyfile=config.client_key,
                            ciphers=config.tls_ciphers)

        mqtt_client.tls_insecure_set(not config.validate_hostname)

    else:
        default_mqtt_port = 1883

    mqtt_client.username_pw_set(config.url.user, config.url.password)

    if config.srv_lookup:
        mqtt_client.connect_srv(config.url.host)
    else:
        mqtt_client.connect(config.url.host, config.url.port or default_mqtt_port)