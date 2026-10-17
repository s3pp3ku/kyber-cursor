#!/usr/bin/env bash
# Build and install Kyber's Linux Rust runtime. This script never runs sudo.
#   ./install.sh            install
#   ./install.sh --yes      install without the confirmation prompt
set -euo pipefail

PLUGIN_DIR=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)
STATE="${XDG_CONFIG_HOME:-$HOME/.config}/kyber-cursor"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
UNIT="$CFG/systemd/user/kyber-ignite.service"
HOOKS="$CFG/omarchy/hooks"
BIN="$HOME/.local/bin"
RUNTIME="$HOME/.local/share/kyber-cursor/rust"

missing=()
for command_name in cargo rustc pkg-config hyprctl grim pw-play; do
  command -v "$command_name" >/dev/null || missing+=("$command_name")
done
if command -v pkg-config >/dev/null; then
  for library in alsa gbm libdrm wayland-client xkbcommon; do
    pkg-config --exists "$library" || missing+=("$library development files")
  done
fi
if ((${#missing[@]})); then
  printf 'Missing build/runtime requirements: %s\n' "${missing[*]}" >&2
  echo "Arch/Omarchy: sudo pacman -S --needed rust pkgconf alsa-lib mesa libdrm wayland libxkbcommon grim pipewire" >&2
  exit 3
fi

cat <<MSG
Kyber Cursor will:
  1. build the Rust cursor and glow runtime from the included source
     (the first build downloads Rust crate dependencies from crates.io)
  2. install its two Rust programs under $RUNTIME
  3. install a user service and Omarchy theme/boot hooks
  4. build and apply the cursor, then copy the included recordings
  5. start the click glow if your session can read mouse events

It will not use sudo, change system files, alter your top bar, or restart
your desktop. Undo everything with ./uninstall.sh. Your sound overrides are kept.
MSG
if [[ ${1:-} != --yes ]]; then
  read -r -p "Continue? [y/N] " answer
  [[ $answer == [yY]* ]] || exit 1
fi

mkdir -p "$BIN" "$RUNTIME" "$STATE" "$(dirname "$UNIT")" \
  "$HOOKS/theme-set.d" "$HOOKS/post-boot.d"
echo "Building Kyber's Rust runtime. The first build can take several minutes."
cargo build --release --locked --manifest-path "$PLUGIN_DIR/rust/Cargo.toml" \
  -p kyber-cli -p kyber-ignite
install -m 755 "$PLUGIN_DIR/rust/target/release/kyber" "$RUNTIME/kyber"
install -m 755 "$PLUGIN_DIR/rust/target/release/kyber-ignite" "$RUNTIME/kyber-ignite"

systemctl --user stop kyber-ignite.service 2>/dev/null || true
NATIVE_DROPIN="$CFG/systemd/user/kyber-ignite.service.d/90-rust-native.conf"
if [[ -f $NATIVE_DROPIN ]]; then
  backup="$HOME/.local/share/kyber-cursor/backups/native-runtime-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "$backup"
  cp -a "$NATIVE_DROPIN" "$backup/90-rust-native.conf"
  rm -f "$NATIVE_DROPIN"
fi
ln -sfn "$PLUGIN_DIR/bin/kyber" "$BIN/kyber"
install -m 644 "$PLUGIN_DIR/lib/kyber-ignite.service" "$UNIT"
for hook_dir in theme-set.d post-boot.d; do
  sed "s|@PLUGIN@|$PLUGIN_DIR|g" "$PLUGIN_DIR/lib/hook.sh" > "$HOOKS/$hook_dir/kyber-cursor.sh"
  chmod 755 "$HOOKS/$hook_dir/kyber-cursor.sh"
done
printf 'overlay\n' > "$STATE/native-runtime"
"$BIN/kyber" apply
"$BIN/kyber" sounds default

systemctl --user daemon-reload
systemctl --user enable kyber-ignite.service
if id -nG | tr ' ' '\n' | grep -qx input && compgen -G "/dev/input/event*" >/dev/null && [[ -r $(ls /dev/input/event* 2>/dev/null | head -1) ]]; then
  systemctl --user start kyber-ignite.service
else
  echo "The cursor is installed. The click glow starts after mouse permission is enabled and the computer is restarted."
fi
echo "Done. Run 'kyber doctor' to check the installation."
