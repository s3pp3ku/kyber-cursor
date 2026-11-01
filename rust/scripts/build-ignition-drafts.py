#!/usr/bin/env python3
"""Original ignition auditions joined to the hash-pinned approved hold hum."""
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
SR = 44100
SECONDS = 6
ONSET = int(.15*SR)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, SR):
            raise ValueError(f"Expected mono PCM16 / 44.1 kHz: {path}")
        values = array.array("h", wav.readframes(wav.getnframes()))
        if sys.byteorder != "little":
            values.byteswap()
    return [value/32767 for value in values]


def write(path, values):
    if not values or any(not math.isfinite(value) or abs(value) > .951 for value in values):
        raise ValueError(f"Invalid samples or inadequate headroom: {path}")
    if abs(values[0]) > 1/32767 or abs(values[-1]) > 1/32767:
        raise ValueError(f"Invalid edge fade: {path}")
    pcm = array.array("h", (int(value*32767) for value in values))
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SR)
        wav.writeframes(pcm.tobytes())


def smooth(value):
    u = max(0, min(1, value))
    return u*u*(3-2*u)


def rising_hum(hum, frame, duration, start_ratio):
    """Accelerate to the original PCM position and speed, without a splice.

    The read position integrates speed = start + (1-start)*smooth(t/duration),
    anchored to the unmodified hold at duration. Linear interpolation only
    slows existing original audio; no reference recordings are read.
    """
    t = frame/SR
    if t >= duration:
        return hum[frame]
    u = t/duration
    remaining = .5-u+u**3-.5*u**4
    position = (t+(1-start_ratio)*duration*remaining)*SR
    index = int(position)
    fraction = position-index
    pitched = hum[index]*(1-fraction)+hum[index+1]*fraction
    level = smooth(t/.10)*(.22+.78*smooth(u))
    return pitched*level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--exported", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    if not args.exported:
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([
            args.cargo, "run", "--offline", "--locked", "--release", "-p", "kyber-core",
            "--example", "ignition_drafts", "--", str(out),
        ], cwd=ROOT/"rust", check=True)
    if (out/"index.html").exists():
        raise ValueError("Already packaged: use a new output directory")
    approval = json.loads((ROOT/"rust/audio-design/hold-hum.json").read_text())
    approved = ROOT/approval["approved_audio"]["file"]
    if sha(approved) != approval["approved_audio"]["sha256"]:
        raise ValueError("Approved hold reference hash changed; stop rather than substituting another hum")
    hum = read(approved)
    if len(hum) < SR*SECONDS:
        raise ValueError("Approved hum is shorter than the audition")
    designs = json.loads((out/"ignitions.json").read_text())
    effects = {design["id"]: read(out/design["file"]) for design in designs}
    cycles = {}
    baseline = [0.0]*(SR*SECONDS)
    for frame in range(ONSET, len(baseline)):
        baseline[frame] = hum[frame-ONSET]*smooth((SECONDS-frame/SR)/.35)
    for design in designs:
        effect = effects[design["id"]]
        cycle = [0.0]*len(baseline)
        for frame in range(ONSET, len(cycle)):
            position = frame-ONSET
            t = position/SR
            accent = effect[position] if position < len(effect) else 0
            body = (rising_hum(hum, position, design["rise_seconds"], design["hum_pitch_start_ratio"])
                    if t < design["rise_seconds"] else baseline[frame])
            cycle[frame] = accent + body
        cycles[design["id"]] = cycle
    peak = max(abs(value) for cycle in cycles.values() for value in cycle)
    common_gain = min(1, .90/max(peak, 1e-12))
    # Keep the accepted held sound at its exact original level. If a future
    # accent is too loud, revise that accent rather than attenuating the hold.
    if common_gain != 1:
        raise ValueError("Accent exceeds available headroom; reduce it and rebuild in a new directory")
    assets = {}
    for design in designs:
        cycle = [value*common_gain for value in cycles[design["id"]]]
        filename = design["id"]+"-into-hum.wav"
        write(out/filename, cycle)
        # The short audition includes the pitched full blend, not just its
        # extra texture. Its final 120 ms fade is only a clip boundary.
        short = cycle[ONSET:ONSET+int(design["file_seconds"]*SR)]
        short = [value*smooth((len(short)-1-frame)/(SR*.12))
                 for frame, value in enumerate(short)]
        accent_filename = design["id"]+"-rise.wav"
        write(out/accent_filename, short)
        design["texture_sha256"] = sha(out/design["file"])
        design["accent_file"] = accent_filename
        design["cycle_file"] = filename
        design["accent_sha256"] = sha(out/accent_filename)
        design["cycle_sha256"] = sha(out/filename)
        design["cycle_peak"] = max(map(abs, cycle))
        assets[design["id"]+"-accent"] = base64.b64encode((out/accent_filename).read_bytes()).decode()
        assets[design["id"]+"-cycle"] = base64.b64encode((out/filename).read_bytes()).decode()
    reference = [value*common_gain for value in baseline]
    write(out/"approved-hum-reference.wav", reference)
    assets["hum"] = base64.b64encode((out/"approved-hum-reference.wav").read_bytes()).decode()
    # The settled sections must equal the same approved hum sample for sample.
    settled = slice(SR*2, SR*5)
    for design in designs:
        rendered = read(out/design["cycle_file"])
        direct = read(out/"approved-hum-reference.wav")
        if rendered[settled] != direct[settled]:
            raise ValueError("Settled hold audio differs from the approved reference")
    manifest = {
        "format": "kyber-ignition-audition", "version": 2,
        "designs": designs, "preview_seconds": SECONDS,
        "onset_seconds": ONSET/SR, "common_preview_gain": common_gain,
        "approved_hum": approval["approved_audio"],
        "settled_hold_identical_seconds": [2, 5],
        "preview_end": "350 ms fade ends the finite audition; shutdown design remains open",
        "direction": "continuous upward pitch and growing energy; power-down must take the opposite direction",
        "runtime_changed": False,
    }
    (out/"ignitions.json").write_text(json.dumps(manifest, indent=2)+"\n")
    template = (ROOT/"rust/scripts/ignition-drafts.html").read_text()
    (out/"index.html").write_text(template
        .replace("/* IGNITION_MANIFEST */{}", json.dumps(manifest))
        .replace("/* IGNITION_AUDIO */{}", json.dumps(assets)))
    shutil.copyfile(ROOT/"LICENSE", out/"LICENSE")
    sources = ["rust/crates/kyber-core/examples/ignition_drafts.rs", "rust/scripts/build-ignition-drafts.py", "rust/scripts/ignition-drafts.html"]
    (out/"provenance.json").write_text(json.dumps({
        "sources": {name: sha(ROOT/name) for name in sources},
        "approved_hum_sha256": sha(approved),
        "reference_recordings_included": False, "license": "MIT",
    }, indent=2)+"\n")
    (out/"READ-ME.txt").write_text(
        "Kyber rising power-up, revision 2 (not yet approved)\n\n"
        "Open index.html. One focused draft plays as a short rise or into the\n"
        "approved four-layer hold hum. Every sequence lasts six seconds.\n"
        "The final gentle fade ends the preview; it is not a shutdown design.\n"
        "Choose local ignition reference MP3s to compare. They stay on the computer.\n"
        "HTML embeds every original preview. Copy this one file to Windows, Mac\n"
        "or Linux; no Python/Rust, server, network or installation needed to listen.\n"
        "Other files are mono PCM16 / 44.1 kHz WAVs, metadata and MIT license.\n"
        "The approved original hum accelerates from 0.38x to its normal pitch\n"
        "over 0.9 seconds. Original rising electrical currents accompany it.\n"
        "The short clip has its own ending fade, not a shutdown design.\n"
        "No reference recordings, impact sparks or strike bodies are included.\n"
        "Approved hold source hash is pinned. Its settled section is identical\n"
        "in every audition after a common headroom gain; no hold retuning.\n"
        "Live cursor settings, services and binaries are unchanged.\n"
    )
    archive = out.with_suffix(".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(out.iterdir()):
            if path.is_file():
                zipped.write(path, out.name+"/"+path.name)
    archive.with_suffix(".zip.sha256").write_text(sha(archive)+"  "+archive.name+"\n")
    print(f"Open {out/'index.html'}")
    print(f"Portable archive: {archive}")
    print(f"Common headroom gain {common_gain:.6f}; settled hold bytes match at 2–5 s for all options.")


if __name__ == "__main__":
    main()
