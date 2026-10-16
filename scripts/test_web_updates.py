"""HTTP regression for the built local Web server; cached fixtures, no GitHub calls.
Run cargo build --manifest-path web-topcoat/Cargo.toml first.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import ProxyHandler, Request, build_opener

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "web-topcoat/target/debug" / (
    "vellio-web-topcoat.exe" if os.name == "nt" else "vellio-web-topcoat")


@unittest.skipUnless(BINARY.is_file(), "Build web-topcoat before running HTTP regressions")
class WebUpdatesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="vellio-update-http-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "config").mkdir()
        names = [f"Vellio-99.0.0-{system}-{arch}.{extension}"
                 for system, extension in [("macos", "dmg"), ("windows", "zip"), ("linux", "tar.gz")]
                 for arch in ["aarch64", "x86_64"]]
        self.cache = {"enabled": False, "checked_at": int(time.time()), "failed": False,
                      "release": {"version": "99.0.0", "url": "https://github.com/ohmycloud/vellio/releases/tag/v99.0.0",
                                  "notes": "<script>not executable</script>", "published": "",
                                  "assets": [{"name": name, "url": "https://github.com/ohmycloud/vellio/releases/download/v99.0.0/" + name}
                                             for name in names]}}
        (self.root / "config/updates.json").write_text(json.dumps(self.cache))
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        self.base = f"http://127.0.0.1:{port}"
        self.http = build_opener(ProxyHandler({}))
        env = dict(os.environ, EASYREAD_LOCATION_FILE=str(self.root / "config/location.json"),
                   VELLIO_WEB_LIBRARY=str(self.root / "library"), VELLIO_DESKTOP_LIBRARY=str(self.root / "desktop"),
                   HOST="127.0.0.1", PORT=str(port), EASYREAD_MODEL="")
        self.proc = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.addCleanup(self.stop)
        for _ in range(100):
            try:
                self.get("/")
                break
            except OSError as error:
                if isinstance(error, HTTPError):
                    error.close()
                time.sleep(.05)
        else:
            self.fail("Web server did not start")

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()

    def get(self, path):
        with self.http.open(self.base + path, timeout=5) as response:
            return response.read().decode()

    def post(self, path, data, headers=None):
        with self.http.open(Request(self.base + path, data=urlencode(data).encode(), headers=headers or {}), timeout=5) as response:
            return json.load(response)

    def test_about_and_host_architecture_download_preserve_opt_out_and_escape_notes(self):
        self.assertIn('/settings?tab=about', self.get('/'))
        page = self.get('/settings?tab=about')
        for text in ['id="check-updates"', 'id="automatic-updates"', 'About / Updates', '99.0.0']:
            self.assertIn(text, page)
        self.assertNotIn('<script>not executable</script>', page)
        state = json.loads(self.get('/settings/updates/status'))
        self.assertFalse(state['enabled'])
        self.assertFalse(state['checking'])
        self.assertEqual(state['latest'], '99.0.0')
        self.assertTrue(state['download'].startswith('https://github.com/ohmycloud/vellio/releases/download/v99.0.0/'))
        self.assertEqual(state['notes'], '<script>not executable</script>')
        self.assertIn("textContent = state.notes", self.get('/updates.js'))
        self.assertFalse(self.post('/settings/updates/automatic', {'enabled': 'false'})['enabled'])
        self.assertEqual(json.loads((self.root / 'config/updates.json').read_text()), self.cache)
        self.assertEqual(list((self.root / 'library').iterdir()), [], "update UI must not create or modify books")

    def test_update_preference_rejects_cross_origin_and_invalid_input(self):
        for data, headers, expected in [({'enabled': 'not-a-bool'}, {}, 400),
                                         ({'enabled': 'false'}, {'Origin': 'https://untrusted.example'}, 403)]:
            with self.assertRaises(HTTPError) as caught:
                self.post('/settings/updates/automatic', data, headers)
            self.assertEqual(caught.exception.code, expected)
            caught.exception.close()


if __name__ == '__main__':
    unittest.main()
