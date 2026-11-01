#!/usr/bin/env python3
"""Package the prepared private sounds, current binaries and source locally.

Does not download, upload, install, open an audio device, or run tests.
Reuses an accepted frame archive without re-rendering the cursor.
"""
import argparse
import base64
import hashlib
import html
import json
import os
import shutil
import subprocess
import zipfile
from pathlib import Path
from package_windows_desktop import package as package_windows

ROOT = Path(__file__).resolve().parents[2]
PACK = ROOT / "assets/sounds"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def archive(folder):
    target = folder.with_suffix(".zip")
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(folder.rglob("*")):
            if path.is_file():
                zipped.write(path, folder.name + "/" + path.relative_to(folder).as_posix())
    target.with_suffix(".zip.sha256").write_text(sha(target) + "  " + target.name + "\n")
    print(target)


def copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def notices(folder):
    copy(PACK / "manifest.json", folder / "sound-pack.json")
    copy(PACK / "README.md", folder / "AUDIO-NOTICE.md")
    copy(ROOT / "LICENSE", folder / "CODE-LICENSE.txt")
    copy(ROOT / "THIRD_PARTY_NOTICES.md", folder / "THIRD-PARTY-NOTICES.md")


def build_info(folder, **extra):
    binaries = {path.name: sha(path) for path in folder.rglob("*")
                if path.is_file() and (path.suffix in (".exe", ".so") or path.name in ("kyber", "kyber-ignite"))}
    (folder / "build-info.json").write_text(json.dumps({
        "sound_pack": "kyber-private-clips-v1", "sound_manifest_sha256": sha(PACK / "manifest.json"),
        "audio": "Selected recordings embedded; generated defaults are not used",
        "binary_sha256": binaries, "tests_run": False,
        "native_audio_listening_completed": False, **extra,
    }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--frames-archive", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((PACK / "manifest.json").read_text())
    for clip in manifest["clips"]:
        if sha(PACK / clip["file"]) != clip["sha256"]:
            raise ValueError("Sound pack hash mismatch: " + clip["file"])

    sounds = out / "kyber-private-sounds"
    shutil.copytree(PACK, sounds)
    cards = []
    for role in ("ignite", "hum", "off", "swipe", "clash"):
        cards.append("<h2>" + html.escape(role) + "</h2>")
        for clip in manifest["clips"]:
            if clip["role"] != role:
                continue
            encoded = base64.b64encode((PACK / clip["file"]).read_bytes()).decode()
            cards.append('<p>' + html.escape(Path(clip["file"]).stem)
                         + '</p><audio controls preload="none" src="data:audio/wav;base64,' + encoded + '"></audio>')
    (sounds / "index.html").write_text('''<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; media-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<title>Kyber — selected recordings</title><style>body{color-scheme:dark;background:#101719;color:#e4efeb;font:17px/1.6 system-ui;max-width:760px;margin:40px auto;padding:0 20px}audio{width:100%;max-width:600px}h2{margin-top:40px}p{margin-bottom:8px}</style>
<h1>Kyber selected recordings</h1><p>These are the clips included in the private packages. Choose any clip to listen. Nothing plays automatically.</p>
''' + "\n".join(cards) + '''<script>const players=[...document.querySelectorAll('audio')];for(const player of players){player.volume=.4;player.addEventListener('play',()=>players.forEach(other=>{if(other!==player)other.pause()}))}document.addEventListener('visibilitychange',()=>{if(document.hidden)players.forEach(player=>player.pause())});</script></html>''')
    archive(sounds)

    package_windows(out, args.frames_archive)

    linux = out / "kyber-linux-runtime-recorded"
    copy(ROOT / "rust/target/release/kyber", linux / "bin/kyber")
    copy(ROOT / "rust/target/release/kyber-ignite", linux / "bin/kyber-ignite")
    adapter = ROOT / "native-cursor/build/kyber-native-color-trial.so"
    if adapter.is_file():
        copy(adapter, linux / "lib/kyber-native.so")
    (linux / "READ-ME.txt").write_text(
        "Kyber private recorded-audio runtime update, Linux x64\n\n"
        "These binaries contain the selected clips, including recorded hold.\n"
        "They are staged files; extracting this folder installs or restarts nothing.\n"
        "This is an update bundle for an existing Rust/Hyprland setup.\n"
        "The native bridge must match the installed Hyprland build. Other Linux\n"
        "distributions may need a source rebuild and their own desktop adapter.\n"
        "Use the accompanying source package for a full source install.\n"
        "The sound-only pack is portable to Windows, Mac and other Linux systems.\n"
        "Live deployment requires a separate saved rollback and helper reload.\n")
    notices(linux)
    build_info(linux, platform="Linux x64", scope="Staged runtime update; not installed; native bridge ABI depends on Hyprland")
    archive(linux)

    source = out / "kyber-private-source"
    tracked = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True).stdout.decode().split("\0")
    files = {ROOT / name for name in tracked if name}
    for directory in ("rust", "assets", "native-cursor", ".github"):
        for parent, directories, names in os.walk(ROOT / directory):
            directories[:] = [name for name in directories if name not in ("target", "build", "__pycache__", ".git")]
            files.update(Path(parent) / name for name in names)
    files.update((ROOT / "HANDOFF.md", ROOT / "lib/sound_pack.py"))
    for path in sorted(files):
        if path.is_file():
            copy(path, source / path.relative_to(ROOT))
    notices(source)
    archive(source)
    (out / "packages.json").write_text(json.dumps({
        "sound_pack": manifest["id"],
        "archives": {p.name: sha(p) for p in sorted(out.glob("*.zip"))},
        "runtime_deployed": False, "github_published": False,
        "macos": "Portable recordings and source only; no native desktop runtime exists yet",
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
