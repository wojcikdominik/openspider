"""Tests für tauri_sig.py gegen RFC-8032-Vektoren und die Update-Fixtures (M3-Q02).

Testinfrastruktur der Matrix, nicht das Produkt: bewusst ohne `# spec:`-Tags. Produktprüfungen mit
Spec-Tags: test_cli_integration.py und die Matrix-Läufe von install-test.yml.
"""
import base64
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import tauri_sig

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'tests' / 'fixtures' / 'update'


class Ed25519(unittest.TestCase):
    # RFC 8032, Abschnitt 7.1, Tests 1 und 2
    VECTORS = [
        ('d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a', '',
         'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b'),
        ('3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c', '72',
         '92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00'),
    ]

    def test_rfc8032_vectors(self):
        for public, message, signature in self.VECTORS:
            self.assertTrue(tauri_sig.ed25519_verify(bytes.fromhex(public), bytes.fromhex(message),
                                                     bytes.fromhex(signature)))

    def test_modified_message_signature_or_key_fails(self):
        public, message, signature = (bytes.fromhex(x) for x in self.VECTORS[1])
        self.assertFalse(tauri_sig.ed25519_verify(public, message + b'\0', signature))
        self.assertFalse(tauri_sig.ed25519_verify(public, message, signature[:-1] + bytes([signature[-1] ^ 1])))
        self.assertFalse(tauri_sig.ed25519_verify(bytes([public[0] ^ 1]) + public[1:], message, signature))
        self.assertFalse(tauri_sig.ed25519_verify(public, message, signature[:63]))


class TauriSignature(unittest.TestCase):
    def setUp(self):
        self.payload = (FIXTURES / 'artifact').read_bytes()
        self.signature = (FIXTURES / 'artifact.sig').read_text()
        self.key = (FIXTURES / 'test.key.pub').read_text()

    def test_fixture_artifact_is_valid_for_its_signed_version(self):
        comment = tauri_sig.verify(self.payload, self.signature, self.key, '1.0.1')
        self.assertIn('version:1.0.1', comment)
        self.assertEqual(tauri_sig.trusted_fields(comment)['file'], ['artifact'])

    def test_one_modified_byte_is_rejected(self):
        for index in (0, len(self.payload) // 2, len(self.payload) - 1):
            changed = bytearray(self.payload)
            changed[index] ^= 0x01
            with self.assertRaises(tauri_sig.SignatureError):
                tauri_sig.verify(bytes(changed), self.signature, self.key, '1.0.1')

    def test_replayed_signature_for_another_version_is_rejected(self):
        with self.assertRaisesRegex(tauri_sig.SignatureError, 'Version'):
            tauri_sig.verify(self.payload, self.signature, self.key, '1.0.2')

    def test_tampered_trusted_comment_is_rejected(self):
        text = base64.b64decode(self.signature).decode()
        forged = base64.b64encode(text.replace('version:1.0.1', 'version:1.0.2').encode()).decode()
        with self.assertRaisesRegex(tauri_sig.SignatureError, 'Kommentar'):
            tauri_sig.verify(self.payload, forged, self.key, '1.0.2')

    def test_other_key_is_rejected(self):
        lines = base64.b64decode(self.key).decode().splitlines()
        blob = bytearray(base64.b64decode(lines[1]))
        blob[-1] ^= 0x01  # gleiche Schlüssel-ID, anderer Schlüssel
        other = base64.b64encode(f'{lines[0]}\n{base64.b64encode(bytes(blob)).decode()}\n'.encode()).decode()
        with self.assertRaises(tauri_sig.SignatureError):
            tauri_sig.verify(self.payload, self.signature, other, '1.0.1')
        blob[3] ^= 0x01  # andere Schlüssel-ID
        other = base64.b64encode(f'{lines[0]}\n{base64.b64encode(bytes(blob)).decode()}\n'.encode()).decode()
        with self.assertRaisesRegex(tauri_sig.SignatureError, 'anderen Schlüssel'):
            tauri_sig.verify(self.payload, self.signature, other, '1.0.1')

    def test_garbage_is_reported_without_crash(self):
        for bad in ('', 'not base64!', base64.b64encode(b'hello').decode()):
            with self.assertRaises(tauri_sig.SignatureError):
                tauri_sig.verify(self.payload, bad, self.key)
            with self.assertRaises(tauri_sig.SignatureError):
                tauri_sig.verify(self.payload, self.signature, bad)

    def test_embedded_key_is_found_once_in_a_binary(self):
        key = self.key.strip().encode()
        binary = b'\x7fELF\0' + key + b'\0padding\0' + key + b'\0' + tauri_sig.PUBKEY_PREFIX + b'AAAA\0'
        self.assertEqual(tauri_sig.embedded_public_keys(binary), [self.key.strip()])
        self.assertEqual(tauri_sig.embedded_public_keys(b'no key here'), [])

    def test_command_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            key = Path(tmp) / 'key'
            key.write_text(self.key)
            out = io.StringIO()
            with redirect_stdout(out):
                self.assertEqual(tauri_sig.main(['verify', str(FIXTURES / 'artifact'), str(FIXTURES / 'artifact.sig'),
                                                 str(key), '--version', '1.0.1']), 0)
            self.assertIn('gültig', out.getvalue())
            changed = Path(tmp) / 'artifact'
            changed.write_bytes(self.payload[:-1] + bytes([self.payload[-1] ^ 1]))
            with redirect_stderr(io.StringIO()):
                self.assertEqual(tauri_sig.main(['verify', str(changed), str(FIXTURES / 'artifact.sig'), str(key)]), 1)
            binary = Path(tmp) / 'bin'
            binary.write_bytes(b'\0' + self.key.strip().encode() + b'\0')
            out = io.StringIO()
            with redirect_stdout(out):
                self.assertEqual(tauri_sig.main(['extract-pubkey', str(binary)]), 0)
            self.assertEqual(out.getvalue().strip(), self.key.strip())


if __name__ == '__main__':
    unittest.main()
