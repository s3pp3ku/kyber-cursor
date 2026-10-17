# Kyber Cursor

Kyber Cursor is an independent Omarchy / Hyprland plugin that turns the mouse pointer into a theme-colored blade. Hold a mouse button to ignite it, light the crossguard beams and leave a fading trail while dragging.

![Kyber Cursor: a glowing blade for your Omarchy pointer](preview.png)

![Kyber igniting on click-and-hold, with the drag trail](media/demo.gif)

[Watch the clip (mp4, no sound)](media/demo.mp4)

| Ignited | Dragging |
|---|---|
| ![The cursor lit up, with its glow](media/glow-ignite.png) | ![The cursor mid-drag, with its fading trail](media/glow-trail.png) |

The options popup: choose a glow color and adjust the cursor and trail.

![The options popup in Theme, Custom and Random modes](media/popup-modes.png)

Every cursor role, generated from the accent:

![All cursor roles](media/cursor-roles.png)

## Features
- Full cursor set (pointer, link, busy, text, crosshair, move, resize, grab, not-allowed...), regenerated in the current accent on every `omarchy theme set` and at boot.
- Click-and-hold glow with ignition animation, beams growing out of the emitters, a slow pulse, and a drag trail (toggle it off with `kyber trail off` if you find it distracting).
- The glow adapts to the page behind it: a pale, defined version over white pages, a vivid one over dark ones.
- A bar button with a popup: a color picker (hue, vividness and brightness bars plus presets), trail and on/off switches. No terminal needed.
- Three glow-color sources: **Theme** (follows the Omarchy accent, the default), **Custom** (your own color) and **Random** (a new vivid color at each theme change and at boot, or on demand). Random colors are always bright and clearly different from the last one.
- Dark accents (deep orange, red, purple) are brightened so every theme glows about equally.
- The default Omarchy setup builds the Rust cursor and glow runtime. Its standard mode uses Wayland layer-shell and read-only mouse events; it does not compile or load the experimental Hyprland C++ bridge.

## Easiest way: let an AI assistant set it up
New to Linux? You don't have to type any commands yourself. If you use an AI coding assistant (Claude Code, Codex, Gemini CLI and similar), paste this to it:

> Set up Kyber Cursor on my Omarchy computer. Read https://raw.githubusercontent.com/s3pp3ku/kyber-cursor/main/AGENTS.md first and follow it exactly. Explain things to me in plain English, and ask me before anything that needs my password, restarts the computer or the desktop shell, or changes my top bar.

What happens next, in plain words:
1. The assistant asks, then downloads Kyber from GitHub and adds it to Omarchy's plugin list (the normal way to install a plugin). No password.
2. It shows you a short **preview** of what it is about to do, and waits for your OK.
3. It builds the Rust cursor and helper. The first build fetches Rust code dependencies from crates.io; no sound files are fetched separately.
4. It then goes through a short checklist **with you**, asking before each item:
   - one command that installs a few helper programs (needs your password, so *you* run it);
   - an **optional** permission for the click glow (explained below);
   - one restart of the desktop shell, only if you chose the optional Kyber button on your top bar.
5. It checks that everything works and tells you how to try it: hold the left mouse button for a second.

The assistant is told never to run `sudo` for you, to use only the included audio, and to stop and ask if anything looks odd. You can say no to anything and still keep the cursor.

**The one permission worth understanding:** the click glow needs to know when you press a mouse button, anywhere on screen. Linux only tells that to programs you have put in a group called `input`. The catch is that the permission belongs to *you*, so any program you run could then also read your keyboard and mouse input. If you are not comfortable with that, skip it: you still get the cursor in your theme's color, and only the glow, trail and sounds stay off. (More in "Is this safe?" below.)

Prefer to do it yourself? Use the Install section below, or run `kyber setup` from the plugin folder. It shows a preview first and changes nothing until you add `--yes`.

## Is this safe? (plain English)

**What is it?** Two parts. (1) A cursor theme: just picture files, like any other cursor pack. (2) A small helper program that draws the glow when you hold a mouse button.

**Why does the glow need the "input group"?** On Linux, mouse and keyboard signals arrive in a locked room (`/dev/input`). Only people on a short list, the `input` group, may listen at that door. The helper has to know "is a mouse button held *anywhere on screen*?", and no normal app is told that, so it has to listen at the door. Adding yourself to the list is that permission.

**What's the honest catch?** The list applies to *you*, so any program you run could also listen at that door, **including your keyboard**. That is how Linux works, not something this plugin does, but it is a real trade-off, so don't add yourself unless you are comfortable with it.

**What does the helper actually do?** The Rust helper reads mouse-button events from `/dev/input` without grabbing the devices. It ignores keyboard events and does not block or change clicks. Its click-through layer-shell window draws the glow and trail.

**Can I skip the group?** Yes. Without it you still get the cursor in your theme's accent. The click glow, trail and sounds simply stay off. The installer detects this and doesn't start the helper.

**Could a command here be hiding something?** The installer never runs `sudo`. The one `sudo` command is only *printed* for you to read and run yourself. Cargo fetches library source during the first build; Kyber makes no network requests at runtime, and it never fetches audio. `./uninstall.sh` reverses the install and keeps your sound overrides.

At runtime, the helper uses local Wayland and Hyprland sockets only.

## Install on Omarchy

Omarchy downloads the plugin files. Kyber's setup then builds the Rust cursor
and background helper from that checkout. Adding the plugin does not run setup
or install software with your password.

1. Open a terminal with **Super + Return** and add the plugin:

   ```sh
   omarchy plugin add https://github.com/s3pp3ku/kyber-cursor --yes
   ```

2. Preview setup (this changes nothing):

   ```sh
   ~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber setup
   ```

3. If the preview looks right, run setup:

   ```sh
   ~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber setup --yes
   ```

If Rust tools or system libraries are missing, setup prints the Arch command to
install them. It needs your password, so run that command yourself in the
terminal, then run the same setup command again. Kyber never runs `sudo` for
you. On the first build Cargo downloads code dependencies from crates.io; this
can take several minutes. No sound files are downloaded separately.

The cursor works without the optional click glow. That feature needs membership
in the Linux `input` group, which is a computer-wide permission; setup explains
the trade-off and leaves the choice to you. The small bar button is optional.
If you want it, add `--bar` to both setup commands above: preview first, then
run the version with `--yes`. This changes the bar and may require a
desktop-shell restart.

To remove Kyber, run the plugin's `uninstall.sh`; it restores the normal cursor.

## Updating
```
omarchy plugin update io.github.s3pp3ku.kyber-cursor
kyber doctor
```
Omarchy updates the plugin files; it does not rerun setup. If an update changes
Kyber's helper or install files, run the setup command again with the same
options you chose before. `kyber doctor` checks the current installation.

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.

## Use
```
kyber apply        # rebuild in the current accent and switch to it
kyber off          # back to Adwaita (the glow stops too)
kyber sound        # cycle sound mode: all -> ignite -> mute
kyber trail        # toggle the drag trail (or `kyber trail on` / `kyber trail off`)
kyber color theme|custom|random   # where the glow color comes from
kyber color #ff7a00   # a color of your own (switches to custom)
kyber random       # roll a new random color now (switches to random)
kyber volume 40    # sound volume, 0-100
kyber config       # every setting as one line of JSON
kyber status
```
### The bar button
Add it to your bar with `omarchy bar put io.github.s3pp3ku.kyber-cursor` (the installer doesn't touch your bar). Left-click opens the options popup, middle-click cycles the sound, right-click turns Kyber on or off.

Bind the sound and trail toggles to keys if you like. In Omarchy's Lua bindings (`~/.config/hypr/bindings.lua`):
```lua
o.bind("F10", "Cycle Kyber sound", "kyber sound")
o.bind("F11", "Toggle drag trail", "kyber trail")
```

## Sounds

The Rust runtime uses the selected recordings for power-up, power-down, clash,
movement and held-hum sounds.

The package uses mono PCM16 / 44.1 kHz WAVs from
[`assets/sounds/`](assets/sounds/README.md). No Downloads folder, audio generator,
network connection, Python, or ffmpeg is needed on the user's computer. The
original MP3s were decoded with a common gain for headroom.

```sh
kyber sounds default                     # copy the included recordings
kyber sounds preview                     # listen to the local sound pack
kyber sounds list                        # list local sound files
kyber sounds add FILE ignite|off|clash|swipe|hum
```

Additional local WAV / MP3 / Ogg / FLAC recordings can override each role.
`sounds default` fills missing bundled files and does not replace existing
personal recordings.

| Sound folder | Role |
| --- | --- |
| `sounds/ignite/` | One power-up per hold |
| `sounds/off/` | Power-down on release in All mode |
| `sounds/clash/` | Double-click clashes |
| `sounds/swipe/` | Movement swipes while holding the blade |
| `sounds/hum/` | Recorded hum after the power-up |

On Omarchy, local overrides live under `~/.config/kyber-cursor/`. This page
covers the Omarchy / Hyprland integration only.

The owner selected these third-party recordings for this noncommercial public
project. Their original ownership and redistribution permissions are unverified, and
**the code's MIT license does not cover the recordings**. File provenance and
hashes are in [`assets/sounds/manifest.json`](assets/sounds/manifest.json).

## Troubleshooting
Run `kyber doctor`. It checks the dependencies, the `input` group, the glow service, the hooks and the sounds, and prints the fix for anything missing. It only reads; it never changes your system. If you use an AI coding agent to set this up, point it at `AGENTS.md`.

## How it works, and its limits
On Hyprland 0.56, `hyprctl setcursor` only changes what *future* cursor requests resolve to; the image on screen stays until an app re-requests its cursor, which never happens mid-click. So the ignition is not a cursor swap. A Rust Wayland layer-shell overlay (click-through, empty input region) draws the glow *under* the real cursor and follows it. The real pointer is never touched.

- The glow and trail are made for dark and mid-tone backgrounds. On very light pages they are subtler (a defined but faint halo), so they don't look like the demo there.
- The overlay doesn't know the current cursor shape, so the glow also appears around an I-beam during a text drag.
- Right after a theme or color change, the rebuild takes 1-2 s. The glow updates at once, but the real pointer image refreshes the next time you move onto another window or the bar. That is how Hyprland handles cursor themes, and it affects every cursor theme, not just Kyber.
- The glow only follows the cursor's logical position. On unusual scaling setups the overlay can be slightly off; set `CURSOR_OVERLAY_SCALE` (e.g. `1.05`) in the service environment to fine-tune.
- Tested on one machine (Hyprland 0.56, 1.6x scaling). Reports from other setups are welcome.

## Safety
- The listener opens `/dev/input/event*` **read-only and without grabbing** them (no `EVIOCGRAB`), so clicks still reach Hyprland and apps unchanged. It counts button presses only: no keys, no coordinates from the device, no network.
- The overlay can never take a click (empty input region).
- It reads a 20x20 pixel patch of your screen next to the cursor, via `grim`, on each press to choose the dark or light glow. The patch is averaged to a single brightness number in memory and is never saved or sent anywhere.
- The service runs with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=read-only`.
- The `input` group lets any program you run read raw input events from all your input devices (including keyboards). Only add yourself if you accept that.

## Customizing
The Rust renderer, timings, trail and audio implementation live under
`rust/crates/`. The older Python sources remain for comparison and migration
reference; the standard installer runs the Rust CLI and helper.

## License

Source code: MIT. The selected third-party sound recordings are separate; see
[the audio notice](assets/sounds/README.md).

*Kyber Cursor is an independent, fan-made project created with the help of AI tools. It is not affiliated with or endorsed by Lucasfilm or Disney.*
