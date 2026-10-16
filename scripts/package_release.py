#!/usr/bin/env python3
"""Build native release archives (Python is a build tool, not an app dependency).

First build with VELLIO_VERSION set to --version, and install a matching PDFium
wheel with: pip install --no-deps --target target/pdfium-wheel pypdfium2==5.13.0
Then: python scripts/package_release.py --target aarch64-apple-darwin --version 0.1.0
"""
import argparse
import base64
import hashlib
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

from mac_sign import sign_app

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "aarch64-apple-darwin": ("macos", "aarch64", "libpdfium.dylib"),
    "x86_64-apple-darwin": ("macos", "x86_64", "libpdfium.dylib"),
    "x86_64-pc-windows-msvc": ("windows", "x86_64", "pdfium.dll"),
    "x86_64-unknown-linux-gnu": ("linux", "x86_64", "libpdfium.so"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64", "libpdfium.so"),
}


def run(*args):
    try:
        subprocess.run([str(arg) for arg in args], check=True)
    except subprocess.CalledProcessError as error:
        # signtool arguments may contain the PFX password.
        raise RuntimeError(f"{args[0]} failed (exit {error.returncode})") from None


def copy_licenses(destination):
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ROOT / "assets/pdfium/licenses", destination / "pdfium")
    for source, name in [("assets/mitex/LICENSE", "MiTeX.txt"),
                         ("assets/fonts/OFL.txt", "NotoSerifSC-OFL.txt"),
                         ("vendor/gpui-base/LICENSE-APACHE", "GPUI-Base.txt")]:
        shutil.copy2(ROOT / source, destination / name)
    (destination / "PDFium-source.txt").write_text(
        "PDFium build 7999 from pypdfium2 5.13.0 wheels (pdfium-binaries).\n"
        "https://github.com/pypdfium2-team/pypdfium2\n"
        "https://github.com/bblanchon/pdfium-binaries\n", encoding="utf-8")


def sign_windows(executable):
    encoded = os.environ.get("WINDOWS_SIGN_PFX")
    if not encoded:
        return
    signtool = shutil.which("signtool")
    if not signtool:
        candidates = sorted((Path(os.environ["ProgramFiles(x86)"]) /
                             "Windows Kits/10/bin").glob("*/x64/signtool.exe"))
        if not candidates:
            raise RuntimeError("Windows SDK signtool.exe not found")
        signtool = str(candidates[-1])
    with tempfile.TemporaryDirectory(prefix="vellio-win-sign-") as folder:
        cert = Path(folder) / "cert.pfx"
        cert.write_bytes(base64.b64decode(encoded, validate=True))
        run(signtool, "sign", "/f", cert, "/p", os.environ.get("WINDOWS_SIGN_PFX_PASSWORD", ""),
            "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", executable)
        run(signtool, "verify", "/pa", executable)


def package(target, version, pdfium_dir, out, require_signing=False):
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        raise ValueError("Version must be a semantic version without the v prefix")
    os_name, arch, engine_name = TARGETS[target]
    binary = ROOT / "target" / target / "release" / ("easyread-gpui.exe" if os_name == "windows" else "easyread-gpui")
    engines = list(pdfium_dir.rglob(engine_name))
    if not binary.is_file() or len(engines) != 1:
        raise RuntimeError(f"Need {binary} and exactly one {engine_name} in {pdfium_dir}")
    out.mkdir(parents=True, exist_ok=True)
    stem = f"Vellio-{version}-{os_name}-{arch}"
    with tempfile.TemporaryDirectory(prefix="vellio-release-") as folder:
        staging = Path(folder)
        if os_name == "macos":
            app = staging / "Vellio.app"
            executable_dir = app / "Contents/MacOS"
            resources = app / "Contents/Resources"
            executable_dir.mkdir(parents=True)
            copy_licenses(resources / "licenses")
            shutil.copy2(binary, executable_dir / "Vellio")
            shutil.copy2(engines[0], executable_dir / engine_name)
            shutil.copy2(ROOT / "assets/icons/Vellio.icns", resources / "Vellio.icns")
            # Keep the identifier used by existing development installations.
            plist = {"CFBundleExecutable": "Vellio", "CFBundleIdentifier": "org.easyread.gpui",
                     "CFBundleName": "Vellio", "CFBundleDisplayName": "Vellio",
                     "CFBundleIconFile": "Vellio.icns", "CFBundleVersion": version.split("-")[0].split("+")[0],
                     "CFBundleShortVersionString": version.split("-")[0].split("+")[0],
                     "CFBundlePackageType": "APPL", "NSHighResolutionCapable": True}
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps(plist))
            sign_app(app, require_signing)
            os.symlink("/Applications", staging / "Applications")
            archive = out / f"{stem}.dmg"
            run("hdiutil", "create", "-volname", "Vellio", "-srcfolder", staging,
                "-format", "UDZO", "-ov", archive)
        else:
            app = staging / "Vellio"
            app.mkdir()
            name = "Vellio.exe" if os_name == "windows" else "Vellio"
            shutil.copy2(binary, app / name)
            shutil.copy2(engines[0], app / engine_name)
            copy_licenses(app / "licenses")
            (app / "README.txt").write_text(
                "Vellio — https://github.com/ohmycloud/vellio\n\n"
                "Extract the complete package. Quit the old application before replacing it.\n"
                "Keep the executable and the PDFium library together. Run Vellio (Vellio.exe on Windows).\n"
                "Your document library lives separately; do not delete or replace that folder.\n"
                "Linux requires a desktop session with Vulkan/GPU drivers and system X11/Wayland libraries.\n",
                encoding="utf-8")
            if os_name == "windows":
                sign_windows(app / name)
                archive = out / f"{stem}.zip"
                with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as stream:
                    for path in sorted(app.rglob("*")):
                        if path.is_file():
                            stream.write(path, path.relative_to(staging))
            else:
                # Write target Unix permissions into the archive directly.
                # Windows chmod/stat cannot represent executable mode bits.
                def linux_permissions(member):
                    if member.isdir() or member.name == "Vellio/Vellio":
                        member.mode = 0o755
                    elif member.isfile():
                        member.mode = 0o644
                    return member

                archive = out / f"{stem}.tar.gz"
                with tarfile.open(archive, "w:gz") as stream:
                    stream.add(app, arcname="Vellio", filter=linux_permissions)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(archive)
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--pdfium-dir", type=Path, default=ROOT / "target/pdfium-wheel")
    parser.add_argument("--out", type=Path, default=ROOT / "target/release-assets")
    parser.add_argument("--require-signing", action="store_true")
    options = parser.parse_args()
    package(options.target, options.version, options.pdfium_dir.resolve(), options.out.resolve(), options.require_signing)
