#!/usr/bin/env python3
"""Local reference measurements. Decodes in memory; exports no source audio.

Developer tool: requires ffmpeg, NumPy and Matplotlib. These are not runtime
dependencies of a generated sound pack. No network or installed-settings access.
"""
import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

os.environ.setdefault("MPLCONFIGDIR", str(Path(tempfile.gettempdir()) / "kyber-hum-matplotlib"))
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

SR = 44100
NFFT = 65536


def peaks(frequencies, power, low, high, count):
    candidates = np.flatnonzero(
        (power[1:-1] > power[:-2]) & (power[1:-1] >= power[2:])
    ) + 1
    candidates = [i for i in candidates if low <= frequencies[i] <= high]
    selected = []
    for index in sorted(candidates, key=lambda i: power[i], reverse=True):
        if all(abs(frequencies[index] - frequencies[other]) > 3 for other in selected):
            selected.append(index)
            if len(selected) == count:
                break
    maximum = max(float(np.max(power)), 1e-30)
    return [{"hz": round(float(frequencies[i]), 2),
             "relative_db": round(float(10 * np.log10(max(float(power[i]), 1e-30) / maximum)), 2)}
            for i in selected]


def measure(path):
    decoded = subprocess.run([
        "ffmpeg", "-v", "error", "-i", str(path), "-t", "60",
        "-ac", "2", "-ar", str(SR), "-f", "f32le", "pipe:1",
    ], check=True, capture_output=True).stdout
    stereo = np.frombuffer(decoded, dtype="<f4").reshape(-1, 2).astype(np.float64)
    if len(stereo) < NFFT or not np.all(np.isfinite(stereo)):
        raise ValueError(f"Reference is too short or contains invalid samples: {path}")
    mono = stereo.mean(axis=1)
    # Omit the edges of a clip from the sustained-tone spectral estimate.
    trim = min(SR // 2, (len(mono) - NFFT) // 2)
    body = mono[trim:len(mono) - trim] if trim else mono
    window = np.hanning(NFFT)
    spectra = [np.abs(np.fft.rfft(body[start:start + NFFT] * window)) ** 2
               for start in range(0, len(body) - NFFT + 1, NFFT // 8)]
    power = np.mean(spectra, axis=0)
    frequencies = np.fft.rfftfreq(NFFT, 1 / SR)
    frame = SR // 100
    envelope = np.sqrt(np.mean(mono[:len(mono) // frame * frame].reshape(-1, frame) ** 2, axis=1))
    modulation = np.abs(np.fft.rfft((envelope - envelope.mean()) * np.hanning(len(envelope)))) ** 2
    modulation_hz = np.fft.rfftfreq(len(envelope), frame / SR)
    total = max(float(power.sum()), 1e-30)
    bands = [(20, 100), (100, 250), (250, 600), (600, 1500), (1500, 5000), (5000, 15000)]
    report = {
        "file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "seconds": round(len(mono) / SR, 4),
        "rms_dbfs": round(float(20 * np.log10(max(float(np.sqrt(np.mean(mono ** 2))), 1e-15))), 2),
        "peak_dbfs": round(float(20 * np.log10(max(float(np.max(np.abs(stereo))), 1e-15))), 2),
        "stereo_correlation": round(float(np.corrcoef(stereo.T)[0, 1]), 4),
        "side_to_mid_rms": round(float(np.sqrt(np.mean(((stereo[:, 0] - stereo[:, 1]) / 2) ** 2)) /
                                           max(float(np.sqrt(np.mean(mono ** 2))), 1e-15)), 4),
        "spectral_peaks": peaks(frequencies, power, 25, 3000, 24),
        "band_energy_percent": {
            f"{low}-{high}": round(float(power[(frequencies >= low) & (frequencies < high)].sum()) / total * 100, 2)
            for low, high in bands
        },
        "envelope_variation": round(float(envelope.std() / max(float(envelope.mean()), 1e-15)), 4),
        "envelope_modulation_peaks": peaks(modulation_hz, modulation, .2, 25, 5),
        "measurement": "Mono mean spectrum, Hann-windowed Welch estimate, excluding up to 0.5 s at each edge; 10 ms RMS envelope",
    }
    return report, mono, frequencies, power, envelope


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("references", nargs="+", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = [measure(path) for path in args.references]
    reports = [item[0] for item in results]
    (args.output / "measurements.json").write_text(json.dumps(reports, indent=2) + "\n")
    fig, axes = plt.subplots(len(results), 2, figsize=(14, 3.5 * len(results)), squeeze=False, layout="constrained")
    for row, (report, mono, frequencies, power, envelope) in enumerate(results):
        ax = axes[row, 0]
        db = 10 * np.log10(np.maximum(power / power.max(), 1e-12))
        ax.semilogx(frequencies[1:], db[1:], color="#147d79", linewidth=1)
        ax.set(xlim=(25, 10000), ylim=(-75, 3), xlabel="Frequency (Hz)", ylabel="Relative power (dB)", title=report["file"] + " · spectral balance")
        ax.grid(alpha=.2)
        for peak in report["spectral_peaks"][:5]:
            ax.annotate(f'{peak["hz"]:.0f}', (peak["hz"], peak["relative_db"]), xytext=(3, 5), textcoords="offset points", fontsize=8)
        ax = axes[row, 1]
        times = np.arange(len(envelope)) / 100
        ax.plot(times, envelope, color="#8c4e87", linewidth=.8)
        ax.set(xlabel="Time (s)", ylabel="RMS amplitude", title="Loudness over time (10 ms blocks)")
        ax.grid(alpha=.2)
    fig.savefig(args.output / "reference-measurements.png", dpi=160)
    plt.close(fig)
    for report in reports:
        print(json.dumps({key: report[key] for key in (
            "file", "seconds", "rms_dbfs", "stereo_correlation", "spectral_peaks", "band_energy_percent", "envelope_modulation_peaks"
        )}))


if __name__ == "__main__":
    main()
