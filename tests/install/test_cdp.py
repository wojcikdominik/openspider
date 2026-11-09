"""Tests für den minimalen CDP-Client (M3-Q02) gegen einen lokalen Schein-DevTools-Server."""
import base64
import hashlib
import http.server
import json
import socket
import struct
import threading
import unittest

import cdp

GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'


def server_frame(payload, opcode=0x1, fin=True):
    header = bytearray([(0x80 if fin else 0) | opcode])
    if len(payload) < 126:
        header.append(len(payload))
    elif len(payload) < 1 << 16:
        header += bytes([126]) + struct.pack('>H', len(payload))
    else:
        header += bytes([127]) + struct.pack('>Q', len(payload))
    return bytes(header) + payload


class FakeDevTools:
    """/json/list und ein WebSocket-Ziel, das Runtime.evaluate beantwortet."""

    def __init__(self, answer):
        self.answer = answer
        self.received = []
        self.ws = socket.socket()
        self.ws.bind(('127.0.0.1', 0))
        self.ws.listen()
        ws_port = self.ws.getsockname()[1]
        pages = [{'type': 'service_worker', 'webSocketDebuggerUrl': 'ws://127.0.0.1:1/x', 'url': 'x'},
                 {'type': 'page', 'url': 'about:blank', 'webSocketDebuggerUrl': f'ws://127.0.0.1:{ws_port}/other'},
                 {'type': 'page', 'url': 'http://tauri.localhost/', 'webSocketDebuggerUrl': f'ws://127.0.0.1:{ws_port}/page/1'}]

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                body = json.dumps(pages).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        self.http = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.port = self.http.server_address[1]
        threading.Thread(target=self.http.serve_forever, daemon=True).start()
        threading.Thread(target=self.serve_ws, daemon=True).start()

    def serve_ws(self):
        conn, _ = self.ws.accept()
        with conn:
            self._session(conn)

    def _session(self, conn):
        request = b''
        while b'\r\n\r\n' not in request:
            request += conn.recv(1)
        self.path = request.split(b' ')[1].decode()
        key = [line.split(b': ')[1] for line in request.split(b'\r\n') if line.lower().startswith(b'sec-websocket-key')][0]
        accept = base64.b64encode(hashlib.sha1(key + GUID.encode()).digest())
        conn.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
                     b'Sec-WebSocket-Accept: ' + accept + b'\r\n\r\n')
        while True:
            try:
                opcode, payload, _ = cdp.read_frame(conn)
            except cdp.CdpError:
                return
            if opcode == 0x8:
                return
            if opcode == 0xA:
                self.received.append('pong')
                continue
            message = json.loads(payload)
            self.received.append(message)
            # Ereignis, Ping und fragmentierte Antwort vor dem Ergebnis
            conn.sendall(server_frame(json.dumps({'method': 'Runtime.consoleAPICalled'}).encode()))
            conn.sendall(server_frame(b'ping', opcode=0x9))
            reply = json.dumps({'id': message['id'], 'result': self.answer}).encode()
            conn.sendall(server_frame(reply[:5], fin=False) + server_frame(reply[5:], opcode=0x0))

    def close(self):
        self.http.shutdown()
        self.http.server_close()
        self.ws.close()


class Frames(unittest.TestCase):
    def test_roundtrip_for_all_length_encodings(self):
        for size in (0, 10, 125, 126, 300, 65535, 70000):
            a, b = socket.socketpair()
            with a, b:
                payload = bytes(range(256)) * (size // 256) + bytes(size % 256)
                frame = cdp.encode_frame(payload, mask=b'\x01\x02\x03\x04')
                threading.Thread(target=a.sendall, args=(frame,), daemon=True).start()
                self.assertEqual(cdp.read_frame(b), (0x1, payload, True))


class Client(unittest.TestCase):
    def test_evaluate_on_the_tauri_page(self):
        fake = FakeDevTools({'result': {'type': 'object', 'value': {'available': {'version': '1.0.1'}}}})
        try:
            conn = cdp.connect(fake.port, wait=5)
            value = conn.evaluate(cdp.invoke_expression('update.check'))
            conn.close()
        finally:
            fake.close()
        self.assertEqual(value, {'available': {'version': '1.0.1'}})
        self.assertEqual(fake.path, '/page/1')
        request = fake.received[0]
        self.assertEqual(request['method'], 'Runtime.evaluate')
        self.assertTrue(request['params']['awaitPromise'] and request['params']['returnByValue'])
        self.assertEqual(request['params']['expression'],
                         'window.__TAURI_INTERNALS__.invoke("plugin:update|check", {})')
        self.assertIn('pong', fake.received)

    def test_javascript_exception_is_an_error(self):
        fake = FakeDevTools({'result': {'type': 'object'},
                             'exceptionDetails': {'text': 'Uncaught', 'exception': {'description': 'Error: nope'}}})
        try:
            conn = cdp.connect(fake.port, wait=5)
            with self.assertRaisesRegex(cdp.CdpError, 'nope'):
                conn.evaluate('boom()')
            conn.close()
        finally:
            fake.close()

    def test_command_names_match_the_tauri_transport(self):
        self.assertEqual(cdp.tauri_command('update.install'), 'plugin:update|install')
        self.assertEqual(cdp.tauri_command('app.info'), 'plugin:os-app|info')
        self.assertEqual(cdp.invoke_expression('update.install', {'when': 'now'}),
                         'window.__TAURI_INTERNALS__.invoke("plugin:update|install", {"when": "now"})')

    def test_only_local_targets(self):
        with self.assertRaises(cdp.CdpError):
            cdp.Connection('ws://example.org/page')
        self.assertIsNone(cdp.pick_page([{'type': 'worker'}]))


if __name__ == '__main__':
    unittest.main()
