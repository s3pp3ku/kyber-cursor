# Contributing

Thanks for helping. A few ground rules keep the project safe to share:

- The owner selected the existing clips in `assets/sounds/` for this public,
  noncommercial project, including the recorded hum. Keep the manifest and
  separate audio notice with packages. The recordings have unverified
  ownership/redistribution permission and are not licensed by the code's MIT
  license. Do not add other third-party media or remove the rights notice.
- Keep the installer opt-in, free of `sudo` and free of downloads.
- Before opening a pull request, run `kyber doctor` and, for build changes,
  compare the output of `lib/build.py` before and after (it should stay
  byte-identical unless the change is meant to alter the look).
- Tell us what you tested on (Hyprland and Omarchy versions, display scale).

## Handy when hacking on Kyber
- After editing `lib/ignite-listener`, restart the glow helper: `systemctl --user restart kyber-ignite`.
- Changes to the bar button's QML (`KyberWidget.qml`, `Panel.qml`) need a shell restart: `omarchy-restart-shell`. Hot reload does not replace already-compiled components.
