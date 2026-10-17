# Contributing

Thanks for helping. A few ground rules keep the project safe to share:

- The owner selected the existing clips in `assets/sounds/` for this public,
  noncommercial project, including the recorded hum. Keep the manifest and
  separate audio notice with packages. The recordings have unverified
  ownership/redistribution permission and are not licensed by the code's MIT
  license. Do not add other third-party media or remove the rights notice.
- Keep setup opt-in and free of `sudo`. On the first build, Cargo downloads
  Rust library dependencies from crates.io; do not add audio downloads.
- Before opening a pull request, run `cargo test --workspace --locked` from
  `rust/` and `kyber doctor`. For rendering changes, inspect generated output
  against the existing cursor appearance.
- Tell us what you tested on (Hyprland and Omarchy versions, display scale).

## Handy when hacking on Kyber
- After editing `lib/ignite-listener`, restart the glow helper: `systemctl --user restart kyber-ignite`.
- Changes to the bar button's QML (`KyberWidget.qml`, `Panel.qml`) need a shell restart: `omarchy-restart-shell`. Hot reload does not replace already-compiled components.
