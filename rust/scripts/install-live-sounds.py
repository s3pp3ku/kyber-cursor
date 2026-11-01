#!/usr/bin/env python3
"""Install the staged swipe/click sound update with a saved rollback."""
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = Path.home() / ".local/share/kyber-cursor/rust"
BACKUPS = Path.home() / ".local/share/kyber-cursor/backups"
SOURCES = {
    "kyber-ignite": ROOT / "rust/target/release/kyber-ignite",
    "kyber-native.so": ROOT / "native-cursor/build/kyber-native-color-trial.so",
}
EXPECTED_OLD = {
    # This is the original helper restored after the earlier live attempt.
    "kyber-ignite": "bbdf755068f21d0c82b9315dd4808dc08c1486a398fbdd72d1cccf58ef134b8f",
    "kyber-native.so": "f9699a564718e057118a2281088c43a08d84b0772022892f59729b66ad35f61d",
}
SERVICE = "kyber-ignite.service"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def active():
    subprocess.run(["systemctl", "--user", "is-active", "--quiet", SERVICE], check=True)


def replace(source, target, stamp):
    temporary = target.with_name(f".{target.name}.new-{stamp}")
    shutil.copy2(source, temporary)
    os.replace(temporary, target)


def wait_for_plugin():
    """systemd's simple service can be active before its plugin finishes loading."""
    deadline = time.monotonic() + 12
    last_status = ""
    while time.monotonic() < deadline:
        result = subprocess.run(["hyprctl", "kybernative", "status"], text=True,
                                capture_output=True, timeout=3)
        last_status = (result.stdout + result.stderr).strip()
        if result.returncode == 0 and "protocol=2" in result.stdout and "enabled=yes" in result.stdout:
            return result.stdout.strip()
        time.sleep(0.25)
    raise RuntimeError(f"Native plugin did not become ready within 12 seconds: {last_status}")


def main():
    if os.environ.get("KYBER_SOUND_UPDATE_APPROVED") != "yes":
        raise SystemExit("Set KYBER_SOUND_UPDATE_APPROVED=yes only after approving the live plugin reload.")
    mode = Path.home() / ".config/kyber-cursor/sound-mode"
    if mode.read_text().strip() != "all":
        raise SystemExit("Sound mode changed since review; no files changed.")
    active()
    for name, source in SOURCES.items():
        if not source.is_file():
            raise SystemExit(f"Missing staged build: {source}")
        target = RUNTIME / name
        if digest(target) != EXPECTED_OLD[name]:
            raise SystemExit(f"Installed {name} changed since review; no files changed.")

    stamp = str(time.time_ns())
    backup = BACKUPS / f"live-sounds-{stamp}"
    backup.mkdir(parents=True, exist_ok=False)
    hashes = {}
    for name in SOURCES:
        target = RUNTIME / name
        saved = backup / name
        shutil.copy2(target, saved)
        hashes[name] = {"previous_sha256": digest(saved), "replacement_sha256": digest(SOURCES[name])}
    (backup / "manifest.json").write_text(json.dumps({"files": hashes}, indent=2) + "\n")

    try:
        for name, source in SOURCES.items():
            replace(source, RUNTIME / name, stamp)
        subprocess.run(["systemctl", "--user", "restart", SERVICE], check=True, timeout=30)
        active()
        status = wait_for_plugin()
    except Exception:
        for name in SOURCES:
            replace(backup / name, RUNTIME / name, f"rollback-{stamp}")
        subprocess.run(["systemctl", "--user", "restart", SERVICE], check=False, timeout=30)
        raise

    print("Installed staged Kyber sound update.")
    print(f"Rollback binaries: {backup}")
    print(status)


if __name__ == "__main__":
    main()
