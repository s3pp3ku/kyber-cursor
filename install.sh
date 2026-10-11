#!/usr/bin/env bash
# Opt-in installer for Kyber Cursor. Nothing is changed until you run this,
# and every step is listed first. No sudo, no network.
#   ./install.sh            install
#   ./install.sh --yes      install without the confirmation prompt
set -euo pipefail

PLUGIN_DIR=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)
STATE="${XDG_CONFIG_HOME:-$HOME/.config}/kyber-cursor"
UNIT="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/kyber-ignite.service"
HOOKS="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/hooks"
BIN="$HOME/.local/bin"

missing=()
/usr/bin/python3 -c 'import gi, cairo, PIL, cairosvg, evdev
gi.require_version("Gtk","4.0"); gi.require_version("Gtk4LayerShell","1.0")' 2>/dev/null || missing+=("python: gobject, cairo, pillow, cairosvg, evdev + gtk4 + gtk4-layer-shell")
command -v grim >/dev/null || missing+=("grim")
command -v hyprctl >/dev/null || missing+=("hyprctl (Hyprland)")
[[ -e /usr/lib/libgtk4-layer-shell.so ]] || missing+=("/usr/lib/libgtk4-layer-shell.so (gtk4-layer-shell)")
if ((${#missing[@]})); then
  printf 'Missing: %s\n' "${missing[@]}" >&2
  echo "Arch: sudo pacman -S --needed python-gobject python-cairo python-pillow python-cairosvg python-evdev gtk4 gtk4-layer-shell grim pipewire" >&2
  exit 1
fi

cat <<MSG
Kyber Cursor will:
  1. link  $BIN/kyber  ->  $PLUGIN_DIR/bin/kyber
  2. add   $UNIT  (systemd --user, the click glow) and start it
  3. add   theme-set + post-boot hooks in $HOOKS  (recolor on theme change)
  4. create $STATE/sounds/{ignite,off,clash}/  (empty: drop your own clips in)
  5. build the cursor in your current accent and switch to it
Undo everything with ./uninstall.sh.
MSG
if [[ ${1:-} != --yes ]]; then
  read -r -p "Continue? [y/N] " ans
  [[ $ans == [yY]* ]] || exit 1
fi

mkdir -p "$BIN" "$STATE/sounds"/{ignite,off,clash} "$(dirname "$UNIT")" "$HOOKS/theme-set.d" "$HOOKS/post-boot.d"
ln -sfn "$PLUGIN_DIR/bin/kyber" "$BIN/kyber"
sed "s|@PLUGIN@|$PLUGIN_DIR|g" "$PLUGIN_DIR/lib/kyber-ignite.service" > "$UNIT"
for d in theme-set.d post-boot.d; do
  sed "s|@PLUGIN@|$PLUGIN_DIR|g" "$PLUGIN_DIR/lib/hook.sh" > "$HOOKS/$d/kyber-cursor.sh"
  chmod +x "$HOOKS/$d/kyber-cursor.sh"
done
"$PLUGIN_DIR/bin/kyber" apply
systemctl --user daemon-reload
if id -nG | tr ' ' '\n' | grep -qx input; then
  systemctl --user enable --now kyber-ignite.service
else
  # without the group the glow helper cannot see button presses and would just
  # fail and restart; the cursor itself works fine without it
  echo "Skipping the click-glow service for now (you are not in the 'input' group)."
fi

if ! id -nG | tr ' ' '\n' | grep -qx input; then
  cat <<MSG

The click glow reads mouse buttons from /dev/input, which needs the 'input'
group. This installer does not use sudo, so run this yourself, then reboot
(a logout is not enough if systemd --user lingers):
    sudo usermod -aG input "$USER"
Then run:  systemctl --user enable --now kyber-ignite.service
Without it you still get the cursor in your theme's accent; only the click
glow, trail and sounds stay off.
MSG
fi
echo "Done. Try holding a mouse button. 'kyber status' shows the state."
