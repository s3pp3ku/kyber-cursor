#!/usr/bin/env python3
"""Apply a reviewed local recorded-audio plan, with a complete saved rollback.

Updates only Kyber's helper, CLI, audio wrapper and recording directories.
Stops/starts kyber-ignite.service; never restarts the desktop shell or system.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

SERVICE = "kyber-ignite.service"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_hashes(path):
    return {file.relative_to(path).as_posix(): sha(file)
            for file in sorted(path.rglob("*")) if file.is_file()}


def run(*args, check=True, timeout=30):
    result = subprocess.run([str(arg) for arg in args], text=True, capture_output=True, timeout=timeout)
    if check and result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stdout.strip()} {result.stderr.strip()}")
    return result


def atomic_copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    stage = target.with_name(target.name + ".recorded-next")
    shutil.copy2(source, stage)
    os.replace(stage, target)


def readiness(expected):
    deadline = time.monotonic() + 15
    last = ""
    while time.monotonic() < deadline:
        active = run("systemctl", "--user", "is-active", "--quiet", SERVICE, check=False)
        status = run("hyprctl", "kybernative", "status", check=False, timeout=3)
        last = (status.stdout + status.stderr).strip()
        tokens = set(last.split())
        if active.returncode == 0 and {"protocol=2", "enabled=yes", *expected}.issubset(tokens):
            return last
        time.sleep(.25)
    raise RuntimeError("Kyber did not become ready: " + last)


def restore(backup):
    record = json.loads((backup / "manifest.json").read_text())
    run("systemctl", "--user", "stop", SERVICE)
    stamp = str(time.time_ns())
    for item in record["files"]:
        target = Path(item["target"])
        saved = backup / item["backup"]
        if item["existed"]:
            atomic_copy(saved, target)
        elif target.exists():
            # Retain newly installed files instead of deleting anything.
            archived = backup / "reverted-files" / stamp / item["backup"]
            archived.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(target), str(archived))
    for item in record["trees"]:
        target = Path(item["target"])
        saved = backup / item["backup"]
        # A partially applied update may never have moved the old directory.
        if item["existed"] and not saved.exists():
            continue
        if target.exists():
            archived = backup / "reverted-trees" / stamp / item["backup"]
            archived.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(target), str(archived))
        if item["existed"]:
            shutil.copytree(saved, target)
    run("systemctl", "--user", "start", SERVICE)
    return readiness(record["expected_plugin_tokens"])


def apply(plan_path):
    plan = json.loads(plan_path.read_text())
    run("systemctl", "--user", "is-active", "--quiet", SERVICE)
    if (Path(plan["state"]) / "sound-mode").read_text().strip() != plan["previous_sound_mode"]:
        raise RuntimeError("Sound mode changed since review; no files changed")
    for item in plan["files"]:
        source, target = Path(item["source"]), Path(item["target"])
        if sha(source) != item["source_sha256"]:
            raise RuntimeError("Staged source changed: " + str(source))
        current = sha(target) if target.is_file() else None
        if current != item["previous_sha256"]:
            raise RuntimeError("Installed file changed since review: " + str(target))
    for item in plan["trees"]:
        source, target = Path(item["source"]), Path(item["target"])
        if tree_hashes(source) != item["source_hashes"]:
            raise RuntimeError("Staged sound tree changed: " + str(source))
        current = tree_hashes(target) if target.is_dir() else None
        if current != item["previous_hashes"]:
            raise RuntimeError("Installed sound tree changed since review: " + str(target))
    for name, expected in plan["unchanged_files"].items():
        if sha(Path(name)) != expected:
            raise RuntimeError("Protected Kyber setting changed: " + name)

    backup = Path(plan["backup"])
    backup.mkdir(parents=True, exist_ok=False)
    record = {"files": [], "trees": [], "expected_plugin_tokens": plan["expected_plugin_tokens"],
              "previous_sound_mode": plan["previous_sound_mode"], "volume": plan["volume"]}
    for index, item in enumerate(plan["files"]):
        target = Path(item["target"])
        relative = f"files/{index}-{target.name}"
        existed = target.is_file()
        if existed:
            saved = backup / relative
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target, saved)
        record["files"].append({"target": str(target), "backup": relative, "existed": existed,
                                "previous_sha256": item["previous_sha256"], "new_sha256": item["source_sha256"]})
    # Mode is the only existing setting changed for the listening test.
    mode = Path(plan["state"]) / "sound-mode"
    relative = "files/sound-mode"
    shutil.copy2(mode, backup / relative)
    record["files"].append({"target": str(mode), "backup": relative, "existed": True})
    for index, item in enumerate(plan["trees"]):
        record["trees"].append({"target": item["target"], "backup": f"trees/{index}",
                                "existed": Path(item["target"]).is_dir()})
    (backup / "manifest.json").write_text(json.dumps(record, indent=2) + "\n")
    (backup / "ROLLBACK.txt").write_text(
        "Restore the saved Kyber audio and restart only its helper:\n"
        f"/usr/bin/python3 {Path(__file__).resolve()} --restore {backup}\n")

    # Stage both recording trees before interrupting the existing helper.
    stages = []
    for index, item in enumerate(plan["trees"]):
        stage = backup / f"staged-tree-{index}"
        shutil.copytree(item["source"], stage)
        stages.append(stage)
    try:
        run("systemctl", "--user", "stop", SERVICE)
        for item in plan["files"]:
            atomic_copy(Path(item["source"]), Path(item["target"]))
        for item, saved, stage in zip(plan["trees"], record["trees"], stages):
            target = Path(item["target"])
            if target.exists():
                archived = backup / saved["backup"]
                archived.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(target), str(archived))
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(stage, target)
        mode.write_text("all\n")
        run("systemctl", "--user", "start", SERVICE)
        status = readiness(plan["expected_plugin_tokens"])
        for item in plan["files"]:
            if sha(Path(item["target"])) != item["source_sha256"]:
                raise RuntimeError("Installed file verification failed")
        for item in plan["trees"]:
            if tree_hashes(Path(item["target"])) != item["source_hashes"]:
                raise RuntimeError("Installed recording verification failed")
        for name, expected in plan["unchanged_files"].items():
            if sha(Path(name)) != expected:
                raise RuntimeError("Protected setting changed during update: " + name)
        doctor = run(Path(plan["runtime"]) / "kyber", "doctor").stdout.strip()
        report = {"status": "installed", "backup": str(backup), "pack": "kyber-private-clips-v1",
                  "sound_mode": "all", "volume": plan["volume"], "plugin_status": status,
                  "doctor": doctor, "files": record["files"], "trees": record["trees"]}
        Path(plan["report"]).write_text(json.dumps(report, indent=2) + "\n")
        print("Installed recorded sounds; full sound mode; volume " + plan["volume"] + "%.")
        print("Rollback: " + str(backup))
        print(doctor)
    except Exception:
        print("Update failed; restoring the saved Kyber installation.")
        status = restore(backup)
        print("Previous installation restored: " + status)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--apply", type=Path, metavar="REVIEWED_PLAN")
    group.add_argument("--restore", type=Path, metavar="BACKUP")
    args = parser.parse_args()
    if args.apply:
        apply(args.apply.resolve())
    else:
        print(restore(args.restore.resolve()))


if __name__ == "__main__":
    main()
