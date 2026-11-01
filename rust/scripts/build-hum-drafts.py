#!/usr/bin/env python3
"""Build portable original hum auditions. Reads no reference audio/settings."""
import argparse
import array
import base64
import hashlib
import json
import math
import shutil
import subprocess
import sys
import wave
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new output directory")
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    out = args.output.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([
        args.cargo, "run", "--offline", "--locked", "--release", "-p", "kyber-core",
        "--example", "hum_drafts", "--", str(out),
    ], cwd=ROOT / "rust", check=True)
    designs = json.loads((out / "designs.json").read_text())
    data = {}
    for design in designs:
        path = out / design["file"]
        with wave.open(str(path), "rb") as wav:
            if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) != (1, 2, 44100, 44100 * 9):
                raise ValueError(f"Invalid audition format/duration: {path}")
            samples = array.array("h", wav.readframes(wav.getnframes()))
            if sys.byteorder != "little":
                samples.byteswap()
        body = samples[8820:-8820]
        rms = math.sqrt(sum(value * value for value in body) / len(body)) / 32767
        peak = max(map(abs, samples)) / 32767
        if not .215 < rms < .225 or peak > .851 or abs(samples[0]) > 1 or abs(samples[-1]) > 1:
            raise ValueError(f"Level/headroom/edge check failed: {path}: RMS {rms}, peak {peak}")
        design.update(sha256=hashlib.sha256(path.read_bytes()).hexdigest(), rms=rms, peak=peak)
        data[design["file"]] = "data:audio/wav;base64," + base64.b64encode(path.read_bytes()).decode()
    template = (ROOT / "rust/scripts/hum-drafts.html").read_text()
    (out / "index.html").write_text(template.replace("/* DRAFT_AUDIO */{}", json.dumps(data)))
    (out / "designs.json").write_text(json.dumps(designs, indent=2) + "\n")
    shutil.copyfile(ROOT / "LICENSE", out / "LICENSE")
    (out / "READ-ME.txt").write_text(
        "Kyber hum design, first drafts\n\n"
        "Open index.html in a browser; all three original auditions are embedded.\n"
        "Copy that single file between Windows, Mac and Linux to compare.\n"
        "Separate WAVs use mono 16-bit PCM at 44.1 kHz; no Python/Rust needed to play.\n"
        "Choose 1, 2 or 3 as a direction, then describe what needs changing.\n"
        "Optional: select local reference files in the page to compare; they stay local.\n"
        "The drafts have matched sustained RMS levels and gentle start/end fades.\n"
        "These are original additive oscillators and generated filtered noise.\n"
        "No reference waveform or recording was used by the generator or included.\n"
        "This is an audition, not an installed runtime change or final loop pack.\n"
        "Code/provenance: rust/crates/kyber-core/examples/hum_drafts.rs\n"
    )
    (out / "provenance.json").write_text(json.dumps({
        "generator": "rust/crates/kyber-core/examples/hum_drafts.rs",
        "generator_sha256": hashlib.sha256((ROOT / "rust/crates/kyber-core/examples/hum_drafts.rs").read_bytes()).hexdigest(),
        "license": "MIT", "reference_audio_included": False,
        "installed_runtime_changed": False,
        "brief": "Low primary tones near 90-92 Hz, lower body layer, different harmonic balances; new phases, drift and noise",
    }, indent=2) + "\n")
    archive = out.with_suffix(".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(out.iterdir()):
            if path.is_file():
                zipped.write(path, out.name + "/" + path.name)
    archive.with_suffix(".zip.sha256").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
    print(f"Open {out / 'index.html'}")
    print(f"Portable archive: {archive}")
    print("Three original drafts: matched RMS, PCM16 format, duration, headroom and fades checked.")


if __name__ == "__main__":
    main()
