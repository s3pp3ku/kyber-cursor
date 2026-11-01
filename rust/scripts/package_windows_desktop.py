#!/usr/bin/env python3
"""Package the desktop-wide Windows app locally; no launch, upload or tests."""
import argparse
import hashlib
import json
import shutil
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "rust/crates/kyber-windows"
DEFAULT_EXE = ROOT / "rust/target/x86_64-pc-windows-gnullvm/release/kyber-cursor.exe"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def package(output, frames_archive, executable=DEFAULT_EXE):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    bundle = output / "kyber-windows-desktop-x64"
    bundle.mkdir(exist_ok=False)
    executable = Path(executable)
    binary = executable.read_bytes()
    if binary[:2] != b"MZ":
        raise ValueError("Expected a Windows PE executable")
    shutil.copy2(executable, bundle / "kyber-cursor.exe")
    shutil.copy2(SOURCE / "READ-ME.txt", bundle / "READ-ME.txt")
    # Use CRLF for the command file even when packaged from a Linux checkout.
    command = (SOURCE / "Restore Windows Cursor.cmd").read_text()
    (bundle / "Restore Windows Cursor.cmd").write_bytes(command.replace("\r\n", "\n").replace("\n", "\r\n").encode())
    frame_hashes = {}
    with zipfile.ZipFile(frames_archive) as old:
        for entry in old.infolist():
            path = PurePosixPath(entry.filename)
            if len(path.parts) < 3 or path.parts[1] != "frames" or entry.is_dir():
                continue
            relative = PurePosixPath(*path.parts[1:])
            if ".." in relative.parts or relative.is_absolute() or any("\\" in p or ":" in p for p in relative.parts):
                raise ValueError("Unsafe frame path")
            if str(relative) in frame_hashes:
                raise ValueError("Duplicate frame path")
            data = old.read(entry)
            target = bundle.joinpath(*relative.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            frame_hashes[str(relative)] = digest(data)
    palette = json.loads((bundle / "frames/colors.json").read_text())
    for name, _ in palette:
        expected = [f"{bank}-{n:02}.png" for bank in ("light", "dark") for n in range(17)]
        expected += [f"{bank}-hold-{n:02}.png" for bank in ("light", "dark") for n in range(24)]
        expected.append("frames.txt")
        for filename in expected:
            if f"frames/{name}/{filename}" not in frame_hashes:
                raise ValueError("Missing accepted frame: " + name + "/" + filename)
    manifest_path = ROOT / "assets/sounds/manifest.json"
    manifest = json.loads(manifest_path.read_text())
    for clip in manifest["clips"]:
        data = (ROOT / "assets/sounds" / clip["file"]).read_bytes()
        if digest(data) != clip["sha256"] or data not in binary:
            raise ValueError("Recording is changed or not embedded: " + clip["file"])
    for source, name in [(manifest_path, "sound-pack.json"),
                         (ROOT / "assets/sounds/README.md", "AUDIO-NOTICE.md"),
                         (ROOT / "LICENSE", "CODE-LICENSE.txt"),
                         (ROOT / "THIRD_PARTY_NOTICES.md", "THIRD-PARTY-NOTICES.md")]:
        shutil.copy2(source, bundle / name)
    info = {
        "package": "Kyber Cursor Windows desktop", "platform": "Windows 10/11 x64",
        "scope": "Desktop-wide standard system cursor replacement; no demo window",
        "target": "x86_64-pc-windows-gnullvm", "executable_sha256": digest(binary),
        "sound_pack": manifest["id"], "embedded_recordings": len(manifest["clips"]),
        "sound_manifest_sha256": digest(manifest_path.read_bytes()),
        "accepted_frame_archive_sha256": digest(Path(frames_archive).read_bytes()),
        "accepted_frame_files": len(frame_hashes),
        "frames_reused_byte_for_byte": True,
        "native_windows_interactive_validation_completed": False,
        "native_windows_audio_approval_completed": False,
        "native_windows_recovery_validation_completed": False,
        "tests_run": False, "live_linux_installation_changed": False,
        "uploaded": False,
    }
    (bundle / "build-info.json").write_text(json.dumps(info, indent=2) + "\n")
    (bundle / "frames-sha256.json").write_text(json.dumps(frame_hashes, indent=2) + "\n")
    archive = output / (bundle.name + ".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(bundle.rglob("*")):
            if path.is_file():
                zipped.write(path, bundle.name + "/" + path.relative_to(bundle).as_posix())
    checksum = digest(archive.read_bytes())
    archive.with_suffix(".zip.sha256").write_text(checksum + "  " + archive.name + "\n")
    (output / "package.json").write_text(json.dumps({**info, "archive": archive.name, "archive_sha256": checksum}, indent=2) + "\n")
    print(archive)
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--frames-archive", type=Path, required=True)
    parser.add_argument("--executable", type=Path, default=DEFAULT_EXE)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Use a new output folder so earlier packages stay intact")
    package(args.output, args.frames_archive, args.executable)
