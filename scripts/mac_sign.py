#!/usr/bin/env python3
"""Sign Vellio using an isolated keychain, following EasyRead's mac_sign.cjs.

Secrets: MAC_SIGN_P12 (base64), MAC_SIGN_P12_PASSWORD, MAC_SIGN_IDENTITY.
Optional notarization: APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD, APPLE_TEAM_ID.
No private key or certificate is stored in the repository/system keychain.
"""
import base64
import os
from pathlib import Path
import secrets
import shlex
import subprocess
import tempfile


def run(*args):
    try:
        subprocess.run([str(arg) for arg in args], check=True)
    except subprocess.CalledProcessError as error:
        # Never include command arguments: security/notarytool receive passwords.
        raise RuntimeError(f"{args[0]} failed (exit {error.returncode})") from None


def seal(app, identity, keychain=None):
    args = ["--force", "--sign", identity]
    if keychain:
        args += ["--keychain", str(keychain)]
    if identity.startswith("Developer ID Application:"):
        args += ["--options", "runtime", "--timestamp"]
    # Sign the bundled engine before sealing the outer bundle.
    run("codesign", *args, app / "Contents/MacOS/libpdfium.dylib")
    run("codesign", *args, app)
    run("codesign", "--verify", "--deep", "--strict", app)
    run("codesign", "-d", "-r-", app)


def sign_app(app, require_signing=False):
    p12 = os.environ.get("MAC_SIGN_P12")
    if not p12:
        if require_signing:
            raise RuntimeError("MAC_SIGN_P12 is required for release builds; configure GitHub Secrets")
        seal(app, "-")
        return
    previous = shlex.split(subprocess.check_output(
        ["security", "list-keychains", "-d", "user"], text=True))
    with tempfile.TemporaryDirectory(prefix="vellio-sign-") as folder:
        folder = Path(folder)
        cert, keychain = folder / "cert.p12", folder / "sign.keychain-db"
        cert.write_bytes(base64.b64decode(p12, validate=True))
        cert.chmod(0o600)
        password = secrets.token_urlsafe(24)
        created = False
        try:
            run("security", "create-keychain", "-p", password, keychain)
            created = True
            run("security", "set-keychain-settings", keychain)
            run("security", "unlock-keychain", "-p", password, keychain)
            run("security", "list-keychains", "-d", "user", "-s", keychain, *previous)
            run("security", "import", cert, "-k", keychain, "-P",
                os.environ.get("MAC_SIGN_P12_PASSWORD", ""), "-T", "/usr/bin/codesign")
            run("security", "set-key-partition-list", "-S", "apple-tool:,apple:",
                "-s", "-k", password, keychain)
            seal(app, os.environ.get("MAC_SIGN_IDENTITY") or "Vellio Self-Signed", keychain)
        finally:
            try:
                run("security", "list-keychains", "-d", "user", "-s", *previous)
            finally:
                if created:
                    run("security", "delete-keychain", keychain)
    credentials = [os.environ.get(name) for name in
                   ["APPLE_ID", "APPLE_APP_SPECIFIC_PASSWORD", "APPLE_TEAM_ID"]]
    if any(credentials):
        if not all(credentials):
            raise RuntimeError("Notarization requires all three Apple credentials")
        with tempfile.TemporaryDirectory(prefix="vellio-notary-") as folder:
            archive = Path(folder) / "Vellio.zip"
            run("ditto", "-c", "-k", "--keepParent", app, archive)
            run("xcrun", "notarytool", "submit", archive, "--wait", "--apple-id",
                credentials[0], "--password", credentials[1], "--team-id", credentials[2])
            run("xcrun", "stapler", "staple", app)
            run("xcrun", "stapler", "validate", app)
