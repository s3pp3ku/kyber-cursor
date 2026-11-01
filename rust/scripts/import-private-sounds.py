#!/usr/bin/env python3
"""Import the owner's selected local clips into the private default pack.

No network access. ffmpeg is a maintainer tool, never a playback dependency.
All files use the same conservative gain, with no synthesis or pitch changes.
"""
import argparse
import array
import hashlib
import json
import math
import subprocess
import sys
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RATE = 44100
GAIN = .65
FILES = {
    "ignite": ("lightsaber-ignite", "lightsaber-ignite-burst", "sith-lightsaber-1"),
    "off": ("lightsaber-off", "sith-deactivate-1"),
    "clash": ("lightsaber-clash-02", "lightsaber-clash-04"),
    "swipe": ("lightsaber-whoosh-01", "lightsaber-whoosh-04"),
    "hum": ("lightsaber-hum", "lightsaber-hummm"),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    out = ROOT / "assets/sounds"
    manifest = {
        "id": "kyber-private-clips-v1", "version": 1,
        "scope": "Owner-authorized private, noncommercial packages",
        "authorization_date": "2026-10-17",
        "rights": "Third-party recordings; ownership and redistribution license unverified. Not covered by the code's MIT license.",
        "encoding": {"sample_rate": RATE, "channels": 1, "bits": 16, "format": "PCM WAV"},
        "preparation": "Decoded to mono 44100 Hz PCM; common gain 0.65; no synthesis, pitch shifting or added effects. Hum looping uses a playback crossfade.",
        "clips": [],
    }
    for role, names in FILES.items():
        (out / role).mkdir(parents=True, exist_ok=True)
        for name in names:
            source = args.source / (name + ".mp3")
            raw = subprocess.run([
                "ffmpeg", "-v", "error", "-i", str(source), "-f", "f32le",
                "-ac", "1", "-ar", str(RATE), "pipe:1",
            ], check=True, capture_output=True).stdout
            values = array.array("f", raw)
            if sys.byteorder != "little":
                values.byteswap()
            if not values or any(not math.isfinite(v) or abs(v*GAIN) >= .95 for v in values):
                raise ValueError("Invalid audio or insufficient headroom: " + str(source))
            samples = array.array("h", (round(v * GAIN * 32767) for v in values))
            if sys.byteorder != "little":
                samples.byteswap()
            target = out / role / (name + ".wav")
            with wave.open(str(target), "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(RATE)
                wav.writeframes(samples.tobytes())
            manifest["clips"].append({
                "role": role, "file": target.relative_to(out).as_posix(),
                "source_filename": source.name, "source_sha256": digest(source),
                "sha256": digest(target), "seconds": len(values)/RATE,
                "gain": GAIN,
            })
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Imported {len(manifest['clips'])} local clips into {out}; originals unchanged.")


if __name__ == "__main__":
    main()
