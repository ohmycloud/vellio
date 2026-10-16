"""Portable archive contracts; no compiler, certificate, network or installer required."""
import hashlib
import os
from pathlib import Path
import plistlib
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import mac_sign
import package_release as release


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="vellio-package-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.out = self.root / "out"
        self.engine = self.root / "pdfium"
        self.engine.mkdir()
        self.addCleanup(patch.stopall)
        patch.object(release, "ROOT", self.root).start()
        for source in ["assets/pdfium/licenses/LICENSE", "assets/mitex/LICENSE", "assets/fonts/OFL.txt",
                       "vendor/gpui-base/LICENSE-APACHE", "assets/icons/Vellio.icns"]:
            path = self.root / source
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture license or icon")

    def build(self, target):
        os_name, _, engine = release.TARGETS[target]
        binary = self.root / "target" / target / "release" / (
            "easyread-gpui.exe" if os_name == "windows" else "easyread-gpui")
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"executable fixture")
        binary.chmod(0o755)
        (self.engine / engine).write_bytes(b"engine fixture")

    def test_windows_archive_contains_engine_licenses_and_checksum(self):
        target = "x86_64-pc-windows-msvc"
        self.build(target)
        with patch.dict(os.environ, {"WINDOWS_SIGN_PFX": ""}):
            archive = release.package(target, "1.2.3", self.engine, self.out)
        self.assertEqual(archive.name, "Vellio-1.2.3-windows-x86_64.zip")
        with zipfile.ZipFile(archive) as stream:
            for name in ["Vellio/Vellio.exe", "Vellio/pdfium.dll", "Vellio/licenses/pdfium/LICENSE"]:
                self.assertIn(name, stream.namelist())
        self.assertTrue(archive.with_name(archive.name + ".sha256").read_text().startswith(
            hashlib.sha256(archive.read_bytes()).hexdigest()))

    def test_linux_archive_keeps_executable_permissions_and_adjacent_engine(self):
        target = "x86_64-unknown-linux-gnu"
        self.build(target)
        archive = release.package(target, "1.2.3", self.engine, self.out)
        self.assertEqual(archive.name, "Vellio-1.2.3-linux-x86_64.tar.gz")
        with tarfile.open(archive) as stream:
            self.assertEqual(stream.getmember("Vellio/Vellio").mode & 0o777, 0o755)
            self.assertIn("Vellio/libpdfium.so", stream.getnames())

    def test_mac_bundle_version_identifier_and_signing_requirement(self):
        target = "aarch64-apple-darwin"
        self.build(target)
        def sign(app, required):
            self.assertTrue(required)
            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
            self.assertEqual(info["CFBundleShortVersionString"], "1.2.3")
            self.assertEqual(info["CFBundleIdentifier"], "org.easyread.gpui")
            self.assertTrue((app / "Contents/MacOS/libpdfium.dylib").is_file())
        def command(*args):
            self.assertEqual(args[0], "hdiutil")
            Path(args[-1]).write_bytes(b"dmg fixture")
        with patch.object(release, "sign_app", sign), patch.object(release, "run", command):
            archive = release.package(target, "1.2.3", self.engine, self.out, True)
        self.assertEqual(archive.name, "Vellio-1.2.3-macos-aarch64.dmg")

    def test_missing_engine_invalid_version_and_unsigned_mac_release_fail_closed(self):
        with self.assertRaises(RuntimeError):
            release.package("x86_64-unknown-linux-gnu", "1.2.3", self.engine, self.out)
        with self.assertRaises(ValueError):
            release.package("x86_64-unknown-linux-gnu", "../../bad", self.engine, self.out)
        with patch.dict(os.environ, {"MAC_SIGN_P12": ""}):
            with self.assertRaises(RuntimeError):
                mac_sign.sign_app(self.root / "Vellio.app", require_signing=True)


if __name__ == "__main__":
    unittest.main()
