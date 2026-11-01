#!/usr/bin/env python3
"""Package original layered hum auditions; no reference clips or settings read."""
import argparse
import array
import base64
import hashlib
import json
import shutil
import subprocess
import sys
import wave
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read_pcm(path, frames):
    with wave.open(str(path), "rb") as audio:
        if (audio.getnchannels(), audio.getsampwidth(), audio.getframerate(), audio.getnframes()) != (1, 2, 44100, frames):
            raise ValueError(f"Wrong format or length: {path}")
        values = array.array("h", audio.readframes(audio.getnframes()))
        if sys.byteorder != "little":
            values.byteswap()
    if max(map(abs, values)) >= 32767 or abs(values[0]) > 1 or abs(values[-1]) > 1:
        raise ValueError(f"Clipping or invalid fade endpoints: {path}")
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--exported", action="store_true", help="package an existing hum_layers export")
    args = parser.parse_args()
    out = args.output.resolve()
    if not args.exported:
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([
            args.cargo, "run", "--offline", "--locked", "--release", "-p", "kyber-core",
            "--example", "hum_layers", "--", str(out),
        ], cwd=ROOT / "rust", check=True)
    if (out / "index.html").exists():
        raise ValueError("Audition already packaged; use a new output directory")
    manifest = json.loads((out / "layers.json").read_text())
    if len(manifest["layers"]) != 4 or manifest["version"] != 3:
        raise ValueError("Expected the four-source hum_layers version 3 export")
    frames = 44100 * manifest["seconds"]
    layers, embedded = [], {}
    for layer in manifest["layers"]:
        path = out / layer["file"]
        layers.append(read_pcm(path, frames))
        layer["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        embedded[layer["id"]] = base64.b64encode(path.read_bytes()).decode()
    mix = read_pcm(out / manifest["mix"]["file"], frames)
    # Quantization of four stems can differ from quantization of their float
    # sum by a few PCM units; catches wrong files or misaligned exports.
    residual = max(abs(sum(values[i] for values in layers) - mix[i]) for i in range(frames))
    if residual > 4:
        raise ValueError(f"Exported layers do not reproduce full mix: {residual} PCM units")
    manifest["quantized_stem_sum_max_error"] = residual
    manifest["mix"]["sha256"] = hashlib.sha256((out / manifest["mix"]["file"]).read_bytes()).hexdigest()
    (out / "layers.json").write_text(json.dumps(manifest, indent=2) + "\n")
    template = (ROOT / "rust/scripts/hum-layers.html").read_text()
    (out / "index.html").write_text(template
        .replace("/* LAYER_MANIFEST */{}", json.dumps(manifest))
        .replace("/* LAYER_AUDIO */{}", json.dumps(embedded)))
    shutil.copyfile(ROOT / "LICENSE", out / "LICENSE")
    (out / "READ-ME.txt").write_text(
        "Kyber layered hum workbench, revision 3\n\n"
        "Open index.html and press Play full hum. Four independent sound sources\n"
        "play together. Listen alone reveals each; sliders and checkboxes adjust\n"
        "their balance while playing. Full mix returns from a solo to the blend.\n"
        "Save mix downloads a small settings JSON for the next design revision.\n"
        "Choose your local reference MP3s to compare; files stay on your computer.\n"
        "Reference level is matched to the starting mix.\n\n"
        "HTML embeds all original stems; copy this one file between Windows,\n"
        "Mac and Linux to audition. Native cursor integration is separate.\n"
        "full-hum.wav is the initial blend; other WAVs are aligned source stems.\n"
        "Mono PCM16, 44.1 kHz, 12 seconds. No reference recordings are packaged.\n"
        "Original synth sources: motor / electrical rasp / lower throb / electrical crackle.\n"
        "Each has independent motion and filtering, with short housing reflections.\n"
        "No live audio settings, services or binaries were changed.\n"
    )
    source = ROOT / "rust/crates/kyber-core/examples/hum_layers.rs"
    (out / "provenance.json").write_text(json.dumps({
        "generator": str(source.relative_to(ROOT)),
        "generator_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "license": "MIT", "reference_audio_included": False,
        "runtime_changed": False,
        "brief": "Moving air replaced by saturated current hum and dense resonant discharges; other three source levels preserved",
    }, indent=2) + "\n")
    archive = out.with_suffix(".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(out.iterdir()):
            if path.is_file():
                zipped.write(path, out.name + "/" + path.name)
    archive.with_suffix(".zip.sha256").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
    print(f"Open {out / 'index.html'}")
    print(f"Portable archive: {archive}")
    print(f"Four aligned sources reproduce the full mix within {residual} PCM16 units.")


if __name__ == "__main__":
    main()
