#!/usr/bin/env python3
"""Minimaler Chrome-DevTools-Protocol-Client für WebView2 (M3-Q02), nur Standardbibliothek.

WebView2 öffnet einen DevTools-Port, wenn die installierte App mit
``WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>`` startet (derselbe Weg, den
``msedgedriver`` für WebView2 nutzt). Darüber ruft die Update-Matrix die IPC-Befehle der Oberfläche
(``update.check``, ``update.download``, ``update.install``) in der echten Release-App auf. Unter macOS
(WKWebView) gibt es in Release-Builds keinen solchen Zugang.
"""
import base64
import json
import os
import socket
import struct
import time
import urllib.request
from urllib.parse import urlsplit


class CdpError(Exception):
    pass


def encode_frame(payload, opcode=0x1, mask=None):
    """Client-Frame (immer maskiert, RFC 6455 5.3)."""
    mask = mask if mask is not None else os.urandom(4)
    header = bytearray([0x80 | opcode])
    length = len(payload)
    if length < 126:
        header.append(0x80 | length)
    elif length < 1 << 16:
        header.append(0x80 | 126)
        header += struct.pack('>H', length)
    else:
        header.append(0x80 | 127)
        header += struct.pack('>Q', length)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    return bytes(header) + mask + masked


def _read_exact(sock, n):
    data = bytearray()
    while len(data) < n:
        chunk = sock.recv(n - len(data))
        if not chunk:
            raise CdpError('Verbindung geschlossen')
        data += chunk
    return bytes(data)


def read_frame(sock):
    """(opcode, payload, fin) eines Frames; Server-Frames sind unmaskiert, maskierte werden entschlüsselt."""
    first, second = _read_exact(sock, 2)
    fin, opcode = bool(first & 0x80), first & 0x0F
    length = second & 0x7F
    if length == 126:
        length = struct.unpack('>H', _read_exact(sock, 2))[0]
    elif length == 127:
        length = struct.unpack('>Q', _read_exact(sock, 8))[0]
    mask = _read_exact(sock, 4) if second & 0x80 else None
    payload = _read_exact(sock, length)
    if mask:
        payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    return opcode, payload, fin


class Connection:
    def __init__(self, ws_url, timeout=30):
        parts = urlsplit(ws_url)
        if parts.scheme != 'ws' or parts.hostname not in ('127.0.0.1', 'localhost'):
            raise CdpError('Nur lokale ws://-Ziele')
        self.sock = socket.create_connection((parts.hostname, parts.port or 80), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode('ascii')
        path = parts.path + (f'?{parts.query}' if parts.query else '')
        self.sock.sendall((f'GET {path} HTTP/1.1\r\nHost: {parts.netloc}\r\nUpgrade: websocket\r\n'
                           f'Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n'
                           'Sec-WebSocket-Version: 13\r\n\r\n').encode('ascii'))
        response = b''
        while b'\r\n\r\n' not in response:
            response += _read_exact(self.sock, 1)
        if not response.startswith(b'HTTP/1.1 101'):
            raise CdpError('WebSocket-Handshake abgelehnt: ' + response.split(b'\r\n', 1)[0].decode('latin-1'))
        self._next = 0

    def close(self):
        try:
            self.sock.sendall(encode_frame(b'', opcode=0x8))
        except OSError:
            pass
        self.sock.close()

    def _message(self):
        parts, opcode = [], None
        while True:
            op, payload, fin = read_frame(self.sock)
            if op == 0x9:
                self.sock.sendall(encode_frame(payload, opcode=0xA))
                continue
            if op == 0x8:
                raise CdpError('Verbindung vom Ziel geschlossen')
            if op in (0x1, 0x2):
                opcode, parts = op, [payload]
            elif op == 0x0:
                parts.append(payload)
            if fin and opcode is not None:
                return b''.join(parts)

    def call(self, method, params=None, timeout=60):
        self._next += 1
        ident = self._next
        self.sock.sendall(encode_frame(json.dumps({'id': ident, 'method': method, 'params': params or {}}).encode()))
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.sock.settimeout(max(0.1, deadline - time.monotonic()))
            message = json.loads(self._message())
            if message.get('id') == ident:
                if 'error' in message:
                    raise CdpError(json.dumps(message['error']))
                return message.get('result', {})
        raise CdpError(f'{method}: keine Antwort')

    def evaluate(self, expression, timeout=60):
        """JavaScript im Seitenkontext ausführen, Promise abwarten, Wert als JSON zurückgeben."""
        result = self.call('Runtime.evaluate', {'expression': expression, 'awaitPromise': True,
                                                'returnByValue': True, 'timeout': int(timeout * 1000)},
                           timeout=timeout + 5)
        if 'exceptionDetails' in result:
            details = result['exceptionDetails']
            text = details.get('exception', {}).get('description') or details.get('text')
            raise CdpError(f'JavaScript-Fehler: {text}')
        return result.get('result', {}).get('value')


def targets(port, timeout=5):
    with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list', timeout=timeout) as response:
        return json.loads(response.read().decode('utf-8'))


def pick_page(items):
    """Das Hauptfenster der App (Tauri unter Windows: ``http://tauri.localhost``)."""
    pages = [t for t in items if t.get('type') == 'page' and t.get('webSocketDebuggerUrl')]
    preferred = [t for t in pages if urlsplit(t.get('url', '')).hostname == 'tauri.localhost'
                 or t.get('url', '').startswith('tauri://')]
    chosen = (preferred or pages)
    return chosen[0] if chosen else None


def connect(port, wait=90):
    deadline = time.monotonic() + wait
    last = None
    while time.monotonic() < deadline:
        try:
            page = pick_page(targets(port))
            if page:
                return Connection(page['webSocketDebuggerUrl'])
        except (OSError, ValueError, CdpError) as error:
            last = error
        time.sleep(1)
    raise CdpError(f'Kein DevTools-Ziel auf Port {port} ({last})')


def tauri_command(name):
    """``update.check`` → ``plugin:update|check`` (wie ``apps/desktop/src/ipc/transport/tauri.ts``)."""
    core = {'app', 'event', 'image', 'menu', 'path', 'resources', 'tray', 'webview', 'window'}
    domain, _, command = name.partition('.')
    if not command:
        return name
    return f'plugin:{"os-" + domain if domain in core else domain}|{command}'


def invoke_expression(name, args=None):
    return (f'window.__TAURI_INTERNALS__.invoke({json.dumps(tauri_command(name))}, '
            f'{json.dumps(args or {})})')
