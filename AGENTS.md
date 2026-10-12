# Setting up Kyber Cursor (for AI coding agents)

You are helping someone install or fix Kyber Cursor on their Omarchy / Hyprland
machine. Read this first. The user's machine is theirs: ask before anything that
changes system configuration, and never run `sudo` yourself.

## Install

1. Run `kyber doctor` if it exists, otherwise `./install.sh --yes` is the
   installer. The installer lists every step and changes nothing outside the
   user's home folder. It never uses sudo and never downloads anything.
2. If dependencies are missing, show the user the pacman command and let them
   run it. Do not run it for them without asking.
3. The click glow needs the user to be in the `input` group. That permission
   lets any program they run read raw input events, including the keyboard, so
   explain this plainly and let the user decide. Without it Kyber still works
   (cursor and color); only the glow, trail and sounds stay off. The user runs
   `sudo usermod -aG input "$USER"` themselves and reboots.
4. Verify: `kyber doctor` should print "Everything checks out." and
   `kyber status` should show the glow service as active.

## Sounds: important

- Kyber bundles **no recordings**. `kyber sounds default` (run by the
  installer) generates a small original pack from `lib/make_sounds.py`.
- To use the user's own clips: `kyber sounds add FILE ignite|off|clash`.
- Do **not** download, copy or add film, TV, game or music audio (lightsaber
  clips from the movies, soundboard sites and the like). That is copyrighted,
  it must never be committed to this repository, and pull requests that add
  it are declined. Point users at their own recordings or CC0 / royalty-free
  libraries (check each file's license).

## Useful commands

| Command | What it does |
|---|---|
| `kyber doctor` | checks everything and prints the fix for each problem |
| `kyber apply` / `kyber off` | build and use the cursor / go back to Adwaita |
| `kyber color theme\|custom\|random\|#rrggbb` | where the glow color comes from |
| `kyber sound`, `kyber trail`, `kyber volume N` | sound mode, drag trail, volume |
| `kyber sounds default\|add\|list\|preview` | manage the sound pack |

## Facts that save time

- Settings live in `~/.config/kyber-cursor/`; the cursor themes are
  `Omarchy-Kyber` and `Omarchy-Kyber-Ignited` in `~/.local/share/icons`.
- The glow is a systemd **user** service, `kyber-ignite`. After editing
  `lib/ignite-listener`, restart it: `systemctl --user restart kyber-ignite`.
- After a color or theme change the glow updates at once, but the real pointer
  image only refreshes when the pointer next crosses onto another window or the
  bar. That is a Hyprland limit, not a bug.
- Changes to the bar button's QML need a shell restart (`omarchy-restart-shell`);
  hot reload does not replace already-compiled components.
- Remove everything with `./uninstall.sh`.
