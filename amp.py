import io
import logging
import threading
from dataclasses import dataclass
from enum import Enum
from typing import ClassVar, Union, Optional, Any, Dict, List, Tuple, Iterator, Type

import serial


class SerialBaudRate(Enum):
    B_9600 = 9600
    B_19200 = 19200
    B_38400 = 38400
    B_57600 = 57600
    B_115200 = 115200
    B_230400 = 230400


class SerialBaudOptions(Enum):
    AUTO = 'auto'


class SerialBaudAdjustOptions(Enum):
    OFF = 'off'
    MAX = 'max'


@dataclass(frozen=True)
class Range:
    min: int
    max: int

    def __contains__(self, i: int):
        return self.min <= i <= self.max

    def __str__(self):
        return f'[{self.min},{self.max}]'

    def __iter__(self) -> Iterator[int]:
        return iter(range(self.min, self.max+1))


@dataclass(init=False, frozen=True)
class ZoneId:
    amp: int
    zone: int

    VALID_AMPS: ClassVar[Range] = Range(1, 3)
    VALID_ZONES: ClassVar[Range] = Range(1, 6)

    def __init__(self, zone_id: Union[int, str, 'ZoneId']) -> None:
        zone_id = int(zone_id)
        object.__setattr__(self, 'amp', int(zone_id / 10))
        object.__setattr__(self, 'zone', zone_id % 10)

        if self.amp not in self.VALID_AMPS:
            raise ValueError(f'Amp {self.amp} is not within accepted range {self.VALID_AMPS}.')

        if self.zone not in self.VALID_ZONES:
            raise ValueError(f'Amp {self.amp} zone {self.zone} is not within accepted range {self.VALID_ZONES}.')

    def topic_fragment(self):
        return f'amps/{self.amp}/zones/{self.zone}'

    def __str__(self):
        return f'{self.amp}{self.zone}'

    def __int__(self):
        return (self.amp * 10) + self.zone


class AmpId(ZoneId):
    VALID_ZONES = Range(0, 0)

    @classmethod
    def for_zone(cls, zone: ZoneId):
        return cls(zone.amp * 10)

    def topic_fragment(self):
        return f'amps/{self.amp}'

    def zones(self) -> List[ZoneId]:
        return [ZoneId((self.amp * 10) + z) for z in ZoneId.VALID_ZONES]


@dataclass(frozen=True)
class ZoneAttribute:
    name: str
    type: Union[Type[int], Type[bool]]
    key: Optional[str] = None
    offset: int = 0
    range: Range = Range(0, 99)

    def _check_value(self, value: Any):
        if not isinstance(value, self.type):
            raise ValueError(f'{self.name}: expected value of type {self.type}. Got a {type(value)}.')

        if self.type == int:
            if value not in self.range:
                raise ValueError(f'{self.name}: {value} is not within accepted range {self.range}.')

    def decode_value(self, data: str) -> Any:
        value = self.type(int(data, base=10) + self.offset)
        self._check_value(value)

        return value

    def encode_value(self, value: Union[int, bool]) -> str:
        self._check_value(value)

        value = int(value) - self.offset

        return f'{value:0=2d}'


# order matters. listed in order of zone enquiry response
ZONE_ATTRIBUTES = [
    ZoneAttribute('public_announcement', bool),
    ZoneAttribute('power', bool, key='PR'),
    ZoneAttribute('mute', bool, key='MU'),
    ZoneAttribute('do_not_disturb', bool, key='DT'),
    ZoneAttribute('volume', int, key='VO', range=Range(0, 38)),
    ZoneAttribute('treble', int, key='TR', offset=-7, range=Range(-7, 7)),
    ZoneAttribute('bass', int, key='BS', offset=-7, range=Range(-7, 7)),
    ZoneAttribute('balance', int, key='BL', offset=-10, range=Range(-10, 10)),
    ZoneAttribute('source', int, key='CH', range=Range(1, 6)),
    ZoneAttribute('keypad_connected', bool)
]
# only attributes with keys are settable
SETTABLE_ZONE_ATTRIBUTES = list(filter(lambda a: a.key is not None, ZONE_ATTRIBUTES))

ZoneEnquiryResponseType = Dict[ZoneId, Dict[ZoneAttribute, Any]]


class MwhaAmpConnection:
    CHUNK_SEP = b'\r\n#'
    CHUNK_SEP_LEN = len(CHUNK_SEP)

    lock = threading.Lock()

    def __init__(self, port: str,
                 baud: Union[SerialBaudRate, SerialBaudOptions] = SerialBaudOptions.AUTO,
                 adjust_baud: Union[SerialBaudRate, SerialBaudAdjustOptions] = SerialBaudAdjustOptions.MAX,
                 reset_baud: bool = True,
                 read_timeout: float = 1.0):

        self.serial = serial.serial_for_url(port,
                                            baudrate=9600 if baud == SerialBaudOptions.AUTO else baud.value,
                                            bytesize=serial.EIGHTBITS,
                                            parity=serial.PARITY_NONE, stopbits=serial.STOPBITS_ONE,
                                            xonxoff=False, rtscts=False, dsrdtr=False,
                                            timeout=read_timeout)

        self.logger = logging.getLogger(MwhaAmpConnection.__name__)

        # detect baud
        self.previous_baud = self.detect_baud() if SerialBaudOptions.AUTO else baud

        # adjust baud, if requested
        if adjust_baud == SerialBaudAdjustOptions.MAX:
            self.set_baud(list(SerialBaudRate)[-1])

        elif adjust_baud == SerialBaudAdjustOptions.OFF:
            pass

        else:
            self.set_baud(adjust_baud)

        self.reset_baud = reset_baud

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    def close(self):
        if self.reset_baud:
            self.set_baud(self.previous_baud)

        self.serial.close()

    def exec_command(self, cmd: str, expected_responses: int) -> List[str]:
        with self.lock:
            self.serial.reset_input_buffer()
            self.serial.reset_output_buffer()

            raw_cmd = (cmd + '\r').encode('ascii')
            self.serial.write(raw_cmd)
            self.serial.flush()

            def read_response():
                # serial.read_until will timeout if the expected terminal string isn't received
                # within the timeout period, requiring a timeout that is big enough to receive all
                # the data (unknown).
                # Instead, read is called directly to fetch a single byte.
                # If data is still being received but slowly it's not a timeout.
                # It's only a problem if the single byte read times out.

                buf = b''
                while True:
                    ch = self.serial.read(1)
                    if len(ch) == 0:
                        raise serial.SerialTimeoutException(
                            f'Timeout occurred while reading response for command "{cmd}".')

                    buf += ch
                    if buf[-3:] == b'\r\n#':
                        break

                buf = buf[:-3]

                if buf == b'\r\nCommand Error.':
                    raise IOError(f'Command error occurred while executing command "{cmd}".')

                return buf.decode('ascii')

            echo = read_response()
            if echo != cmd:
                raise IOError(f'Serial echoback was not the expected value. Expected "{cmd}" got "{echo}".')

            return [read_response() for _ in range(expected_responses)]

    def set_baud(self, rate: SerialBaudRate):
        self.logger.info('Setting baud rate to %s', rate.value)

        self.serial.reset_input_buffer()
        self.serial.reset_output_buffer()

        self.serial.write(f'<{rate.value}\r'.encode('ascii'))
        self.serial.flush()

        # As soon as the amp receives the '\r' of the command it switches baud.
        # There's no way to sync switching local baud with the amp, to my knowledge, esp. over IP.
        # Hence, even though baud set commands return "#Done." on success, the response is almost always corrupted.
        # Instead, drain the input buffer and resync after changing baud...
        while len(self.serial.read(1)) > 0:
            pass

        self.serial.baudrate = rate.value

        self._resync()

    def _resync(self):
        """
        Re-synchronise the serial stream. Send up to 4 empty commands (equiv of just '\r') and eventually expect an
        empty, yet valid reply.
        """
        for i in range(4):
            try:
                self.exec_command('', 0)
                return

            except IOError as e:
                if i == 3:
                    raise IOError('Unable to resync serial port.') from e

                continue

    def detect_baud(self) -> SerialBaudRate:
        """
        Detect the current baud rate of the amp.
        This is always 9600 after power on but an abruptly closed serial session could leave the baud set to a higher
        rate.

        Since the amp echos back what is written on the serial line, detection is done by writing a known string and
        trying to read the echo back. If the response is corrupted, try a different baud rate.
        """
        test_data = b'baudrate detect'

        detected_rate = None

        for rate in list(SerialBaudRate):
            self.logger.info('Trying baud rate %s', rate.value)

            self.serial.baudrate = rate.value

            self.serial.write(test_data)
            echo = self.serial.read(len(test_data))
            if echo == test_data:
                detected_rate = rate
                break

            self.logger.debug('%s =! %s', test_data, echo)

        if detected_rate is None:
            raise IOError('Unable to detect current baud rate.')

        self.logger.info('Detected current baud rate as %s', detected_rate.value)

        self._resync()

        return detected_rate

    def zone_enquiry(self, zone_id: Union[ZoneId, AmpId]) -> ZoneEnquiryResponseType:
        """
        Read the status of a zone, or all zones on an amp.

        When passed a ZoneID, will return the status of that zone.
        When passed an AmpID, will return the status of all zones on that amp.
        """
        expected_responses = 6 if isinstance(zone_id, AmpId) else 1

        def decode(data: str) -> Tuple[ZoneId, Dict[ZoneAttribute, Any]]:
            """Parse response: `>xxaabbccddeeffgghhiijj`"""
            s = io.StringIO(data)
            s.read(1)  # prefix '>'

            zid = ZoneId(s.read(2))
            attributes = {a: a.decode_value(s.read(2)) for a in ZONE_ATTRIBUTES}

            return zid, attributes

        zone_attributes = [decode(r) for r in self.exec_command(f'?{zone_id}', expected_responses)]

        return dict(zone_attributes)

    def zone_set(self, zone_id: ZoneId, attribute: ZoneAttribute, value: Any) -> None:
        """
        Set the value of a zone attribute.

        When passed a ZoneID, will set the attribute on that zone.
        When passed an AmpID, will set the attribute on all zones on that amp.
        """
        if attribute.key is None:
            raise ValueError(f"Can't set attribute {zone_id}{attribute.name}.")

        encoded = attribute.encode_value(value)

        self.exec_command(f'<{zone_id}{attribute.key}{encoded}', 0)