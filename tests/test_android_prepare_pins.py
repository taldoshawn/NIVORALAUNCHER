"""Preparation uses one authenticated manifest for metadata and dependencies."""
import contextlib
import hashlib
import http.server
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch

from tools import android_prepare as prepare


def sha1(data):
    return hashlib.sha1(data).hexdigest()


class PreparationPinsTests(unittest.TestCase):
    @contextlib.contextmanager
    def fixture(self):
        routes, requested = {}, []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                requested.append(self.path)
                body = routes.get(self.path)
                if body is None:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                base = f'http://127.0.0.1:{server.server_port}'
                # These are small original fixture bytes, not game resources.
                client, srv, library = b'client fixture', b'server fixture', b'old library fixture'
                routes.update({'/client': client, '/server': srv, '/old-library': library})
                index = json.dumps({'objects': {}}).encode()
                routes['/assets.json'] = index
                artifact = lambda path, body: {'url': base + path, 'sha1': sha1(body)}
                manifest = {
                    'id': 'fixture',
                    'downloads': {'client': artifact('/client', client), 'server': artifact('/server', srv)},
                    'libraries': [{
                        'name': 'org.example:freetype:old',
                        'downloads': {'artifact': {
                            **artifact('/old-library', library),
                            'path': 'org/example/freetype/old/freetype-old.jar',
                        }},
                    }],
                    'assetIndex': {'id': 'fixture', **artifact('/assets.json', index)},
                }
                original = json.dumps(manifest).encode()
                digest = sha1(original)
                manifest_route = f'/packages/{digest}/fixture.json'
                routes[manifest_route] = original
                # Simulate a republished manifest with the same game jars but
                # another library version, which is not the preparer's input.
                newer = json.loads(original)
                newer['libraries'][0]['name'] = 'org.example:freetype:new'
                newer['libraries'][0]['downloads']['artifact']['path'] = 'freetype-new.jar'
                routes['/current.json'] = json.dumps(newer).encode()
                pins = {'version': 'fixture', 'manifest_sha1': digest,
                        'client_sha1': sha1(client), 'server_sha1': sha1(srv)}
                (root / 'research-lock.json').write_text(json.dumps({'minecraft': pins}))
                with patch.object(prepare, 'ROOT', root), patch.object(prepare, 'PACKAGES', base + '/packages'):
                    yield root, routes, requested, manifest, original, manifest_route
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_republished_metadata_does_not_replace_pinned_dependencies(self):
        with self.fixture() as (root, _, requested, expected, original, _):
            seed = root / '.local/minecraft/fixture/manifest.json'
            seed.parent.mkdir(parents=True)
            seed.write_bytes(b'corrupt cached manifest')
            minecraft = root / 'minecraft'
            actual, server = prepare.download_game('fixture', minecraft)
            self.assertEqual(actual, expected)
            self.assertEqual(seed.read_bytes(), original)
            self.assertEqual((minecraft / 'versions/fixture/fixture.json').read_bytes(), original)
            self.assertEqual((minecraft / 'libraries/org/example/freetype/old/freetype-old.jar').read_bytes(),
                             b'old library fixture')
            self.assertEqual(server.read_bytes(), b'server fixture')
            self.assertFalse((minecraft / 'libraries/freetype-new.jar').exists())
            self.assertNotIn('/current.json', requested)

    def test_bad_manifest_checksum_stops_before_downloading_game_inputs(self):
        with self.fixture() as (root, routes, requested, _, _, manifest_route):
            routes[manifest_route] = b'tampered manifest'
            with self.assertRaisesRegex(ValueError, 'Checksum mismatch'):
                prepare.download_game('fixture', root / 'minecraft')
            self.assertEqual(requested, [manifest_route])
            self.assertFalse((root / 'minecraft/versions/fixture/fixture.json').exists())
            self.assertFalse((root / 'minecraft/libraries').exists())


if __name__ == '__main__':
    unittest.main()
