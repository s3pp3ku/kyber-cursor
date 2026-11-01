#!/usr/bin/python3
"""Compare Rust CLI output to the unchanged Bash command and actual QML consumers.
All settings fixtures are temporary. No live settings or desktop changes.
"""
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PANEL = ROOT / 'Panel.qml'
rust = ROOT / 'rust/target/release/kyber'
legacy = ROOT / 'bin/kyber'
panel = PANEL.read_text()
block = re.search(r'id: trailSwitch\s+checked: ([^\n]+)\s+accent: [^\n]+\s+onToggled: ([^\n]+)', panel)
assert block, 'Could not locate actual tray switch bindings'
checked, toggled = block.groups()
results = []
with tempfile.TemporaryDirectory(prefix='kyber-tray-contract-') as temp:
    settings = Path(temp) / 'kyber-cursor'
    settings.mkdir()
    env = dict(os.environ, XDG_CONFIG_HOME=temp)
    for mode, trail in itertools.product(['theme', 'custom', 'random'], ['on', 'off']):
        for name, value in {'color-mode':mode, 'color':'#080bf9', 'random-color':'#4efade',
                            'sound-mode':'all', 'trail-mode':trail, 'volume':'56'}.items():
            (settings / name).write_text(value + '\n')
        (settings / 'active').touch()
        def config(command):
            return json.loads(subprocess.check_output([str(command), 'config'], env=env, text=True))
        expected, actual = config(legacy), config(rust)
        assert actual == expected, (mode, trail, expected, actual)
        js = 'const root={cfg:' + json.dumps(actual) + ',act:command=>console.log(command)};'
        js += f'console.log(JSON.stringify({checked})); {toggled};'
        display, command = subprocess.check_output(['node', '-e', js], text=True).strip().splitlines()
        assert json.loads(display) == (trail == 'on')
        assert command == ('trail off' if trail == 'on' else 'trail on')
        results.append({'mode':mode, 'trail':trail, 'checked':json.loads(display), 'click_command':command,
                        'legacy_schema_identical':True, 'config':actual})
print(f'{len(results)} cases: Rust matches Bash config; actual tray bindings display/toggle correctly.')
