#!/usr/bin/env python3
"""Build one complete lightsaber sound from the owner's ElevenLabs takes.

Power-up, held hum, a swing, a clash, and power-down, assembled into a single
continuous file so it can be judged as one sound rather than as separate
clips.  The individual pieces come from mix-elevenlabs-pack.py.

    build-lightsaber-demo.py OUTPUT.wav [--takes DIR]
"""
import argparse
import importlib.util
import pathlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", help="WAV file to write")
parser.add_argument(
    "--takes",
    default=str(pathlib.Path.home() / "Downloads" / "Sounds for Lightsaber"),
    help="folder with the owner's ElevenLabs takes")
args = parser.parse_args()
out_path = pathlib.Path(args.output)

spec = importlib.util.spec_from_file_location(
    "mixer",
    str(pathlib.Path(__file__).with_name("mix-elevenlabs-pack.py")))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

takes = m.Takes(args.takes)
rate = m.RATE

# ---- the pieces ----------------------------------------------------------
hum = m.fit(m.blade_bed(takes, 13.0), -12.0, -2.5)
ignite = m.ignite(takes, hum)
power_down = m.power_down(takes, hum, blade_hz=92.0, number=4)
swing = m.fit(m.swipe(takes, 3), -16.0, -3.0)
clash = m.fit(m.clash(takes, 4), -17.0, -4.0)

# ---- the timeline (seconds) ----------------------------------------------
IGNITE_AT = 0.00
BED_AT = 1.55          # the hum takes over as the ignition settles
SWING_AT = 2.55
CLASH_AT = 3.75
OFF_AT = 6.05
BED_END = OFF_AT + 0.05   # the hum stops as the power-down takes over
TOTAL = OFF_AT + len(power_down) / rate

buf = [0.0] * int(TOTAL * rate)
m.add_at(buf, ignite, int(IGNITE_AT * rate))


def duck_gain(t, at, depth, hold=0.06, recover=0.40):
    """Dip the hum on a hit, then recover. Never dips before the hit."""
    if t < at:
        return 1.0
    dt = t - at
    if dt < hold:
        return depth
    if dt < hold + recover:
        k = (dt - hold) / recover
        return depth + (1.0 - depth) * (k * k * (3.0 - 2.0 * k))
    return 1.0


# The ignition ends with hum[0.83:0.98] fading out, so the bed picks up at
# that same point in the hum and fades in over the same 150 ms.  Same signal
# and matching linear curves mean the two sum back to the hum itself.
bed_len = BED_END - BED_AT
bed = m.trim(hum, 0.83, 0.83 + bed_len)
bed = m.fade(bed, 0.15, 0.05, linear=True)
for i in range(len(bed)):
    t = BED_AT + i / rate
    bed[i] *= min(duck_gain(t, SWING_AT, 0.55),
                  duck_gain(t, CLASH_AT, 0.45))
m.add_at(buf, bed, int(BED_AT * rate))

m.add_at(buf, swing, int(SWING_AT * rate))
m.add_at(buf, clash, int(CLASH_AT * rate))
m.add_at(buf, power_down, int(OFF_AT * rate))

m.save_wav(out_path, buf, peak_db=-1.0)
print("wrote %s  %.2f s" % (out_path, len(buf) / rate))
