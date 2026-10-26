"""Release build contracts; no compiler, signing identity or network required."""
import subprocess
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


if __name__ == "__main__":
    unittest.main()
