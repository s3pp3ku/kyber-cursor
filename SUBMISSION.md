# Marketplace submission

Pre-filled answers for the submission issue at
https://github.com/omacom/omarchy-plugin-marketplace/issues/new?template=submit-plugin.yml

File this only after the plugin has been installed from the repository and
tested on a real Omarchy machine.

**Repository URL**
```
https://github.com/s3pp3ku/kyber-cursor
```

**Category**
```
Appearance
```

**Tags** (max 3)
```
Hyprland
```

**Suggest a missing tag**
```
Cursor
```

**Maintainer notes**
```
A mouse pointer in the active Omarchy theme's accent color, inspired by the
iconic laser sword of a very famous space opera. Holding a mouse button
ignites it: a brighter blade, crossguard beams, a pulsing glow and a fading
drag trail. Optional sounds (no clips bundled: users add their own). The cursor
is rebuilt on every `omarchy theme set` and at boot.

Ships as a `service`-kind plugin with no UI of its own. Nothing is installed
until the user runs the bundled, opt-in `install.sh`, which lists every step
and asks first: a `kyber` command in ~/.local/bin, a systemd *user* service
(the click glow) and theme-set/post-boot hooks. `uninstall.sh` reverts all of it.

Permissions and dependencies you should know about:
- The click glow needs the user to be in the `input` group. It opens
  /dev/input/event* read-only and WITHOUT grabbing (no EVIOCGRAB), counts
  mouse-button presses only (no keys, no coordinates), and has no network access.
  The installer never uses sudo: the README tells the user to run
  `sudo usermod -aG input $USER` themselves, with a warning about what the group allows.
- On each press it reads a 20x20 pixel patch of the screen next to the cursor
  via `grim` to choose a dark or light glow. The patch is reduced to one number
  in memory and is never stored or sent.
- The glow is a click-through GTK4 layer-shell overlay (empty input region).
  The systemd unit sets NoNewPrivileges, ProtectSystem=strict, ProtectHome=read-only.
- Dependencies (Arch package names are in the README): python-gobject,
  python-cairo, python-pillow, python-cairosvg, python-evdev, gtk4,
  gtk4-layer-shell, grim, pipewire.
- No network access at install or run time. MIT licensed.

Known limits: tested on one machine (Hyprland 0.56, 1.6x scaling); the glow is
subtler on very light backgrounds; the overlay does not know the current cursor
shape, so a text drag also glows.
```

**Submission checklist**
- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
