#!/usr/bin/env python3
"""Measure local transient references. Exports figures/numbers, no source audio."""
import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

os.environ.setdefault("MPLCONFIGDIR", str(Path(tempfile.gettempdir()) / "kyber-effects-matplotlib"))
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

SR = 44100


def measure(path):
    raw = subprocess.run([
        "ffmpeg", "-v", "error", "-i", str(path), "-map", "0:a:0", "-t", "30",
        "-ac", "1", "-ar", str(SR), "-f", "f32le", "pipe:1",
    ], check=True, capture_output=True).stdout
    samples = np.frombuffer(raw, dtype="<f4").astype(np.float64)
    if len(samples) < 4096 or not np.all(np.isfinite(samples)):
        raise ValueError(f"Invalid/too short reference: {path}")
    block = 441
    rms = np.sqrt(np.mean(samples[:len(samples)//block*block].reshape(-1, block)**2, axis=1))
    window = np.hanning(4096)
    spectrum = np.stack([abs(np.fft.rfft(samples[start:start+4096]*window, n=16384))**2
                         for start in range(0, len(samples)-4096+1, block)])
    frequencies = np.fft.rfftfreq(16384, 1/SR)
    bands = [(20, 180), (180, 600), (600, 2500), (2500, 9000)]
    energy = np.cumsum(samples*samples)
    energy /= max(float(energy[-1]), 1e-20)
    level = max(float(rms.max()), 1e-15)
    on = np.flatnonzero(rms > level*.04)
    band_levels = np.stack([np.sqrt(spectrum[:, (frequencies>=low)&(frequencies<high)].sum(axis=1))
                            for low, high in bands], axis=1)
    report = {
        "file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "seconds": round(len(samples)/SR, 4),
        "onset_seconds": round(float(on[0])/100, 3) if len(on) else None,
        "peak_envelope_seconds": round(float(np.argmax(rms))/100, 3),
        "energy_quantile_seconds": {str(q): round(float(np.searchsorted(energy, q))/SR, 3)
                                     for q in [.05, .5, .95]},
        "peak": float(np.max(np.abs(samples))),
        "rms": float(np.sqrt(np.mean(samples*samples))),
        "bands_hz": bands,
        "band_envelope_peak_seconds": [round(float(np.argmax(band_levels[:, i]))/100+.0464, 3)
                                       for i in range(len(bands))],
    }
    return report, rms, frequencies, spectrum, band_levels


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("references", nargs="+", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = [measure(path) for path in args.references]
    (args.output/"measurements.json").write_text(json.dumps([row[0] for row in results], indent=2)+"\n")
    fig, axes = plt.subplots(len(results), 2, figsize=(14, len(results)*3.2), squeeze=False, layout="constrained")
    for row, (report, rms, frequencies, spectrum, levels) in enumerate(results):
        times = np.arange(len(rms))*.01
        ax = axes[row, 0]
        ax.plot(times, rms/max(float(rms.max()), 1e-15), color="#126d67", linewidth=1.2, label="Full level")
        band_times = np.arange(len(levels))*.01+.0464
        for column, label in enumerate(("20–180 Hz", "180–600 Hz", "600–2500 Hz", "2.5–9 kHz")):
            ax.plot(band_times, levels[:, column]/max(float(levels.max()), 1e-15), linewidth=.8, alpha=.8, label=label)
        ax.set(title=report["file"]+" · changing layers", xlabel="Time (s)", ylabel="Relative level")
        ax.legend(fontsize=7, ncol=2);ax.grid(alpha=.2)
        ax = axes[row, 1]
        select = (frequencies>=40)&(frequencies<=9000)
        db = 10*np.log10(np.maximum(spectrum[:, select].T/spectrum.max(), 1e-9))
        ax.pcolormesh(band_times, frequencies[select], db, shading="auto", vmin=-60, vmax=0, cmap="magma")
        ax.set(yscale="log", ylim=(40,9000), xlabel="Time (s)", ylabel="Frequency (Hz)", title="Spectral movement")
    fig.savefig(args.output/"reference-motion.png", dpi=160)
    plt.close(fig)
    for row in results:
        print(json.dumps(row[0]))


if __name__ == "__main__":
    main()
