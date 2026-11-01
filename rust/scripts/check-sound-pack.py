#!/usr/bin/env python3
"""Verify a built Kyber sound pack against the approved reference.

Checks two things:

  * each role's spectrum against the pack that was in place before, measured in
    the time domain so a loud transient at the start of a clip is not lost;
  * that the designed loop really loops: exact length, a seam no worse than an
    ordinary sample step, and a sample-identical second pass.

Standard library plus ffmpeg.  Exit code 1 if any check fails.

    check-sound-pack.py PACK_DIR [--reference DIR]
"""
import argparse
import math
import re
import subprocess
import sys
import wave
from pathlib import Path

BANDS = [(31.5, 63), (63, 125), (125, 250), (250, 500), (500, 1000),
         (1000, 2000), (2000, 4000), (4000, 8000), (8000, 16000)]

# role, our file, their file, first band to compare, check absolute level?
#
# The ignition and the power-down both hand over to the held hum, so they are
# anchored to it rather than to the old pack.  The ignition therefore carries
# the hum's bass (bands 0-2) and is louder overall; the power-down starts at the
# hum's level, which makes its RMS lower than the old file's.  Both are checked
# above that, and only for shape.
ROLES = [
    ("hum", "kyber-hum.wav", "hum/lightsaber-hum.wav", 0, True),
    ("ignite", "kyber-ignite.wav", "ignite/lightsaber-ignite.wav", 3, False),
    ("off", "kyber-off.wav", "off/lightsaber-off.wav", 0, False),
    ("clash", "kyber-clash-01.wav", "clash/lightsaber-clash-02.wav", 0, True),
    ("swipe", "kyber-swipe-01.wav", "swipe/lightsaber-whoosh-01.wav", 0, True),
]


def band_rms(path, lo, hi):
    err = subprocess.run(
        ["ffmpeg", "-hide_banner", "-i", str(path), "-af",
         "highpass=f=%g,lowpass=f=%g,astats" % (lo, hi), "-f", "null", "-"],
        check=True, capture_output=True).stderr.decode()
    match = re.search(r"RMS level dB: (-?[\d.]+)", err)
    return float(match.group(1)) if match else float("nan")


def bands(path):
    return [band_rms(path, lo, hi) for lo, hi in BANDS]


def read_wav(path):
    with wave.open(str(path), "rb") as wav:
        if wav.getnchannels() != 1 or wav.getsampwidth() != 2:
            raise SystemExit("%s is not mono PCM16" % path)
        if wav.getframerate() != 44100:
            raise SystemExit("%s is not 44100 Hz" % path)
        raw = wav.readframes(wav.getnframes())
    return [int.from_bytes(raw[i:i + 2], "little", signed=True) / 32768.0
            for i in range(0, len(raw), 2)]


def check_roles(pack, reference):
    print("Spectrum, relative to the approved pack")
    print("  %-8s %s  worst" % ("role", " ".join("%6d" % lo for lo, hi in BANDS)))
    failed = 0
    for role, ours, theirs, first_band, check_level in ROLES:
        new_path = pack / role / ours
        ref_path = reference / theirs
        if not new_path.exists():
            print("  %-8s MISSING %s" % (role, new_path))
            failed += 1
            continue
        if not ref_path.exists():
            print("  %-8s no reference at %s" % (role, ref_path))
            continue
        got, want = bands(new_path), bands(ref_path)
        delta = [g - w for g, w in zip(got, want)]
        # shape only, above the bands that deliberately hand over to the hum
        kept = delta[first_band:]
        offset = sum(kept) / len(kept)
        shape = [d - offset for d in kept]
        worst = max(abs(d) for d in shape)
        ok = worst <= 8.0
        if ok and check_level and abs(offset) > 4.0:
            ok = False
        failed += 0 if ok else 1
        print("  %-8s %s %6.1f  %s" % (
            role,
            " ".join("%+6.1f" % d for d in delta),
            worst,
            "ok" if ok else
            ("FAIL (level %.1f dB)" % offset if check_level
             else "FAIL (shape)")))
    return failed


def check_loop(pack):
    loop = pack / "loop" / "hum-loop-10s.wav"
    print("\nDesigned loop")
    if not loop.exists():
        print("  no loop/ directory; skipped")
        return 0
    samples = read_wav(loop)
    failed = 0
    seconds = len(samples) / 44100.0
    exact = abs(seconds - 10.0) < 0.0005
    print("  length          %.4f s  %s" % (seconds, "ok" if exact else "FAIL"))
    failed += 0 if exact else 1

    seam = abs(samples[0] - samples[-1])
    steps = sorted(abs(samples[i + 1] - samples[i])
                   for i in range(len(samples) - 1))
    typical = steps[int(len(steps) * 0.999)]
    ok = seam <= typical
    print("  seam jump       %.5f (ordinary step %.5f)  %s"
          % (seam, typical, "ok" if ok else "FAIL"))
    failed += 0 if ok else 1

    twice = pack / "loop" / "hum-loop-10s-x3.wav"
    if twice.exists():
        three = read_wav(twice)
        period = len(samples)
        if len(three) >= 2 * period:
            diff = max(abs(three[i] - three[i + period]) for i in range(period))
            ok = diff == 0.0
            print("  second pass     max difference %.6f  %s"
                  % (diff, "ok" if ok else "FAIL"))
            failed += 0 if ok else 1
    return failed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pack", help="folder written by mix-elevenlabs-pack.py")
    parser.add_argument("--reference", default="assets/sounds",
                        help="folder with the pack that was in place before")
    args = parser.parse_args()
    pack = Path(args.pack)
    reference = Path(args.reference)
    if not pack.is_dir():
        raise SystemExit("no such folder: " + str(pack))
    failed = check_roles(pack, reference) + check_loop(pack)
    print()
    if failed:
        print("%d check(s) failed" % failed)
        sys.exit(1)
    print("All checks passed.")


if __name__ == "__main__":
    main()
