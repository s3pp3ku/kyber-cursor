#!/usr/bin/env bash
# Removes everything install.sh added and restores the Adwaita cursor.
# Your sound clips in ~/.config/kyber-cursor/sounds are kept.
set -euo pipefail
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
"$HOME/.local/bin/kyber" off 2>/dev/null || true
systemctl --user disable --now kyber-ignite.service 2>/dev/null || true
rm -f "$CFG/systemd/user/kyber-ignite.service" "$HOME/.local/bin/kyber" \
  "$CFG/omarchy/hooks/theme-set.d/kyber-cursor.sh" "$CFG/omarchy/hooks/post-boot.d/kyber-cursor.sh" \
  "$CFG/kyber-cursor/active" "$CFG/kyber-cursor/sound-mode"
rm -rf "$HOME/.local/share/icons/Omarchy-Kyber" "$HOME/.local/share/icons/Omarchy-Kyber-Ignited" \
  "$HOME/.local/share/icons/.kyber-builds"
systemctl --user daemon-reload
echo "Kyber removed. (Sounds kept in $CFG/kyber-cursor/sounds.)"
