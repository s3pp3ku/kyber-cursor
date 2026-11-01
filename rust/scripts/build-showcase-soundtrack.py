#!/usr/bin/env python3
"""Build a 45-second All-mode showcase mix from the selected local clips.

The same centered soundtrack serves both halves of the Python/Rust comparison.
No sound is synthesized, fetched, or sent to a device.
"""
from __future__ import annotations

import array
import hashlib
import json
import math
import struct
import sys
import wave
from pathlib import Path

RATE = 44_100
SECONDS = 45
SCENE = 2.5


def read_clip(path: Path) -> list[float]:
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getcomptype()) != (1, 2, RATE, "NONE"):
            raise ValueError(f"Unsupported clip format: {path}")
        raw = wav.readframes(wav.getnframes())
    return [sample / 32768.0 for (sample,) in struct.iter_unpack("<h", raw)]


def hum_loop(samples: list[float], length: int) -> list[float]:
    edge = min(RATE * 3 // 10, len(samples) // 10)
    body = samples[edge:len(samples) - edge]
    overlap = max(2, min(RATE * 8 // 100, len(body) // 4))
    out: list[float] = []
    cursor = 0
    for i in range(length):
        if cursor >= len(body) - overlap:
            offset = cursor - (len(body) - overlap)
            fraction = offset / (overlap - 1)
            value = body[cursor] * (1.0 - fraction) + body[offset] * fraction
        else:
            value = body[cursor]
        cursor += 1
        if cursor >= len(body):
            cursor = overlap
        value *= min(1.0, i / (RATE * 0.15))
        if i >= length - RATE // 50:
            release = i - (length - RATE // 50)
            value *= 0.5 + 0.5 * math.cos(math.pi * release / (RATE // 50))
        out.append(value)
    return out


def add(dst: list[float], src: list[float], at: int, gain: float = 1.0) -> None:
    start = max(0, at)
    source_start = max(0, -at)
    count = min(len(dst) - start, len(src) - source_start)
    for i in range(max(0, count)):
        dst[start + i] += src[source_start + i] * gain


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    clips = root / "assets/sounds"
    out = Path(sys.argv[1]) if len(sys.argv) == 2 else root / "test-artifacts/kyber-showcase-python-vs-rust-recorded-45s"
    out.mkdir(parents=True, exist_ok=True)
    count = RATE * SECONDS
    mix = [0.0] * count
    names = {
        "ignite": ["lightsaber-ignite.wav", "sith-lightsaber-1.wav", "lightsaber-ignite-burst.wav"],
        "off": ["lightsaber-off.wav", "sith-deactivate-1.wav"],
        "hum": ["lightsaber-hum.wav", "lightsaber-hummm.wav"],
        "swipe": ["lightsaber-whoosh-01.wav", "lightsaber-whoosh-04.wav"],
        "clash": ["lightsaber-clash-02.wav", "lightsaber-clash-04.wav"],
    }
    loaded = {role: [read_clip(clips / role / name) for name in items] for role, items in names.items()}
    sections = []
    for scene in range(18):
        base = round(scene * SCENE * RATE)
        ignite_i = scene % len(names["ignite"])
        off_i = (scene // 2) % len(names["off"])
        hum_i = (scene // 3) % len(names["hum"])
        swipe_i = scene % len(names["swipe"])
        ignite = loaded["ignite"][ignite_i]
        off = loaded["off"][off_i]
        hum = hum_loop(loaded["hum"][hum_i], int(RATE * 1.12))
        add(mix, ignite, base + int(0.08 * RATE), 0.78)
        add(mix, hum, base + int(0.42 * RATE), 0.74)
        add(mix, off, base + int(1.72 * RATE), 0.78)
        if scene % 3 == 1:
            swipe = loaded["swipe"][swipe_i]
            add(mix, swipe, base + int(1.02 * RATE), 0.52)
        if scene % 6 == 4:
            clash = loaded["clash"][scene % len(names["clash"])]
            add(mix, clash, base + int(1.34 * RATE), 0.55)
        sections.append({
            "scene": scene,
            "start_seconds": round(scene * SCENE, 3),
            "ignite": names["ignite"][ignite_i],
            "hum": names["hum"][hum_i],
            "off": names["off"][off_i],
            "swipe": names["swipe"][swipe_i] if scene % 3 == 1 else None,
            "clash": names["clash"][scene % len(names["clash"])] if scene % 6 == 4 else None,
        })

    peak = max(abs(x) for x in mix)
    gain = min(1.0, 0.88 / peak) if peak else 1.0
    fade = RATE // 20
    for i in range(count):
        end_gain = min(1.0, (count - i - 1) / fade)
        mix[i] *= gain * end_gain
    pcm = array.array("h", (max(-32768, min(32767, round(x * 32767))) for x in mix))
    if sys.byteorder != "little":
        pcm.byteswap()
    wav_path = out / "showcase-audio.wav"
    with wave.open(str(wav_path), "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        stereo = array.array("h", (sample for pair in zip(pcm, pcm) for sample in pair))
        if sys.byteorder != "little":
            stereo.byteswap()
        wav.writeframes(stereo.tobytes())

    manifest = json.loads((clips / "manifest.json").read_text())
    provenance = {
        "duration_seconds": SECONDS,
        "sample_rate": RATE,
        "channels": 2,
        "mode": "All: power-up, held hum, power-down, movement swipes and occasional clashes",
        "mix_gain": gain,
        "sound_pack_id": manifest["id"],
        "same_centered_audio_for_both_comparison_halves": True,
        "sections": sections,
        "source_clips": [
            {"file": item["file"], "sha256": item["sha256"]}
            for item in manifest["clips"]
        ],
        "wav_sha256": hashlib.sha256(wav_path.read_bytes()).hexdigest(),
    }
    (out / "showcase-audio.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"Wrote {wav_path} ({SECONDS}s; All-mode recorded clips)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
