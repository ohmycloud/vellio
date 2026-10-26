"""Release build contracts; no compiler, signing identity or network required."""
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

import build_release as release


class ReleaseBuildTests(unittest.TestCase):
    def test_bundle_profile_only_optimizes_application_for_size(self):
        with (release.ROOT / ".cargo/bundle-config.toml").open("rb") as stream:
            config = tomllib.load(stream)
        profile = config["profile"]["release"]
        self.assertEqual(profile["opt-level"], 3)
        self.assertEqual(profile["strip"], "debuginfo")
        self.assertEqual(profile["package"]["easyread-gpui"]["opt-level"], "z")
        self.assertEqual(profile["package"]["easyread-gpui"]["codegen-units"], 1)
        self.assertNotIn("*", profile["package"])
        with (release.ROOT / "Cargo.toml").open("rb") as stream:
            normal = tomllib.load(stream)
        self.assertNotIn("release", normal.get("profile", {}))
        self.assertFalse((release.ROOT / ".cargo/config.toml").exists())

    def test_mac_hides_only_rust_v0_symbols_on_the_final_executable(self):
        for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"]:
            with self.subTest(target=target):
                command = release.build_command(target)
                self.assertEqual(command[3], "rustc")
                self.assertEqual(command[-3:], ["--", "-C", "link-arg=-Wl,-unexported_symbol,__R*"])
                self.assertEqual(command[command.index("--bin") + 1], "easyread-gpui")
                self.assertIn("--locked", command)
                self.assertIn("--release", command)
                self.assertEqual(command[command.index("--target") + 1], target)

    def test_other_platforms_never_receive_macho_linker_options(self):
        for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]:
            with self.subTest(target=target):
                command = release.build_command(target, 2)
                self.assertEqual(command[3], "build")
                self.assertNotIn("--", command)
                self.assertFalse(any("unexported_symbol" in arg for arg in command))
                self.assertEqual(command[-2:], ["--jobs", "2"])

    def test_build_uses_repo_root_preserves_environment_and_propagates_failures(self):
        command = release.build_command("aarch64-apple-darwin", 2)
        with patch.object(release.subprocess, "run") as run:
            release.build("aarch64-apple-darwin", 2)
            run.assert_called_once_with(command, cwd=release.ROOT, check=True)
        with patch.object(release.subprocess, "run", side_effect=subprocess.CalledProcessError(1, command)):
            with self.assertRaises(subprocess.CalledProcessError):
                release.build("aarch64-apple-darwin", 2)
        with self.assertRaises(ValueError):
            release.build_command("unsupported")
        with self.assertRaises(ValueError):
            release.build_command("aarch64-apple-darwin", 0)


@unittest.skipUnless(os.name == "posix", "macOS entrypoint requires a POSIX shell")
class MacBundleEntrypointTests(unittest.TestCase):
    """Run the actual shell entrypoint with isolated resources and stub tools."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="vellio-bundle-contract-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.log = self.root / "commands.log"
        script = self.root / "scripts/package_macos.sh"
        script.parent.mkdir()
        script.write_text((release.ROOT / "scripts/package_macos.sh").read_text())
        for name in ["assets/pdfium/libpdfium.dylib", "assets/pdfium/licenses/TEST.txt",
                     "assets/mitex/LICENSE", "assets/fonts/OFL.txt", "assets/icons/Vellio.icns",
                     "vendor/gpui-base/LICENSE-APACHE"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture resource")
        tools = self.root / "tools"
        tools.mkdir()
        programs = {
            "rustc": "printf 'host: aarch64-apple-darwin\\n'\n",
            "python3": '''[ "${FAIL_BUILD:-0}" != 1 ] || exit 7
mkdir -p target/aarch64-apple-darwin/release
printf 'release' > target/aarch64-apple-darwin/release/easyread-gpui
''',
            "cargo": '''mkdir -p target/debug
printf 'debug' > target/debug/easyread-gpui
''',
            "xcrun": '''[ "${FAIL_STRIP:-0}" != 1 ] || exit 8
[ "$1" = strip ] && [ "$2" = -x ] || exit 10
printf '%s' '-stripped' >> "$3"
''',
            "codesign": '''if [ "$1" = --verify ] && [ "${FAIL_VERIFY:-0}" = 1 ]; then exit 9; fi
''',
        }
        for name, body in programs.items():
            tool = tools / name
            tool.write_text(f'''#!/bin/sh
set -eu
printf '%s' '{name}' >> "$MAC_PACKAGE_LOG"
printf ' %s' "$@" >> "$MAC_PACKAGE_LOG"
printf '\\n' >> "$MAC_PACKAGE_LOG"
{body}''')
            tool.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
                    "MAC_PACKAGE_LOG": str(self.log), "FAIL_BUILD": "0", "FAIL_STRIP": "0",
                    "FAIL_VERIFY": "0"}

    def run_bundle(self, *args, **env):
        return subprocess.run(["sh", "scripts/package_macos.sh", *args], cwd=self.root,
                              env={**self.env, **env}, capture_output=True, text=True)

    def commands(self):
        return self.log.read_text() if self.log.exists() else ""

    def test_default_and_explicit_release_use_optimized_build_and_strip_before_signing(self):
        for args in [(), ("--release",)]:
            with self.subTest(args=args):
                self.log.unlink(missing_ok=True)
                result = self.run_bundle(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
                commands = self.commands()
                self.assertIn("python3 scripts/build_release.py --target aarch64-apple-darwin", commands)
                self.assertNotIn("cargo build", commands)
                self.assertLess(commands.index("xcrun strip -x"), commands.index("codesign --force"))
                self.assertLess(commands.index("codesign --force"), commands.index("codesign --verify --deep --strict"))
                self.assertEqual((self.root / "target/Vellio.app/Contents/MacOS/Vellio").read_text(),
                                 "release-stripped")
                self.assertEqual((self.root / "target/aarch64-apple-darwin/release/easyread-gpui").read_text(),
                                 "release", "strip must never modify Cargo's build artifact")
                self.assertIn("release bundle (ad-hoc signed)", result.stdout)

    def test_development_requires_explicit_flag_and_does_not_strip(self):
        result = self.run_bundle("--dev")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cargo build --locked --features desktop", self.commands())
        self.assertNotIn("python3", self.commands())
        self.assertNotIn("xcrun", self.commands())
        self.assertIn("larger and unoptimized", result.stderr)
        self.assertIn("development bundle", result.stdout)
        self.assertEqual((self.root / "target/Vellio.app/Contents/MacOS/Vellio").read_text(), "debug")

    def test_invalid_or_extra_arguments_do_not_build(self):
        for args in [("--debug",), ("",), ("--release", "--dev")]:
            with self.subTest(args=args):
                result = self.run_bundle(*args)
                self.assertEqual(result.returncode, 2)
                self.assertIn("Usage:", result.stderr)
                self.assertEqual(self.commands(), "")

    def test_build_failure_keeps_existing_bundle(self):
        executable = self.root / "target/Vellio.app/Contents/MacOS/Vellio"
        executable.parent.mkdir(parents=True)
        executable.write_text("existing bundle")
        result = self.run_bundle(FAIL_BUILD="1")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(executable.read_text(), "existing bundle")
        self.assertNotIn("codesign", self.commands())

    def test_strip_failure_stops_before_signing(self):
        result = self.run_bundle(FAIL_STRIP="1")
        self.assertEqual(result.returncode, 8)
        self.assertNotIn("codesign", self.commands())
        self.assertNotIn("bundle (ad-hoc signed)", result.stdout)

    def test_verification_failure_is_not_reported_as_success(self):
        result = self.run_bundle(FAIL_VERIFY="1")
        self.assertEqual(result.returncode, 9)
        self.assertIn("codesign --verify --deep --strict", self.commands())
        self.assertNotIn("bundle (ad-hoc signed)", result.stdout)


if __name__ == "__main__":
    unittest.main()
