import argparse
import re
import shlex
import socket
import sys
import threading
from argparse import Namespace
from cmd import Cmd
from collections import defaultdict
from traceback import print_exception, print_exc
from tabulate import tabulate

from typing import Optional, Union, Any, Dict, IO, List

import serial
import serial.rfc2217

from amp import ZoneId, AmpId, ZoneEnquiryResponseType, ZoneAttribute, Range


class CommandError(Exception):
    pass


class DummySerial(serial.SerialBase):
    @property
    def cts(self):
        return False

    @property
    def dsr(self):
        return False

    @property
    def ri(self):
        return False

    @property
    def cd(self):
        return False

    def reset_input_buffer(self):
        pass

    def reset_output_buffer(self):
        pass


# order matters. listed in order of zone enquiry response
PA_ATTRIBUTE = ZoneAttribute('public_announcement', bool)
ZONE_ATTRIBUTES = {
    PA_ATTRIBUTE: False,
    ZoneAttribute('power', bool, key='PR'): False,
    ZoneAttribute('mute', bool, key='MU'): False,
    ZoneAttribute('do_not_disturb', bool, key='DT'): False,
    ZoneAttribute('volume', int, key='VO', range=Range(0, 38)): 0,
    ZoneAttribute('treble', int, key='TR', range=Range(0, 14)): 7,
    ZoneAttribute('bass', int, key='BS', range=Range(0, 14)): 7,
    ZoneAttribute('balance', int, key='BL', range=Range(0, 20)): 10,
    ZoneAttribute('source', int, key='CH', range=Range(1, 6)): 1,
    ZoneAttribute('keypad_connected', bool): False
}

# only attributes with keys are settable
SETTABLE_ZONE_ATTRIBUTES: Dict[str, ZoneAttribute] = {
    a.key: a for a in filter(lambda a: a.key is not None, ZONE_ATTRIBUTES)
}


class AmpEmulator:
    def __init__(self) -> None:
        self.zones: Dict[ZoneId, Dict[ZoneAttribute, Any]] = defaultdict(dict)

        for i in range(1, 7):
            for a, default in ZONE_ATTRIBUTES.items():
                self.zones[ZoneId(10 + i)][a] = default

    def zone_enquiry(self, zone_id: Union[ZoneId, AmpId]) -> ZoneEnquiryResponseType:
        zone_ids = zone_id.zones() if isinstance(zone_id, AmpId) else [zone_id]

        return {zid: zone for zid, zone in self.zones.items() if zid in zone_ids}

    def zone_set(self, zone_id: ZoneId, attribute: ZoneAttribute, value: Any) -> None:
        assert attribute in SETTABLE_ZONE_ATTRIBUTES.values()
        assert isinstance(value, attribute.type)

        zone_ids = zone_id.zones() if isinstance(zone_id, AmpId) else [zone_id]

        for zone_id in zone_ids:
            self.zones[zone_id][attribute] = value


def parse_zone_id(s: str):
    return AmpId(s) if s[1] == '0' else ZoneId(s)


def parse_bool(s: str):
    s = s.lower()
    if 'true'.startswith(s): return True
    elif 'false'.startswith(s): return False

    try:
        i = int(s)
        if i == 1: return True
        if i == 0: return False

    except ValueError:
        pass

    raise ValueError(f'"{s}" is not a valid boolean value.')


class AmpSerialEmulator:
    def __init__(self, sock, amp: AmpEmulator) -> None:
        self.socket = sock
        self.amp = amp

        self.serial = DummySerial()
        self._write_lock = threading.Lock()

        self.port_manager = serial.rfc2217.PortManager(self.serial, self)

    def write(self, data):
        with self._write_lock:
            self.socket.sendall(data)

    def run(self):
        buffer = bytearray()

        while True:
            data = self.socket.recv(1024)
            if not data:
                break

            for byte in self.port_manager.filter(data):
                if byte != b'\n':
                    buffer.extend(byte)

                self.write(byte)  # echo... cho... o...

                def write_response(data: Optional[bytes]):
                    if data is not None:
                        self.write(data)
                    self.write(b'\r\n#')

                if buffer[-1:] == b'\r':
                    cmd = buffer[:-1].decode()

                    try:
                        self.write(b'\n#')

                        if cmd == '':
                            continue

                        if match := re.match(r'<(\d+)', cmd):
                            # set baud
                            new_baud = int(match.group(1))
                            self.serial.baudrate = new_baud
                            self.write(b'#Done.')  # maybe correct?
                            continue

                        if match := re.match(r'\?(\d\d)', cmd):
                            # zone enquiry
                            zone_id = parse_zone_id(match.group(1))

                            for zone_id, attributes in self.amp.zone_enquiry(zone_id).items():
                                values = ''.join([a.encode_value(value) for a, value in attributes.items()])

                                write_response(f'>{zone_id}{values}'.encode('ascii'))

                            continue

                        if match := re.match(r'<(\d\d)(\w\w)(\d\d)', cmd):
                            # zone set
                            zone_id = parse_zone_id(match.group(1))
                            attribute = match.group(3)
                            value = match.group(4)

                            attribute = SETTABLE_ZONE_ATTRIBUTES.get(attribute)
                            value = attribute.decode_value(value)

                            self.amp.zone_set(zone_id, attribute, value)

                            continue

                        raise CommandError(f'Unknown command "{cmd}"')

                    except Exception as e:
                        print_exc()

                        write_response(b'\r\nCommand Error.')

                    finally:
                        buffer.clear()


class CustomParser(argparse.ArgumentParser):
    def exit(self, status=0, message=None):
        if message:
            self._print_message(message)

        raise CommandError()

    def _print_message(self, message, file=None):
        if message:
            sys.stdout.write(message)


class AmpCmd(Cmd):
    prompt = "amp> "

    class Handler:
        def __init__(self, **kwargs):
            self.parser = CustomParser(**kwargs)

        def __call__(self, arg):
            try:
                args = self.parser.parse_args(shlex.split(arg))

                self.handle(args)

            except CommandError:
                pass

        def help(self):
            self.parser.print_help()

        def handle(self, args: Namespace):
            pass

    class SetHandler(Handler):
        def __init__(self, amp: AmpEmulator):
            super().__init__(prog='set', description='set zone attribute')

            self.amp = amp

            def zone(v: str):
                try:
                    return parse_zone_id(v)
                except ValueError as e:
                    raise argparse.ArgumentTypeError(str(e)) from e

            def attribute(v: str):
                for a in ZONE_ATTRIBUTES.keys():
                    if v == a.name or (a.key and v.lower() == a.key.lower()):
                        return a

                raise argparse.ArgumentTypeError(f'Unknown attribute "{v}"')

            attr_help = ', '.join([f'{a.name} ({a.key.lower()})' for a in SETTABLE_ZONE_ATTRIBUTES.values()])

            value_type_help = []
            for a in SETTABLE_ZONE_ATTRIBUTES.values():
                s = f'{a.name} {a.type.__name__}'
                if a.type == int:
                    s += f'{a.range}'
                value_type_help.append(s)
            value_type_help = '; '.join(value_type_help)

            self.parser.add_argument('zone', type=zone, help=f'zone ID. format is AZ, where A is the amp '
                                                             f'number [1,3], Z is the zone number [0,6]. If zone '
                                                             f'number is 0, equivalent to all zones on the specified '
                                                             f'amp.')
            self.parser.add_argument('attribute', type=attribute, help=f'attribute to set. one of: {attr_help}.')
            self.parser.add_argument('value', help=f'attribute value. data type: {value_type_help}.')

        def handle(self, args):
            if args.attribute.type == bool:
                value = parse_bool(args.value)
            else:
                value = args.attribute.type(args.value)

            self.amp.zone_set(args.zone, args.attribute, value)

    class StatusHandler(Handler):
        def __init__(self, amp: AmpEmulator):
            super().__init__(prog='status', description='zone status enquiry')

            self.amp = amp

        def handle(self, args: Namespace):
            data = []
            for zid, attrs in self.amp.zones.items():
                row = {
                    'id': zid,
                    **{a.name: value for a, value in attrs.items()}
                }
                data.append(row)

            print(tabulate(data, headers='keys'))
            # for zone_id, attributes in self.amp.zones.items():
            #     values = ' '.join([f'{a.name}={value}' for a, value in attributes.items()])
            #     print(f'{zone_id} {values}')

    class PublicAnnouncementHandler(Handler):
        def __init__(self, amp: AmpEmulator):
            super().__init__(prog='pa', description='set amp public announcement state')

            self.amp = amp

            self.parser.add_argument('state', type=parse_bool, help='public announcement state. true/false')

        def handle(self, args: Namespace):
            for attributes in self.amp.zones.values():
                attributes[PA_ATTRIBUTE] = args.state

    def __init__(self, amp: AmpEmulator) -> None:
        super().__init__()

        self.do_set = AmpCmd.SetHandler(amp)
        self.help_set = self.do_set.help

        self.do_status = AmpCmd.StatusHandler(amp)
        self.help_status = self.do_status.help

        self.do_pa = AmpCmd.PublicAnnouncementHandler(amp)
        self.help_pa = self.do_pa.help

    def get_names(self) -> List[str]:
        names = dir(self)
        names.remove('do_EOF')
        return names

    def do_EOF(self, arg):
        return True


class ServerThread(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)

    def run(self) -> None:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(('', 5678))
        srv.listen(1)

        print(f'Listening for connections on {srv.getsockname()}')

        while True:
            client_socket, addr = srv.accept()
            print(f'Accepted connection from {addr}')

            with client_socket:
                client_socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

                AmpSerialEmulator(client_socket, amp).run()

            print(f'Connection from {addr} closed.')


server_thread = ServerThread()
server_thread.start()


amp = AmpEmulator()
cmd = AmpCmd(amp)

cmd.cmdloop()

exit(0)



