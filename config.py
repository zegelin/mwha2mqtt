import argparse
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Dict, Any, List, Set, Union, Optional, Type

import toml as toml
from pydantic import BaseModel, AnyUrl, Extra, conint, confloat, validator

from amp import ZoneId, SerialBaudRate, SerialBaudOptions, SerialBaudAdjustOptions


def config_file(config_type: Type[BaseModel]):
    def load_config(s: str):
        try:
            with open(s) as f:
                d = toml.load(f)
                return config_type.parse_obj(d)

        except Exception as e:
            raise argparse.ArgumentTypeError(str(e))

    return load_config


class ConfigZoneId(ZoneId):
    @classmethod
    def __get_validators__(cls):
        yield cls


SourceIdType = conint(ge=1, le=6)


class MqttUrl(AnyUrl):
    allowed_schemes = {'mqtt', 'mqtts'}


class ConfigModel(BaseModel):
    class Config:
        extra = Extra.forbid


class SerialConfig(ConfigModel):
    port: str
    read_timeout: confloat(gt=0) = 1
    baud: Union[SerialBaudRate, SerialBaudOptions] = SerialBaudOptions.AUTO
    adjust_baud: Union[SerialBaudRate, SerialBaudAdjustOptions] = SerialBaudAdjustOptions.MAX
    reset_baud: bool = True


class MqttConfig(ConfigModel):
    url: MqttUrl = "mqtt://localhost/mwah/"
    srv_lookup: bool = False

    client_id: str = "mwha2mqtt"

    ca_certs: Optional[Path] = None
    client_cert: Optional[Path] = None
    client_key: Optional[Path] = None
    tls_ciphers: Optional[str] = None
    validate_hostname: bool = True

    def topic_base(self):
        return 'mwah/' if self.url is None else self.url.path[1:]


class AmpConfig(ConfigModel):
    poll_interval: confloat(gt=0) = 0.5

    manufacturer: str = "Monoprice"
    model: str = "MPR-6ZHMAUT"
    serial: str = ""

    disabled_sources: Set[SourceIdType] = {}

    # prevent_startle: bool = False
    # volume_max: conint(ge=0, le=38) = 38


class DaemonConfig(ConfigModel):
    serial: SerialConfig
    mqtt: MqttConfig
    amp: AmpConfig
    zones: Dict[ConfigZoneId, str]
    sources: Dict[SourceIdType, str]

    @validator('zones')
    def check_zones_not_empty(cls, zones):
        assert len(zones) > 0, 'At least one zone must be configured.'
        return zones

    @validator('zones', each_item=True)
    def check_zone_names_not_empty(cls, value):
        assert value != "", 'Zone names cannot be empty.'
        return value

    @validator('sources', each_item=True)
    def check_source_names_not_empty(cls, value):
        assert value != "", 'Source names cannot be empty.'
        return value


class CliMqttConfig(MqttConfig):
    client_id: Optional[str] = None


class CliConfig(ConfigModel):
    mqtt: CliMqttConfig

    serial: Any
    amp: Any
    zones: Any
    sources: Any

