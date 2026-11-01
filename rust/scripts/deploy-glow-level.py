#!/usr/bin/env python3
"""Back up and deploy a locally built Kyber glow renderer revision.

The deployment changes only the managed kyber-ignite executable and its
generated frame-bank symlink. It leaves user settings, sounds, the native
bridge binary, desktop shell, and bar configuration alone.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


HOME = Path.home()
RUNTIME = HOME / ".local/share/kyber-cursor/rust"
FRAMES = HOME / ".cache/kyber-cursor/native"
CONFIG = Path(os.environ.get("XDG_CONFIG_HOME", HOME / ".config")) / "kyber-cursor"
BACKUPS = HOME / ".local/share/kyber-cursor/backups"
SERVICE = "kyber-ignite.service"


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, text=True, capture_output=True)
    if check and result.returncode:
        message = (result.stdout + result.stderr).strip()
        raise RuntimeError(f"{args[0]} failed: {message}")
    return result


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def selected_color() -> str:
    mode = (CONFIG / "color-mode").read_text().strip()
    if mode == "custom":
        color_file = CONFIG / "color"
    elif mode == "random":
        color_file = CONFIG / "random-color"
    else:
        color_file = HOME / ".local/share/icons/Omarchy-Kyber/.accent"
    color = color_file.read_text().strip().lower()
    if len(color) != 7 or color[0] != "#":
        raise RuntimeError("The selected Kyber glow color is invalid")
    return color


def check_ready(expected_generation: str | None = None) -> tuple[str, str]:
    service = run("/usr/bin/systemctl", "--user", "is-active", SERVICE)
    status = run("/usr/bin/hyprctl", "kybernative", "status")
    if service.stdout.strip() != "active":
        raise RuntimeError("The Kyber helper service is not active")
    if "protocol=2" not in status.stdout or "generation=" not in status.stdout:
        raise RuntimeError("The Kyber compositor bridge is not ready")
    if expected_generation and expected_generation not in status.stdout:
        raise RuntimeError("The compositor has not loaded the new Kyber frame bank")
    return service.stdout.strip(), status.stdout.strip()


def atomic_copy(source: Path, target: Path) -> None:
    temporary = target.with_name(target.name + ".next")
    shutil.copy2(source, temporary)
    os.replace(temporary, target)


def replace_current(generation: str) -> None:
    temporary = FRAMES / f"current-next-{os.getpid()}-{time.time_ns()}"
    os.symlink(generation, temporary)
    os.replace(temporary, FRAMES / "current")


def restart(expected_generation: str) -> str:
    run("/usr/bin/systemctl", "--user", "restart", SERVICE)
    deadline = time.monotonic() + 30
    last_error = "service did not become ready"
    while time.monotonic() < deadline:
        try:
            _, status = check_ready(expected_generation)
            return status
        except (RuntimeError, OSError) as error:
            last_error = str(error)
            time.sleep(0.25)
    raise RuntimeError(last_error)


def rollback(backup: Path, restart_service: bool = True) -> None:
    manifest = json.loads((backup / "manifest.json").read_text())
    atomic_copy(backup / "kyber-ignite", RUNTIME / "kyber-ignite")
    replace_current(manifest["previous_generation"])
    if restart_service and manifest["service_active"]:
        restart(manifest["previous_generation"])


def apply(binary: Path, frame_bank: Path) -> None:
    binary = binary.resolve(strict=True)
    frame_bank = frame_bank.resolve(strict=True)
    installed = RUNTIME / "kyber-ignite"
    current = FRAMES / "current"
    if not binary.is_file() or not installed.is_file() or not current.is_symlink():
        raise RuntimeError("Kyber's installed helper or current frame bank is missing")
    color = selected_color()
    staged_color = (frame_bank / "color.txt").read_text().strip().lower()
    if staged_color != color:
        raise RuntimeError(f"Frame bank color {staged_color} does not match current setting {color}")
    old_target = os.readlink(current)
    if not (FRAMES / old_target).is_dir():
        raise RuntimeError("The current Kyber frame bank is missing; nothing changed")
    check_ready()

    stamp = time.time_ns()
    name = f"generation-glow-eight-{stamp}"
    backup = BACKUPS / f"glow-eight-{stamp}"
    backup.mkdir(parents=True, exist_ok=False)
    shutil.copy2(installed, backup / "kyber-ignite")
    manifest = {
        "target_level": 8,
        "previous_generation": old_target,
        "new_generation": name,
        "service_active": True,
        "previous_helper_sha256": sha256(backup / "kyber-ignite"),
        "new_helper_sha256": sha256(binary),
        "color_preserved": color,
        "config_modified": False,
        "bridge_binary_modified": False,
    }
    (backup / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (backup / "ROLLBACK.txt").write_text(
        "Restore with:\n"
        f"python3 {Path(__file__).resolve()} --restore {backup}\n"
    )

    generation = FRAMES / name
    shutil.copytree(frame_bank, generation)
    try:
        atomic_copy(binary, installed)
        replace_current(name)
        status = restart(name)
    except Exception:
        rollback(backup)
        raise
    print(f"Kyber glow level 8 is active ({color}).")
    print(f"Bridge: {status}")
    print(f"Rollback backup: {backup}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", nargs=2, metavar=("BINARY", "FRAME_BANK"))
    parser.add_argument("--restore", type=Path)
    args = parser.parse_args()
    try:
        if args.restore:
            rollback(args.restore.resolve(strict=True))
            print(f"Restored Kyber from {args.restore}")
        elif args.apply:
            apply(Path(args.apply[0]), Path(args.apply[1]))
        else:
            parser.error("choose --apply BINARY FRAME_BANK or --restore BACKUP")
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"Deployment stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
