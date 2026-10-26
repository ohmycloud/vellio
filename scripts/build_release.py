#!/usr/bin/env python3
"""Compile a size-optimized desktop release without changing development builds.

VELLIO_VERSION=1.2.3 python scripts/build_release.py --target aarch64-apple-darwin
Then run scripts/package_release.py with the same target and version.
"""
import argparse
from pathlib import Path
import shlex
import subprocess

from package_release import TARGETS

ROOT = Path(__file__).resolve().parent.parent


def build_command(target, jobs=None):
    if target not in TARGETS:
        raise ValueError(f"Unsupported release target: {target}")
    if jobs is not None and jobs < 1:
        raise ValueError("Build jobs must be positive")
    macos = TARGETS[target][0] == "macos"
    command = ["cargo", "--config", str(ROOT / ".cargo/bundle-config.toml"),
               "rustc" if macos else "build", "--locked", "--release",
               "--features", "desktop", "--bin", "easyread-gpui", "--target", target]
    if jobs is not None:
        command += ["--jobs", str(jobs)]
    if macos:
        # Apply only to the final executable, never to dependencies/dylibs.
        # Mach-O adds '_' to Rust v0 names (_R...). Keep ObjC/C entry points
        # exported for GPUI and system frameworks; do not hide every symbol.
        # Pass '*' literally, without a shell expanding it into filenames.
        command += ["--", "-C", "link-arg=-Wl,-unexported_symbol,__R*"]
    return command


def build(target, jobs=None):
    command = build_command(target, jobs)
    print(shlex.join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--jobs", type=int, help="Cargo build parallelism (otherwise use Cargo defaults)")
    options = parser.parse_args()
    if options.jobs is not None and options.jobs < 1:
        parser.error("--jobs must be positive")
    build(options.target, options.jobs)
