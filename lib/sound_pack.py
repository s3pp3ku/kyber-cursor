#!/usr/bin/env python3
"""Copy the selected recordings for the Omarchy installer and CLI."""
import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path

PACK = Path(__file__).resolve().parents[1] / "assets/sounds"
LEGACY = {
    "ignite": ("ignite-1.wav", "ignite-2.wav", "ignite-3.wav"),
    "off": ("off-1.wav", "off-2.wav"),
    "clash": ("clash-1.wav", "clash-2.wav", "clash-3.wav", "2-clash-1.wav", "2-clash-2.wav"),
}


def install(output):
    manifest = json.loads((PACK / "manifest.json").read_text())
    for clip in manifest["clips"]:
        path = PACK / clip["file"]
        if hashlib.sha256(path.read_bytes()).hexdigest() != clip["sha256"]:
            raise ValueError("Pack file hash mismatch: " + str(path))
    # Retire only Kyber's reserved generated-default filenames, preserving them
    # in a dated backup instead of mixing them with the replacement recordings.
    backup = output.parent / "backups" / ("generated-sounds-" + str(time.time_ns()))
    for role, names in LEGACY.items():
        for name in names:
            old = output / role / name
            if old.is_file():
                (backup / role).mkdir(parents=True, exist_ok=True)
                shutil.move(str(old), str(backup / role / name))
    for clip in manifest["clips"]:
        source = PACK / clip["file"]
        target = output / clip["file"]
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            shutil.copyfile(source, target)
    shutil.copyfile(PACK / "manifest.json", output / "manifest.json")
    shutil.copyfile(PACK / "README.md", output / "AUDIO-NOTICE.md")
    print("Default sounds: selected recording pack (power-up, off, clash, swipe, hum).")
    if backup.exists():
        print("Previous generated sounds saved in " + str(backup))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    install(parser.parse_args().output)
