#!/usr/bin/env python3
"""Build a self-contained, offline listening comparison and portable WAV pack.

Uses the original checked-in Python generator and current Rust core. Does not
read personal sounds/settings, play audio, or install anything. Python/Cargo
are build tools only: recipients just open index.html or play the WAV files.
"""
import argparse
import array
import base64
import hashlib
import importlib.util
import json
import math
import shutil
import subprocess
import sys
import wave
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SR = 44100
KINDS = ("ignite", "off", "clash")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_wav(path):
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, SR):
            raise ValueError(f"Expected mono PCM16 at {SR} Hz: {path}")
        data = array.array("h", wav.readframes(wav.getnframes()))
        if sys.byteorder != "little":
            data.byteswap()
        return [sample / 32767 for sample in data]


def write_wav(path, samples):
    if any(not math.isfinite(sample) or abs(sample) > 1 for sample in samples):
        raise ValueError(f"Non-finite or clipped comparison mix: {path}")
    data = array.array("h", (int(sample * 32767) for sample in samples))
    if sys.byteorder != "little":
        data.byteswap()
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SR)
        wav.writeframes(data.tobytes())


def mix_cycle(ignite, off, hum=None):
    """Same 5 s hold, 300 ms ignition threshold and release for every version."""
    cycle = [0.0] * (SR * 6)
    for start, track in ((int(SR * .3), ignite), (SR * 5, off)):
        for index, sample in enumerate(track):
            if start + index < len(cycle):
                cycle[start + index] += sample
    if hum is not None:
        # Mirrors BladeSource: 1 s delay from ignition, 300 ms fade-in,
        # 20 ms cosine release; the oscillator starts after the silent delay.
        start, release, tail = int(SR * 1.3), SR * 5, SR // 50
        for index in range(start, release + tail):
            position = index - start
            gain = min(1.0, position / (SR * .3))
            if index >= release:
                gain *= .5 + .5 * math.cos(math.pi * (index - release) / tail)
            cycle[index] += hum[position] * gain
    return cycle


def manifest(directory, generator, sources):
    clips = []
    for kind in KINDS:
        for path in sorted((directory / kind).glob("*.wav")):
            samples = read_wav(path)
            clips.append({
                "file": path.relative_to(directory).as_posix(),
                "sha256": sha256(path), "frames": len(samples),
                "peak": max(map(abs, samples)),
            })
    if len(clips) != 10:
        raise ValueError(f"Expected ten generated default clips in {directory}")
    result = {
        "format": "kyber-generated-sounds", "version": 1,
        "generator": generator, "license": "MIT",
        "encoding": {"type": "PCM", "sample_rate": SR, "channels": 1, "bits": 16},
        "sources": sources, "clips": clips,
    }
    (directory / "pack.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def archive(directory, destination):
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as output:
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                output.write(path, path.relative_to(directory.parent).as_posix())
    destination.with_suffix(destination.suffix + ".sha256").write_text(
        f"{sha256(destination)}  {destination.name}\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new comparison directory")
    parser.add_argument("--cargo", default="cargo", help="Cargo executable")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    python_pack, rust_pack = out / "python-original", out / "rust-current"
    source = ROOT / "lib/make_sounds.py"
    spec = importlib.util.spec_from_file_location("kyber_original_sounds", source)
    original = importlib.util.module_from_spec(spec)
    # Avoid creating a bytecode cache beside the source being compared.
    sys.dont_write_bytecode = True
    spec.loader.exec_module(original)
    original.main(python_pack)
    subprocess.run([
        args.cargo, "run", "--offline", "--locked", "--release", "-p", "kyber-core",
        "--example", "sound_review", "--", str(rust_pack),
    ], cwd=ROOT / "rust", check=True)

    sources = {str(path.relative_to(ROOT)): sha256(path) for path in (
        source, ROOT / "rust/crates/kyber-core/src/sounds.rs",
        ROOT / "rust/crates/kyber-core/src/blade_audio.rs",
        ROOT / "rust/crates/kyber-ignite/src/blade_source.rs",
    )}
    python_manifest = manifest(python_pack, "lib/make_sounds.py (unchanged)", sources)
    manifest(rust_pack, "kyber-core::sounds (current source)", sources)
    shutil.copyfile(ROOT / "LICENSE", python_pack / "LICENSE")
    shutil.copyfile(source, python_pack / "generate.py")
    (python_pack / "READ-ME.txt").write_text(
        "Kyber original Python-generated sounds (MIT)\n\n"
        "Ten original synthesized WAV files; no sampled recordings.\n"
        "Play on Windows, macOS or Linux. No Python/Rust needed to play them.\n"
        "Keep ignite, off and clash together inside your Kyber sounds folder.\n"
        "Linux: ~/.config/kyber-cursor/sounds (or XDG_CONFIG_HOME equivalent)\n"
        "Windows preview: %APPDATA%\\kyber-cursor\\sounds\n"
        "Future Mac runtime settings: ~/Library/Application Support/kyber-cursor/sounds\n"
        "The Mac cursor/audio runtime is not implemented yet.\n\n"
        "Compare before installing. Extra files in an existing sound folder are\n"
        "also eligible for playback; merging packs does not isolate this pack.\n"
        "pack.json records per-file hashes and source hashes.\n"
        "generate.py recreates the WAVs using the Python standard library.\n"
    )

    listening = out / "listening"
    listening.mkdir()
    python_hum = original.hum(SR * 6, lambda _t: 86.0)
    rust_hum = read_wav(rust_pack / "hum-86.wav")
    write_wav(listening / "python-hum-ingredient.wav", python_hum)
    cycles = {
        "python-cycle": mix_cycle(
            read_wav(python_pack / "ignite/ignite-2.wav"),
            read_wav(python_pack / "off/off-2.wav")),
        "rust-cycle": mix_cycle(
            read_wav(rust_pack / "ignite/ignite-2.wav"),
            read_wav(rust_pack / "off/off-2.wav"), rust_hum),
        "python-with-hum": mix_cycle(
            read_wav(python_pack / "ignite/ignite-2.wav"),
            read_wav(python_pack / "off/off-2.wav"), python_hum),
    }
    # Preserve the relative levels. Use one common headroom factor for all
    # full cycles, never independently normalize the versions being compared.
    peak = max(abs(sample) for cycle in cycles.values() for sample in cycle)
    cycle_gain = min(1.0, .95 / peak)
    for name, samples in cycles.items():
        write_wav(listening / f"{name}.wav", [sample * cycle_gain for sample in samples])

    data = {
        path.relative_to(out).as_posix(): "data:audio/wav;base64," + base64.b64encode(path.read_bytes()).decode()
        for path in sorted(out.rglob("*.wav"))
    }
    template = (ROOT / "rust/scripts/sound-review.html").read_text()
    (out / "index.html").write_text(template.replace("/* AUDIO_DATA */{}", json.dumps(data)))
    (out / "review.json").write_text(json.dumps({
        "sources": sources,
        "reference": "Original generated Python defaults, not personal sound files",
        "runtime_model": "5 s hold; ignition at 0.3 s; release at 5 s; hum delay 1 s and fade-in 0.3 s",
        "cycle_gain": cycle_gain,
        "rust_hum_seed": "HoldingHum::new(86, 44100); live Linux chooses a fresh seed per hold",
        "python_with_hum": "Proposed combination for audition, not original Python listener behavior",
        "limitations": "Generated-source comparison; not a capture of the installed runtime or native OS audio output",
        "original_clip_count": len(python_manifest["clips"]),
    }, indent=2) + "\n")
    (out / "READ-ME.txt").write_text(
        "Open index.html in a browser. All audio is embedded; no server or internet.\n"
        "Start with the three full sequences, then compare the individual sounds.\n"
        "Copy index.html alone to another computer, or copy this whole folder.\n"
        "WAV files and the Python pack are also available separately.\n"
        "This compares generated sources; the desktop also has personal clips.\n"
        "No live settings or installed files were changed.\n"
    )
    archive(python_pack, out / "kyber-python-original-sounds.zip")
    archive(out, out.with_suffix(".zip"))
    print(f"Open {out / 'index.html'}")
    print(f"Original pack: {out / 'kyber-python-original-sounds.zip'}")
    print(f"Portable comparison: {out.with_suffix('.zip')}")


if __name__ == "__main__":
    main()
