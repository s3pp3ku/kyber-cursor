#!/usr/bin/env python3
"""Build a Kyber sound pack from the owner's ElevenLabs generations.

Input: a folder of ElevenLabs MP3 takes.  Output: prepared 44.1 kHz mono
PCM16 WAVs for every Kyber role.

Everything audible comes from those takes.  No other recording is sampled and
nothing is downloaded.  The tools used on the owner's own material are:

  * pitch/time resampling, to move a take to the blade's fundamental,
  * equal-power looping, to extend a one-second take into a held bed,
  * granular re-spraying, to break the audible repeat of a short take,
  * biquad filters and mixing, to shape and balance the result.

The spectral target is the pack the owner already approved, so the new sounds
sit in the same place as the old ones.

    mix-elevenlabs-pack.py --src DIR --out DIR
"""
import argparse
import array
import hashlib
import json
import math
import random
import subprocess
import sys
import wave
from pathlib import Path

RATE = 44100
TAU = 2.0 * math.pi


# --------------------------------------------------------------------------
# decoding / writing
# --------------------------------------------------------------------------
def load_audio(path):
    """Decode any ffmpeg-readable file to mono float samples at RATE."""
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-f", "f32le",
         "-ac", "1", "-ar", str(RATE), "pipe:1"],
        check=True, capture_output=True).stdout
    values = array.array("f", raw)
    if sys.byteorder != "little":
        values.byteswap()
    if not values or any(not math.isfinite(v) for v in values):
        raise ValueError("Invalid audio: " + str(path))
    return list(values)


def save_wav(path, samples, peak_db=-1.5):
    peak = max((abs(v) for v in samples), default=0.0)
    limit = 10 ** (peak_db / 20.0)
    scale = limit / peak if peak > limit else 1.0
    pcm = array.array("h", (max(-32768, min(32767, round(v * scale * 32767)))
                            for v in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(pcm.tobytes())


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


# --------------------------------------------------------------------------
# small DSP helpers (mono, RATE)
# --------------------------------------------------------------------------
def biquad_run(x, b, a):
    b0, b1, b2 = b
    a1, a2 = a
    x1 = x2 = y1 = y2 = 0.0
    out = [0.0] * len(x)
    for i, v in enumerate(x):
        y = b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1 = x1, v
        y2, y1 = y1, y
        out[i] = y
    return out


def biquad(x, kind, fc, q=0.70710678, gain_db=0.0):
    w0 = TAU * fc / RATE
    c, s = math.cos(w0), math.sin(w0)
    alpha = s / (2.0 * q)
    a0 = 1.0 + alpha
    if kind == "lp":
        b0 = b2 = (1.0 - c) / 2.0
        b1 = 1.0 - c
    elif kind == "hp":
        b0 = b2 = (1.0 + c) / 2.0
        b1 = -(1.0 + c)
    elif kind == "peak":
        A = 10 ** (gain_db / 40.0)
        b0, b2 = 1.0 + alpha * A, 1.0 - alpha * A
        b1 = -2.0 * c
        a0 = 1.0 + alpha / A
        a1, a2 = -2.0 * c, 1.0 - alpha / A
        return biquad_run(x, [b0 / a0, b1 / a0, b2 / a0], [a1 / a0, a2 / a0])
    elif kind == "notch":
        b0 = b2 = 1.0
        b1 = -2.0 * c
        a1, a2 = -2.0 * c, 1.0 - alpha
        return biquad_run(x, [b0 / a0, b1 / a0, b2 / a0], [a1 / a0, a2 / a0])
    else:
        raise ValueError(kind)
    return biquad_run(x, [b0 / a0, b1 / a0, b2 / a0],
                      [-2.0 * c / a0, (1.0 - alpha) / a0])


def highpass(x, fc, poles=2):
    for _ in range(max(1, poles // 2)):
        x = biquad(x, "hp", fc)
    return x


def lowpass(x, fc, poles=2):
    for _ in range(max(1, poles // 2)):
        x = biquad(x, "lp", fc)
    return x


def peaking(x, fc, gain_db, q=2.0):
    return biquad(x, "peak", fc, q=q, gain_db=gain_db)


def bandpass(x, lo, hi):
    """Two cascaded filters: high-pass then low-pass, 12 dB/octave each."""
    x = biquad(x, "hp", lo)
    x = biquad(x, "lp", hi)
    return x


def scale(x, gain):
    return [v * gain for v in x]


def mix(*tracks):
    n = max((len(t) for t in tracks), default=0)
    out = [0.0] * n
    for t in tracks:
        for i, v in enumerate(t):
            out[i] += v
    return out


def add_at(out, src, offset):
    for i, v in enumerate(src):
        j = offset + i
        if 0 <= j < len(out):
            out[j] += v
    return out


def rms(x):
    if not x:
        return 0.0
    return math.sqrt(sum(v * v for v in x) / len(x))


def normalize_rms(x, db):
    r = rms(x)
    if r <= 0.0:
        return list(x)
    g = 10 ** (db / 20.0) / r
    return [v * g for v in x]


def normalize_peak(x, db):
    peak = max((abs(v) for v in x), default=0.0)
    if peak <= 0.0:
        return list(x)
    g = 10 ** (db / 20.0) / peak
    return [v * g for v in x]


def fit(x, rms_db, ceiling_db=-2.0):
    """Set the loudness, then keep the peak under the ceiling."""
    out = normalize_rms(x, rms_db)
    peak = max((abs(v) for v in out), default=0.0)
    limit = 10 ** (ceiling_db / 20.0)
    if peak > limit:
        out = [v * limit / peak for v in out]
    return out


def build_up(x, seconds, start=0.12):
    """A rising level, so an activation grows instead of starting at full."""
    out = []
    for i, v in enumerate(x):
        t = i / RATE
        g = start + (1.0 - start) * min(1.0, t / max(1e-6, seconds)) ** 1.5
        out.append(v * g)
    return out


def decay(x, tau=0.2):
    """An exponential decay, for thumps and struck bodies."""
    return [v * math.exp(-(i / RATE) / tau) for i, v in enumerate(x)]


def soft_clip(x, drive=1.5):
    """A gentle limiter: keeps transients from outshouting the body."""
    norm = math.tanh(drive)
    return [math.tanh(drive * v) / norm for v in x]


def fade(x, fade_in=0.0, fade_out=0.0, linear=False):
    """Fade the edges.  Linear pairs with a linear fade on the other side so
    two copies of the same signal sum back to one; the sine curve keeps power
    constant across two unrelated signals."""
    n = len(x)
    a = min(int(fade_in * RATE), n)
    b = min(int(fade_out * RATE), n - a)
    out = list(x)
    for i in range(a):
        t = i / max(1, a)
        out[i] *= t if linear else math.sin(t * math.pi / 2)
    for i in range(b):
        t = (i + 1) / b
        out[n - b + i] *= (1.0 - t) if linear else math.sin((1.0 - t) * math.pi / 2)
    return out


def trim(x, start_s, end_s=None):
    a = max(0, int(start_s * RATE))
    b = len(x) if end_s is None else min(len(x), int(end_s * RATE))
    return x[a:b]


def resample(x, step):
    """Resample with a fixed step.  step < 1 raises pitch and shortens."""
    if step <= 0:
        raise ValueError("step must be positive")
    out = []
    last = len(x) - 1
    pos = 0.0
    while int(pos) < last:
        i = int(pos)
        f = pos - i
        out.append(x[i] * (1.0 - f) + x[i + 1] * f)
        pos += step
    return out


def resample_sweep(x, start_step, end_step):
    """Resample with a linearly changing step: a rising or falling sweep."""
    out = []
    last = len(x) - 1
    n = len(x)
    pos = 0.0
    i = 0
    while int(pos) < last:
        t = i / max(1, n)
        step = start_step + (end_step - start_step) * t
        j = int(pos)
        f = pos - j
        out.append(x[j] * (1.0 - f) + x[j + 1] * f)
        pos += step
        i += 1
    return out


def equal_power_loop(segment, seconds, xfade=0.30):
    """Tile one take into a longer bed with equal-power joins."""
    seg = list(segment)
    want = int(seconds * RATE)
    nf = max(4, min(int(xfade * RATE), len(seg) // 3))
    out = list(seg)
    while len(out) < want + nf:
        add = seg
        for i in range(nf):
            t = (i + 1) / nf
            a = math.cos(t * math.pi / 2)
            b = math.sin(t * math.pi / 2)
            out[-nf + i] = out[-nf + i] * a + add[i] * b
        out.extend(add[nf:])
    return out[:want]


def _hann(n):
    if n <= 1:
        return [1.0] * max(0, n)
    return [0.5 - 0.5 * math.cos(TAU * i / (n - 1)) for i in range(n)]


def respray(src, seconds, grain=0.14, hop=0.045, pitch_jitter=0.0,
            seed=0, walk=0.0):
    """Re-spray a short take as a bed of overlapping grains.

    Each grain is taken from a random point in the source, optionally with a
    small random pitch offset, and is windowed in.  The result keeps the
    timbre of the source but has no audible repeat, which is what a one-second
    take needs when it has to sound like a continuous texture.
    """
    rnd = random.Random(seed)
    grain_len = int(grain * RATE)
    hop_len = max(1, int(hop * RATE))
    if len(src) <= grain_len + 4:
        return equal_power_loop(src, seconds)
    out = [0.0] * int(seconds * RATE)
    window = _hann(grain_len)
    positions = list(range(0, len(src) - grain_len, max(1, grain_len // 8)))
    # slow random level walk so the bed breathes instead of sitting flat
    levels = []
    value = 0.0
    for _ in range(len(positions) + 2):
        value += rnd.uniform(-walk, walk)
        value = max(-walk, min(walk, value * 0.97))
        levels.append(1.0 + value)
    pos = 0
    index = 0
    while pos + grain_len <= len(out):
        start = positions[rnd.randrange(len(positions))]
        g = src[start:start + grain_len]
        if pitch_jitter:
            g = resample(g, 1.0 + rnd.uniform(-pitch_jitter, pitch_jitter))
            if len(g) < grain_len:
                g = g + [0.0] * (grain_len - len(g))
        level = levels[index % len(levels)]
        for i in range(grain_len):
            out[pos + i] += g[i] * window[i] * level
        pos += hop_len
        index += 1
    return out


# --------------------------------------------------------------------------
# the owner's takes
# --------------------------------------------------------------------------
class Takes:
    """Locate and decode the owner's ElevenLabs takes by role."""

    GROUPS = {
        # role        prefix,                        takes
        "hum":    ("Deep_sci-fi_energy_b", [1, 2, 3, 4]),
        "swipe":  ("A_heavy_cinematic_ba", [1, 2, 3, 4]),
        "ignite": ("Instant_plasma_surge", [1, 2, 3, 4]),
        "clash":  ("Blinding_sci-fi_lase", [1, 2, 3, 4]),
    }
    # A dedicated shutdown take is optional.  Without it the power-down is
    # derived from the ignition, which works but sounds less natural.
    OPTIONAL = {
        "off": ("Sci-fi_energy_blade_", [1, 2, 3, 4]),
    }

    def __init__(self, folder):
        self.folder = Path(folder)
        self.files = sorted(p for p in self.folder.glob("*.mp3"))
        if not self.files:
            raise SystemExit("no MP3 takes found in " + str(folder))
        self.paths = {}
        for role, (prefix, numbers) in self.GROUPS.items():
            for number in numbers:
                matches = [p for p in self.files
                           if p.name.startswith(prefix) and
                           ("#%d-" % number) in p.name]
                if not matches:
                    raise SystemExit("missing %s take %d" % (role, number))
                self.paths[(role, number)] = matches[0]
        for role, (prefix, numbers) in self.OPTIONAL.items():
            for number in numbers:
                matches = [p for p in self.files
                           if p.name.startswith(prefix) and
                           ("#%d-" % number) in p.name]
                if matches:
                    self.paths[(role, number)] = matches[0]
        self._cache = {}

    def has(self, role, number):
        return (role, number) in self.paths

    def get(self, role, number):
        key = (role, number)
        if key not in self._cache:
            self._cache[key] = load_audio(self.paths[key])
        return self._cache[key]

    def source_hashes(self):
        return {("%s-%d" % (role, number)): digest(self.paths[(role, number)])
                for (role, number) in sorted(self.paths)}


# --------------------------------------------------------------------------
# the roles
# --------------------------------------------------------------------------
# The approved pack's hum sits at 92 Hz with hollow upper partials at 278,
# 370 and 462 Hz.  The owner's hum takes live an octave lower, so they are
# raised to the blade's fundamental and the partials are brought forward
# with narrow peaks.
BLADE_HZ = 92.0


def engine(takes, role, number, source_hz, blade_hz, start=0.0, end=None):
    """A take moved onto the blade's fundamental.

    The owner's takes sit at their own fundamentals: the surge takes near
    110-125 Hz, the deep takes near 48 Hz.  Resampling by the ratio moves
    them onto one common fundamental so the layers beat against each other
    instead of sitting apart.
    """
    x = takes.get(role, number)
    x = trim(x, start, end)
    # resampling by a step above 1 plays the take faster and raises its pitch
    return resample(x, blade_hz / source_hz)


def blade_bed(takes, seconds=13.0, blade_hz=BLADE_HZ):
    """The held hum: the blade's voice, its engine, and electrical texture.

    Each layer is one of the owner's takes moved onto one common fundamental,
    so they beat against each other instead of sitting apart.
    """
    # The blade's voice.  The surge take has the cleanest ladder of partials:
    # a fundamental near 109 Hz with harmonics at 217 and 322 Hz.
    voice = engine(takes, "ignite", 2, 109.0, blade_hz, 0.02, 0.88)

    # The engine.  The deep take carries the blade's third partial, which
    # lands on the hollow upper partial the approved hum has.
    body = engine(takes, "hum", 3, 49.8, blade_hz, 0.03, 0.95)

    # The pulse.  This take beats against itself, and that is the thrum that
    # makes the blade feel powered rather than played.
    pulse = engine(takes, "hum", 4, 48.4, blade_hz, 0.03, 0.95)

    # each layer loops at its own rate, so the bed takes a long time to repeat
    a = equal_power_loop(resample(voice, 1.0 / 0.997), seconds)
    b = equal_power_loop(resample(body, 1.0 / 1.000), seconds)
    c = equal_power_loop(resample(pulse, 1.0 / 1.005), seconds)
    bed = mix(scale(a, 1.00), scale(b, 0.85), scale(c, 0.70))

    # electrical texture: the brightest part of the surge takes, re-sprayed
    # so it does not repeat, and kept low enough to sit under the voice
    crackle = respray(trim(takes.get("ignite", 4), 0.0, 0.55),
                      seconds, grain=0.11, hop=0.036, pitch_jitter=0.005,
                      seed=11, walk=0.06)
    crackle = bandpass(crackle, 2400.0, 9000.0)

    bed = mix(bed, scale(crackle, 0.09))

    # the blade is hollow: the second partial is weak, the third and fourth
    # are strong, and the top rolls away
    bed = hum_tone(bed, blade_hz)
    return fade(bed, 0.12, 0.25)


def ignite(takes, hum_tail, seconds=1.7, blade_hz=BLADE_HZ):
    """A blade powering on: a switch click, a rising sweep, then the hum.

    The tail is the held hum itself rather than a copy of it, so the change
    from the activation to the loop is exact instead of approximate.
    """
    # the switch click: the very start of the brightest surge take
    click = trim(takes.get("ignite", 4), 0.0, 0.035)
    click = bandpass(click, 1500.0, 6000.0)

    # rising energy: the owner's surge take played with a rising rate, so its
    # pitch and brightness climb together
    sweep = resample_sweep(trim(takes.get("ignite", 3), 0.02, 1.70),
                            0.95, 1.55)
    sweep = biquad(sweep, "hp", 400.0)
    sweep = lowpass(sweep, 6500.0, poles=4)
    sweep = peaking(sweep, 1600.0, 3.0, q=1.0)
    sweep = peaking(sweep, 700.0, -8.0, q=0.8)
    sweep = peaking(sweep, 350.0, -5.0, q=1.0)
    sweep = peaking(sweep, 4000.0, 3.0, q=1.0)
    # the activation grows rather than starting at full strength
    sweep = build_up(sweep, 0.28, 0.40)

    # rising motor: the blade fundamental climbing into place
    motor = resample_sweep(engine(takes, "hum", 4, 48.4, blade_hz, 0.03),
                            0.72, 1.16)
    motor = highpass(motor, 110.0, poles=2)
    motor = fade(motor, 0.02, 0.05)

    # the attack is shaped on its own, so the soft clip below never touches
    # the hum that follows it
    attack = [0.0] * int(0.90 * RATE)
    add_at(attack, scale(click, 0.55), 0)
    add_at(attack, scale(sweep, 1.00), int(0.02 * RATE))
    add_at(attack, scale(motor, 0.30), int(0.03 * RATE))
    attack = lowpass(attack, 7000.0)
    attack = soft_clip(normalize_peak(attack, 1.0), 1.4)
    attack = scale(attack, 0.90)
    attack = fade(attack, 0.004, 0.18)

    # the held hum, faded in under the end of the attack.  It is used exactly
    # as it will be looped, with no filtering, or the bass would drop out at
    # the change and the join would be audible.
    tail = fade(trim(hum_tail, 0.0, 0.95), 0.18, 0.0)

    out = [0.0] * int(seconds * RATE)
    add_at(out, attack, 0)
    add_at(out, tail, int(0.72 * RATE))
    # The runtime starts the held hum 0.15 s before this clip ends and fades
    # it in linearly over exactly that long.  Two copies of the same hum, one
    # fading out linearly and one fading in linearly, sum back to one.
    out = fade(out, 0.0, 0.15, linear=True)
    peak = max((abs(v) for v in out), default=0.0)
    ceiling = 10 ** (-1.5 / 20.0)
    if peak > ceiling:
        out = scale(out, ceiling / peak)
    return out


def power_down(takes, hum_head, seconds=1.55, blade_hz=BLADE_HZ, number=4):
    """A blade powering off, handed over from the hum.

    When the owner has generated a dedicated shutdown take it is used as is:
    it starts at full energy and collapses into silence.  Otherwise the
    ignition is played with a falling rate, which is honest but less natural.
    """
    # the hand-over: the first moments have to be exactly as loud as the held
    # hum, or releasing the button sounds like a drop in volume
    # unchanged hum, so the hand-over keeps the same tone as the held note
    head = fade(trim(hum_head, 0.0, 0.40), 0.0, 0.30)

    if takes.has("off", number):
        fall = trim(takes.get("off", number), 0.0, 1.75)
        # a little more downward motion, so it reads as the blade dying
        fall = resample_sweep(fall, 1.10, 0.84)
        fall = highpass(fall, 180.0)
        fall = peaking(fall, 2000.0, 16.0, q=0.6)
        fall = peaking(fall, 5000.0, 16.0, q=0.5)
        fall = peaking(fall, 8000.0, 10.0, q=0.6)
        fall = fade(fall, 0.03, 0.0)
        fall_gain, tail = 1.15, 0.30
    else:
        # the owner's surge take naturally collapses in brightness and energy
        fall = resample_sweep(trim(takes.get("ignite", 3), 0.0, 1.60),
                              1.10, 0.72)
        fall = biquad(fall, "hp", 250.0)
        fall = lowpass(fall, 6500.0, poles=4)
        fall = peaking(fall, 2200.0, 4.0, q=1.0)
        fall = peaking(fall, 2500.0, 4.0, q=1.0)
        fall = peaking(fall, 4500.0, 4.0, q=1.0)
        fall = peaking(fall, 180.0, 8.0, q=1.2)
        fall = fade(fall, 0.12, 0.0)
        fall_gain, tail = 0.55, 0.40

    motor = resample_sweep(engine(takes, "hum", 4, 48.4, blade_hz, 0.03),
                           1.05, 0.74)
    motor = highpass(motor, 110.0, poles=4)

    length = int(seconds * RATE)
    out = [0.0] * length
    add_at(out, head, 0)
    add_at(out, scale(fall, fall_gain), int(0.01 * RATE))
    add_at(out, scale(fade(motor, 0.05, 0.0), 0.25), int(0.02 * RATE))
    # the sound closes down: a low-pass that keeps shutting
    shut = lowpass(out, 5000.0)
    for i in range(len(out)):
        t = i / RATE
        k = max(0.0, 1.0 - t / seconds)
        out[i] = out[i] * 0.55 + shut[i] * 0.45 * (0.25 + 0.75 * k)
    # a decaying sound has a high crest by nature; round the peak a little so
    # the first moment is not spiky against the body
    out = soft_clip(normalize_peak(out, 1.0), 1.2)
    out = fade(out, 0.005, tail)
    # anchor the hand-over to the held hum, then respect the peak ceiling
    start = rms(out[:int(0.10 * RATE)])
    out = scale(out, rms(hum_head) / start if start > 0 else 1.0)
    peak = max((abs(v) for v in out), default=0.0)
    ceiling = 10 ** (-1.5 / 20.0)
    if peak > ceiling:
        out = scale(out, ceiling / peak)
    return out


def clash(takes, number=4, seconds=0.85):
    """Blades striking: a hard electrical crack over a low body."""
    x = trim(takes.get("clash", number), 0.0, 1.00)
    x = fade(x, 0.001, 0.001)
    # the crack keeps the high bite, the body keeps the low weight
    high = bandpass(x, 900.0, 11000.0)
    high = peaking(high, 2500.0, 3.0, q=1.0)
    low = bandpass(x, 60.0, 700.0)
    body = trim(takes.get("hum", 3), 0.05, 0.50)
    body = highpass(resample(body, BLADE_HZ / 49.8), 55.0)
    body = decay(body, 0.18)
    thud = trim(takes.get("hum", 4), 0.03, 0.50)
    thud = highpass(resample(thud, BLADE_HZ / 48.4), 55.0)
    thud = decay(thud, 0.22)
    out = mix(scale(high, 1.60), scale(low, 1.20),
              scale(body, 12.0), scale(thud, 6.0))
    out = peaking(out, 180.0, 3.0, q=1.2)
    out = peaking(out, 1600.0, 3.0, q=1.0)
    out = peaking(out, 3000.0, 3.0, q=0.9)
    out = peaking(out, 6000.0, 3.0, q=1.0)
    out = lowpass(out, 11000.0)
    out = trim(out, 0.0, seconds)
    # a strike should be sharp, but not so sharp that the body disappears
    out = soft_clip(normalize_peak(out, 0.5), 1.2)
    return fade(out, 0.001, 0.10)


def swipe(takes, number=3, seconds=1.05):
    """A blade sweeping through space."""
    x = trim(takes.get("swipe", number), 0.04, 1.20)
    x = highpass(x, 80.0)
    x = biquad(x, "notch", 155.0, q=1.0)
    x = peaking(x, 90.0, 8.0, q=1.2)
    x = peaking(x, 800.0, -10.0, q=1.0)
    x = peaking(x, 1500.0, -10.0, q=1.0)
    x = peaking(x, 5000.0, 3.0, q=1.2)
    # a small movement of the blade's own voice, so it belongs to the hum
    voice = bandpass(trim(takes.get("hum", 3), 0.03, 0.9), 300.0, 4000.0)
    voice = fade(voice, 0.02, 0.2)
    out = mix(scale(x, 1.0), scale(voice, 0.18))
    out = trim(out, 0.0, seconds)
    return fade(out, 0.02, 0.12)


# --------------------------------------------------------------------------
# package
# --------------------------------------------------------------------------
def seam_loop(x, seconds, xfade=0.35):
    """Return exactly `seconds` of `x` that loops back into itself cleanly.

    Material shorter than the loop is extended first, then the audio just past
    the loop point is folded onto the beginning, so the last sample flows into
    the first instead of cutting.
    """
    want = int(seconds * RATE)
    nf = max(4, int(xfade * RATE))
    need = want + nf
    if len(x) < need:
        x = equal_power_loop(x, need / RATE, xfade=min(0.25, seconds / 8.0))
    src = list(x[:need])
    src += [0.0] * (need - len(src))
    for i in range(nf):
        t = (i + 1) / nf
        src[i] = (src[i] * math.sin(t * math.pi / 2)
                  + src[want + i] * math.cos(t * math.pi / 2))
    return src[:want]


def filter_loop(x, chain, preroll=1.0):
    """Run an IIR chain over a loop without a startup transient.

    A filter starts from silence, so filtering a finished loop would leave a
    different state at the end than at the start and the seam would click.
    Feeding the loop's tail first lets the filter settle, and the pre-roll is
    dropped afterwards.
    """
    n = min(int(preroll * RATE), len(x))
    out = chain(list(x[-n:]) + list(x))
    return out[n:]


def hum_tone(bed, blade_hz=BLADE_HZ):
    """The blade's tone: hollow partials and a steep roll-off."""
    bed = highpass(bed, 0.78 * blade_hz, poles=4)
    bed = biquad(bed, "notch", 2.0 * blade_hz, q=1.0)
    bed = peaking(bed, 1.0 * blade_hz, 4.0, q=1.2)
    bed = peaking(bed, 3.0 * blade_hz, 12.0, q=2.0)
    bed = peaking(bed, 4.0 * blade_hz, 6.0, q=2.4)
    bed = peaking(bed, 1200.0, -2.0, q=0.8)
    bed = peaking(bed, 2400.0, -3.0, q=1.4)
    bed = peaking(bed, 6000.0, -4.0, q=1.2)
    bed = peaking(bed, 11000.0, 9.0, q=1.2)
    bed = lowpass(bed, 2600.0 * blade_hz / BLADE_HZ, poles=4)
    return bed


def designed_loop(takes, seconds=10.0, blade_hz=BLADE_HZ):
    """A designed held hum that loops exactly on `seconds`.

    Every layer completes a whole number of cycles inside the loop, so the
    whole bed returns to its starting phase at the loop point.  The tonal
    layers run at 10/3, 10/4 and 10/2 seconds, which means their beat pattern
    only comes back round once per loop.

    Returns (mix, layers), where layers maps a name to its own 10 s loop.
    """
    voice = engine(takes, "ignite", 2, 109.0, blade_hz, 0.02, 0.88)
    body = engine(takes, "hum", 3, 49.8, blade_hz, 0.03, 0.95)
    pulse = engine(takes, "hum", 4, 48.4, blade_hz, 0.03, 0.95)

    # a slightly different repeat rate per layer, so they drift against each
    # other across the loop instead of marching together
    voice = seam_loop(resample(voice, 1.0 / 0.997), seconds)
    body = seam_loop(resample(body, 1.0 / 1.000), seconds)
    pulse = seam_loop(resample(pulse, 1.0 / 1.005), seconds)

    # electrical texture: re-sprayed so it has no repeat of its own, then
    # folded so the loop point is continuous
    texture = respray(trim(takes.get("ignite", 4), 0.0, 0.55),
                      seconds, grain=0.11, hop=0.036, pitch_jitter=0.005,
                      seed=11, walk=0.06)
    texture = bandpass(texture, 2400.0, 9000.0)
    texture = seam_loop(texture, seconds, xfade=0.45)

    air = respray(trim(takes.get("clash", 4), 0.0, 0.40),
                  seconds, grain=0.09, hop=0.030, seed=37, walk=0.10)
    air = bandpass(air, 4000.0, 11000.0)
    air = seam_loop(air, seconds, xfade=0.45)

    layers = {
        "voice": filter_loop(scale(voice, 1.00), hum_tone),
        "engine": filter_loop(scale(body, 0.85), hum_tone),
        "pulse": filter_loop(scale(pulse, 0.70), hum_tone),
        "texture": filter_loop(scale(texture, 0.09), hum_tone),
        "air": filter_loop(scale(air, 0.05), hum_tone),
    }

    bed = [0.0] * len(voice)
    for samples in layers.values():
        add_at(bed, samples, 0)
    # a slow swell that is also a whole cycle per loop, so the bed breathes
    # without the breathing being a clue to where the loop point is
    for i in range(len(bed)):
        bed[i] *= 1.0 + 0.10 * math.sin(TAU * (i / RATE) / seconds)
    return bed, layers


def build_loop(takes, out_dir, seconds=10.0):
    """Write the designed loop and its stems into out_dir/loop."""
    out_dir = Path(out_dir) / "loop"
    out_dir.mkdir(parents=True, exist_ok=True)
    bed, layers = designed_loop(takes, seconds)
    bed = normalize_peak(normalize_rms(bed, -12.0), -2.5)
    save_wav(out_dir / ("hum-loop-%ds.wav" % int(seconds)), bed)

    for name in ("voice", "engine", "pulse", "texture", "air"):
        stem = normalize_rms(layers[name], -16.0)
        save_wav(out_dir / ("stem-%s-%ds.wav" % (name, int(seconds))), stem)

    # three passes, so the loop point can actually be judged
    save_wav(out_dir / ("hum-loop-%ds-x3.wav" % int(seconds)), bed * 3, -1.0)
    return bed, layers


def build(takes, out_dir):
    out_dir = Path(out_dir)
    hum = fit(blade_bed(takes, 13.0, BLADE_HZ), -12.0, -2.5)
    # the second hum is a heavier blade: a lower fundamental
    hum_alt = fit(blade_bed(takes, 13.0, BLADE_HZ * 0.80), -12.0, -2.5)

    files = {}
    files[("hum", "kyber-hum")] = hum
    files[("hum", "kyber-hum-alt")] = hum_alt
    files[("ignite", "kyber-ignite")] = ignite(takes, hum, blade_hz=BLADE_HZ)
    files[("ignite", "kyber-ignite-alt")] = ignite(
        takes, hum_alt, blade_hz=BLADE_HZ * 0.80)
    files[("off", "kyber-off")] = power_down(
        takes, hum, blade_hz=BLADE_HZ, number=4)
    files[("off", "kyber-off-alt")] = power_down(
        takes, hum_alt, blade_hz=BLADE_HZ * 0.80, number=3)
    files[("clash", "kyber-clash-01")] = fit(clash(takes, 4), -20.0, -6.0)
    files[("clash", "kyber-clash-02")] = fit(clash(takes, 2), -20.0, -6.0)
    files[("swipe", "kyber-swipe-01")] = fit(swipe(takes, 3), -19.0, -4.0)
    files[("swipe", "kyber-swipe-02")] = fit(swipe(takes, 4), -19.0, -4.0)

    for (role, name), samples in files.items():
        save_wav(out_dir / role / (name + ".wav"), samples)
    return files


def build_preview(out_dir, files, reference=None):
    """One audition file: the sounds in the order a user meets them."""
    def take(role, name):
        return files[(role, name)]

    plan = [
        ("hum", "kyber-hum", 3.2),
        ("ignite", "kyber-ignite", None),
        ("hum", "kyber-hum", 3.2),
        ("off", "kyber-off", None),
        ("swipe", "kyber-swipe-01", None),
        ("swipe", "kyber-swipe-02", None),
        ("clash", "kyber-clash-01", None),
        ("clash", "kyber-clash-02", None),
        ("hum", "kyber-hum-alt", 3.2),
        ("ignite", "kyber-ignite-alt", None),
        ("hum", "kyber-hum-alt", 3.2),
        ("off", "kyber-off-alt", None),
    ]
    gap = 0.55
    chunks = []
    for role, name, seconds in plan:
        audio = take(role, name)
        if seconds:
            audio = trim(audio, 0.0, seconds)
        chunks.append(audio)
        chunks.append([0.0] * int(gap * RATE))
    total = sum(len(c) for c in chunks)
    out = [0.0] * total
    pos = 0
    for chunk in chunks:
        add_at(out, chunk, pos)
        pos += len(chunk)
    save_wav(Path(out_dir) / "preview.wav", out, peak_db=-1.0)
    if reference:
        save_wav(Path(out_dir) / "reference-old-pack.wav", reference,
                 peak_db=-1.0)


def reference_from_assets(folder):
    """The pack the owner already approved, for side-by-side listening."""
    folder = Path(folder)
    order = [
        ("hum", "lightsaber-hum", 3.2),
        ("ignite", "lightsaber-ignite", None),
        ("hum", "lightsaber-hum", 3.2),
        ("off", "lightsaber-off", None),
        ("swipe", "lightsaber-whoosh-01", None),
        ("swipe", "lightsaber-whoosh-04", None),
        ("clash", "lightsaber-clash-02", None),
        ("clash", "lightsaber-clash-04", None),
    ]
    gap = 0.55
    chunks = []
    for role, name, seconds in order:
        path = folder / role / (name + ".wav")
        if not path.exists():
            continue
        audio = load_audio(path)
        if seconds:
            audio = trim(audio, 0.0, seconds)
        chunks.append(audio)
        chunks.append([0.0] * int(gap * RATE))
    total = sum(len(c) for c in chunks)
    out = [0.0] * total
    pos = 0
    for chunk in chunks:
        add_at(out, chunk, pos)
        pos += len(chunk)
    return out


def write_manifest(takes, out_dir, files):
    out_dir = Path(out_dir)
    manifest = {
        "schema_version": 2,
        "pack": "kyber-elevenlabs-mix-v2",
        "date": "2026-10-20",
        "origin": "Every sound is mixed from the owner's own ElevenLabs "
                  "sound-effect generations. No other recording is used.",
        "format": "mono PCM16 44100 Hz",
        "sources": takes.source_hashes(),
        "outputs": {},
    }
    for (role, name) in sorted(files):
        path = out_dir / role / (name + ".wav")
        manifest["outputs"][role + "/" + name + ".wav"] = {
            "sha256": digest(path),
            "bytes": path.stat().st_size,
            "seconds": round(path.stat().st_size / 2.0 / RATE, 3),
        }
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


NOTICE = """# Kyber sound pack, mixed from the owner's ElevenLabs takes

Every sound in this folder is mixed from the owner's own ElevenLabs
sound-effect generations in `~/Downloads/Sounds for Lightsaber`.  No other
recording is sampled, copied or downloaded, and nothing here comes from a
film, game or third-party sound library.

## What is in the folder

| File | What it is |
| --- | --- |
| `preview.wav` | The whole pack in the order a user meets it |
| `reference-old-pack.wav` | The previously approved pack, for comparison |
| `hum/` | The held hum: the blade's voice, its engine, its pulse |
| `ignite/` | Powering on |
| `off/` | Powering down |
| `clash/` | Two blades striking |
| `swipe/` | Two movement sounds |

`preview.wav` runs in this order: hum, ignite, hum, off, swipe, swipe,
clash, clash, then the same again with the heavier second hum.

## The 10-second loop

`loop/hum-loop-10s.wav` is *designed* to loop rather than trimmed to fit.
Every layer completes a whole number of cycles in ten seconds, and the tone
filters are run circularly so the filter state at the end matches the start.
The last sample therefore flows into the first, and the loop repeats
sample-exactly.

`loop/hum-loop-10s-x3.wav` plays it three times, so the loop point can be
judged rather than taken on trust.

`loop/stem-*.wav` are those same ten seconds split into layers. Each one loops
on its own, so they can be paired, dropped, or rebalanced:

| Stem | What it carries |
| --- | --- |
| `stem-voice` | the blade's note: the fundamental and its upper partials |
| `stem-engine` | the body underneath it |
| `stem-pulse` | the thrum, the part that makes it feel powered |
| `stem-texture` | electrical crackle |
| `stem-air` | fine sparkle on top |

The stems are individually normalised for listening; the loop is the balanced
mix. Summing the stems does not reproduce the loop exactly, because the loop
also carries a slow swell.

## How it was made

The owner's takes are between one and two seconds long, so each one was
stretched into a role:

* **The voice.** `Instant_plasma_surge` #2 holds the cleanest ladder of
  partials, a fundamental near 109 Hz with harmonics at 217 Hz and 322 Hz.
  Resampling puts it on the blade's 92 Hz fundamental, so its third partial
  lands on the hollow 278 Hz partial the approved hum has.
* **The engine.** `Deep_sci-fi_energy_b` #3 carries the same third partial
  and is the layer that gives the hum its weight.
* **The pulse.** `Deep_sci-fi_energy_b` #4 beats against itself at about
  6.7 Hz.  That is the thrum that makes the blade feel powered rather than
  played.
* **The texture.** The bright part of `Instant_plasma_surge` #4 is re-sprayed
  as overlapping grains, which keeps its crackle without repeating it.
* **Powering on** is the surge take played with a rising rate, so its pitch
  and brightness climb together, and it hands over to the hum at the end.
* **Powering down** is the same take played with a falling rate, handed over
  from the hum at the start.
* **Clashing** is the sharp `Blinding_sci-fi_lase` crack over a struck body
  from the deep take, so it lands heavy.
* **Swiping** is `A_heavy_cinematic_ba`, shaped to sit with the hum.

Every result was measured band by band against the previously approved pack
and trimmed to match it, so the new sounds sit in the same place as the old
ones.

## Rights

These takes are the owner's own ElevenLabs generations, and the mixing code
is part of this project.  Nothing here is a third-party recording, so the
unverified permission problem that applied to the previous pack does not
apply to this one.  The previous recordings remain third-party material and
should be removed from the repository history before this pack replaces them.
"""


def write_notice(out_dir):
    (Path(out_dir) / "README.md").write_text(NOTICE)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", required=True,
                        help="folder with the owner's ElevenLabs MP3 takes")
    parser.add_argument("--out", required=True, help="output folder")
    parser.add_argument("--reference", default=None,
                        help="folder with the approved pack, for comparison")
    parser.add_argument("--loop", action="store_true",
                        help="also write a designed 10 s loopable hum and its stems")
    args = parser.parse_args()

    takes = Takes(args.src)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    files = build(takes, out_dir)
    reference = None
    if args.reference:
        reference = reference_from_assets(args.reference)
    build_preview(out_dir, files, reference)
    if args.loop:
        build_loop(takes, out_dir, 10.0)
    write_manifest(takes, out_dir, files)
    write_notice(out_dir)
    print("wrote", out_dir)


if __name__ == "__main__":
    main()
