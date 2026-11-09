#!/usr/bin/env python3
"""Tauri-Updater-Signaturen (minisign, Ed25519) ohne Fremdpakete prüfen (M3-Q02, OUT-252).

Eine Tauri-Signatur ist die Base64-Kodierung einer minisign-Datei; der öffentliche Schlüssel ist die
Base64-Kodierung einer minisign-Schlüsseldatei. Geprüft werden Schlüssel-ID, die Signatur über die
Nutzdaten (vorgehasht mit BLAKE2b-512 bei ``ED``), die globale Signatur über den vertrauenswürdigen
Kommentar und die dort eingetragene Version. Ed25519 nach RFC 8032, nur Verifikation.

Aufruf:
  python3 tauri_sig.py verify <datei> <signatur.sig> <pubkey-base64-datei|-> [--version X.Y.Z]
  python3 tauri_sig.py extract-pubkey <binärdatei>
"""
import argparse
import base64
import hashlib
import re
import sys
from pathlib import Path

# --- Ed25519 (RFC 8032, Abschnitt 5.1.7, nur Verifikation) -------------------------------------
P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def _add(a, b):
    x1, y1, z1, t1 = a
    x2, y2, z2, t2 = b
    aa = (y1 - x1) * (y2 - x2) % P
    bb = (y1 + x1) * (y2 + x2) % P
    cc = 2 * t1 * t2 * D % P
    dd = 2 * z1 * z2 % P
    e, f, g, h = bb - aa, dd - cc, dd + cc, bb + aa
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def _mul(scalar, point):
    result = (0, 1, 1, 0)
    while scalar:
        if scalar & 1:
            result = _add(result, point)
        point = _add(point, point)
        scalar >>= 1
    return result


def _equal(a, b):
    return (a[0] * b[2] - b[0] * a[2]) % P == 0 and (a[1] * b[2] - b[1] * a[2]) % P == 0


def _recover_x(y, sign):
    if y >= P:
        return None
    x2 = (y * y - 1) * pow(D * y * y + 1, P - 2, P) % P
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P:
        x = x * SQRT_M1 % P
    if (x * x - x2) % P:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


def _decompress(data):
    if len(data) != 32:
        return None
    y = int.from_bytes(data, 'little')
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    if x is None:
        return None
    return (x, y, 1, x * y % P)


_BY = 4 * pow(5, P - 2, P) % P
BASE = (_recover_x(_BY, 0), _BY, 1, _recover_x(_BY, 0) * _BY % P)


def ed25519_verify(public, message, signature):
    """True, wenn ``signature`` (64 Byte) eine gültige Ed25519-Signatur von ``message`` ist."""
    if len(public) != 32 or len(signature) != 64:
        return False
    a = _decompress(public)
    r = _decompress(signature[:32])
    if a is None or r is None:
        return False
    s = int.from_bytes(signature[32:], 'little')
    if s >= L:
        return False
    h = int.from_bytes(hashlib.sha512(signature[:32] + public + message).digest(), 'little') % L
    return _equal(_mul(s, BASE), _add(r, _mul(h, a)))


# --- minisign / Tauri ---------------------------------------------------------------------------
class SignatureError(Exception):
    """Signatur oder Schlüssel ungültig; die Meldung enthält keine Nutzdaten."""


def _b64(text, what):
    try:
        return base64.b64decode(text.strip(), validate=True)
    except (ValueError, TypeError) as error:
        raise SignatureError(f'{what}: kein gültiges Base64') from error


def public_key(encoded):
    """(Schlüssel-ID, 32-Byte-Schlüssel) aus dem Tauri-Format (Base64 einer minisign-Schlüsseldatei)."""
    lines = _b64(encoded, 'Öffentlicher Schlüssel').decode('utf-8', 'replace').splitlines()
    if len(lines) < 2 or not lines[0].startswith('untrusted comment:'):
        raise SignatureError('Öffentlicher Schlüssel: kein minisign-Format')
    blob = _b64(lines[1], 'Öffentlicher Schlüssel')
    if len(blob) != 42 or blob[:2] != b'Ed':
        raise SignatureError('Öffentlicher Schlüssel: unbekannter Algorithmus')
    return blob[2:10], blob[10:]


def parse_signature(encoded):
    lines = _b64(encoded, 'Signatur').decode('utf-8', 'replace').splitlines()
    if len(lines) < 4 or not lines[0].startswith('untrusted comment:') \
            or not lines[2].startswith('trusted comment: '):
        raise SignatureError('Signatur: kein minisign-Format')
    blob = _b64(lines[1], 'Signatur')
    if len(blob) != 74 or blob[:2] not in (b'Ed', b'ED'):
        raise SignatureError('Signatur: unbekannter Algorithmus')
    return dict(algorithm=blob[:2], key_id=blob[2:10], signature=blob[10:],
                trusted_comment=lines[2][len('trusted comment: '):],
                global_signature=_b64(lines[3], 'Globale Signatur'))


def trusted_fields(trusted_comment):
    fields = {}
    for part in trusted_comment.split('\t'):
        key, sep, value = part.partition(':')
        if sep:
            fields.setdefault(key.strip(), []).append(value.strip())
    return fields


def verify(payload, signature_b64, pubkey_b64, version=None):
    """Prüft wie ``update::verify_release``; gibt den vertrauenswürdigen Kommentar zurück."""
    key_id, key = public_key(pubkey_b64)
    sig = parse_signature(signature_b64)
    if sig['key_id'] != key_id:
        raise SignatureError('Signatur stammt von einem anderen Schlüssel')
    message = hashlib.blake2b(payload, digest_size=64).digest() if sig['algorithm'] == b'ED' else payload
    if not ed25519_verify(key, message, sig['signature']):
        raise SignatureError('Signatur der Nutzdaten ungültig')
    if not ed25519_verify(key, sig['signature'] + sig['trusted_comment'].encode('utf-8'),
                          sig['global_signature']):
        raise SignatureError('Signatur des vertrauenswürdigen Kommentars ungültig')
    if version is not None:
        versions = trusted_fields(sig['trusted_comment']).get('version', [])
        if [v.removeprefix('v') for v in versions] != [version]:
            raise SignatureError('Signierte Version stimmt nicht mit dem Manifest überein')
    return sig['trusted_comment']


# Base64-Präfix von "untrusted comment: minisign public key" (36 Byte = 48 Zeichen, ohne Füllung).
PUBKEY_PREFIX = base64.b64encode(b'untrusted comment: minisign public key'[:36])


def embedded_public_keys(binary):
    """Alle im Binary eingebetteten Tauri-Schlüssel (``OPENSPIDER_UPDATER_PUBKEY`` zur Bauzeit)."""
    found = set()
    for match in re.finditer(re.escape(PUBKEY_PREFIX) + rb'[A-Za-z0-9+/]*={0,2}', binary):
        candidate = match.group(0).decode('ascii')
        try:
            public_key(candidate)
        except SignatureError:
            continue
        found.add(candidate)
    return sorted(found)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    v = sub.add_parser('verify')
    v.add_argument('file', type=Path)
    v.add_argument('signature', type=Path)
    v.add_argument('pubkey', help='Datei mit dem Base64-Schlüssel oder - für stdin')
    v.add_argument('--version')
    e = sub.add_parser('extract-pubkey')
    e.add_argument('binary', type=Path)
    args = parser.parse_args(argv)
    if args.command == 'extract-pubkey':
        keys = embedded_public_keys(args.binary.read_bytes())
        if len(keys) != 1:
            print(f'Erwartet genau einen eingebetteten Schlüssel, gefunden: {len(keys)}', file=sys.stderr)
            return 1
        print(keys[0])
        return 0
    pubkey = sys.stdin.read() if args.pubkey == '-' else Path(args.pubkey).read_text(encoding='utf-8')
    try:
        comment = verify(args.file.read_bytes(), args.signature.read_text(encoding='utf-8'), pubkey, args.version)
    except SignatureError as error:
        print(f'abgelehnt: {error}', file=sys.stderr)
        return 1
    print(f'gültig: {comment}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
