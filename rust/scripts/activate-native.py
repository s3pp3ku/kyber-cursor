#!/usr/bin/env python3
"""Opt-in user-local deployment for this rig, with a saved rollback.
Does not restart the desktop, change bars, install packages, or use sudo.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

REPO = Path(__file__).resolve().parents[2]
HOME_DIR = Path.home()
STATE = Path(os.environ.get('XDG_CONFIG_HOME', str(HOME_DIR / '.config'))) / 'kyber-cursor'
RUNTIME = HOME_DIR / '.local/share/kyber-cursor/rust'
FRAMES = HOME_DIR / '.cache/kyber-cursor/native'
DROPIN = HOME_DIR / '.config/systemd/user/kyber-ignite.service.d/90-rust-native.conf'
ENTRY = HOME_DIR / '.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber'

def run(*args, check=True):
    result = subprocess.run([str(arg) for arg in args], text=True, capture_output=True)
    if check and result.returncode:
        raise RuntimeError(f'{args[0]} failed: {result.stdout.strip()} {result.stderr.strip()}')
    return result

def active(unit):
    return run('systemctl', '--user', 'is-active', '--quiet', unit, check=False).returncode == 0

def atomic_copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + '.next')
    shutil.copy2(source, temporary)
    temporary.replace(destination)

def restore(backup):
    manifest = json.loads((backup / 'manifest.json').read_text())
    run('systemctl', '--user', 'stop', 'kyber-ignite.service', check=False)
    for item in reversed(manifest['files']):
        target = Path(item['path'])
        if item['existed']:
            atomic_copy(backup / item['backup'], target)
        elif target.exists():
            target.unlink()
    run('systemctl', '--user', 'daemon-reload')
    if manifest['service_active']:
        run('systemctl', '--user', 'start', 'kyber-ignite.service')
    if manifest['trial_active']:
        run('systemd-run', '--user', '--unit=kyber-native-art-trial.service',
            '--property=Restart=on-failure', '--property=RestartSec=2',
            REPO / 'test-artifacts/rust-daily-2026-10-16/art-watcher-before',
            '--watch-native', REPO / 'test-artifacts/native-cursor-trial')
    if manifest['old_adapter']:
        run('hyprctl', 'plugin', 'load', REPO / 'test-artifacts/rust-daily-2026-10-16/adapter-0.3.so')
        run('hyprctl', 'kybernative', manifest['bank'])
    print('Previous cursor runtime restored.')

def apply():
    if not ENTRY.is_file():
        raise RuntimeError('Existing Kyber installation was not found; no changes made.')
    sources = [REPO / 'rust/target/release/kyber-ignite', REPO / 'rust/target/release/kyber',
               REPO / 'native-cursor/build/kyber-native-color-trial.so']
    if not all(path.is_file() for path in sources):
        raise RuntimeError('Build the Rust binaries and native adapter first.')
    status = run('hyprctl', 'kybernative', 'status', check=False).stdout
    bank = 'light' if 'mode=light' in status else 'dark'
    backup = REPO / 'test-artifacts/rust-daily-2026-10-16' / f'backup-{time.time_ns()}'
    backup.mkdir(parents=True)
    targets = [DROPIN, ENTRY, STATE/'native-runtime', STATE/'native-bank', STATE/'trail-mode',
               RUNTIME/'kyber-ignite', RUNTIME/'kyber', RUNTIME/'kyber-native.so']
    manifest = {'files':[], 'service_active':active('kyber-ignite.service'),
                'trial_active':active('kyber-native-art-trial.service'), 'old_adapter':'generation=' in status,
                'bank':bank}
    for index, target in enumerate(targets):
        name = f'file-{index}'
        existed = target.is_file()
        if existed:
            shutil.copy2(target, backup/name)
        manifest['files'].append({'path':str(target), 'existed':existed, 'backup':name})
    (backup/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    (backup/'ROLLBACK.txt').write_text(f'python3 {Path(__file__).resolve()} --restore {backup}\n')
    # Prepare everything before interrupting the running trial.
    for source, name in zip(sources, ['kyber-ignite','kyber','kyber-native.so']):
        atomic_copy(source, RUNTIME/name)
    FRAMES.mkdir(parents=True,exist_ok=True)
    run(RUNTIME/'kyber-ignite','--prepare-native',FRAMES)
    text = ENTRY.read_text()
    begin = '# >>> Kyber Rust daily runtime >>>'
    end = '# <<< Kyber Rust daily runtime <<<'
    source = (REPO/'bin/kyber').read_text()
    block = source[source.index(begin):source.index(end)+len(end)]
    if begin in text:
        text = text[:text.index(begin)] + block + text[text.index(end)+len(end):]
    else:
        insertion = text.index('\n',text.index('\nSTATE=')+1)+1
        text = text[:insertion]+'\n'+block+'\n'+text[insertion:]
    try:
        temporary = ENTRY.with_name('kyber.next')
        temporary.write_text(text)
        temporary.chmod(ENTRY.stat().st_mode & 0o777)
        temporary.replace(ENTRY)
        DROPIN.parent.mkdir(parents=True,exist_ok=True)
        atomic_copy(REPO/'rust/systemd/native.conf', DROPIN)
        (STATE/'native-bank').write_text(bank+'\n')
        (STATE/'native-runtime').write_text('0.4\n')
        (STATE/'trail-mode').write_text('off\n')
        if manifest['trial_active']:
            run('systemctl','--user','stop','kyber-native-art-trial.service')
        if manifest['old_adapter']:
            run('hyprctl','plugin','unload',REPO/'native-cursor/build/kyber-native-color-trial.so')
        run('systemctl','--user','daemon-reload')
        run('systemctl','--user','enable','--now','kyber-ignite.service')
        for _ in range(60):
            status=run('hyprctl','kybernative','status',check=False).stdout
            if active('kyber-ignite.service') and 'protocol=2' in status and str(FRAMES) in status:
                print(f'Rust daily runtime active. Rollback saved in {backup}')
                return
            time.sleep(0.25)
        raise RuntimeError('Rust runtime did not become ready; inspect its journal.')
    except Exception:
        restore(backup)
        raise

if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--apply',action='store_true')
    parser.add_argument('--restore',type=Path)
    args=parser.parse_args()
    if args.restore:
        restore(args.restore)
    elif args.apply:
        apply()
    else:
        print('Plan: install user-local Rust binaries and adapter, prepare accepted frames, save rollback,')
        print('route existing Kyber controls to Rust, replace the temporary watcher with kyber-ignite.service,')
        print('and enable it at login. No desktop restart, bar changes, downloads, or system file changes.')
