# Linux Rust runtime

This directory contains Kyber's Linux / Hyprland Rust implementation: the
cursor and glow renderer, settings CLI, audio daemon, compositor frame watcher,
and the Rust trail renderer called by the Hyprland bridge in `../native-cursor`.
These notes cover the Linux/Hyprland integration on Omarchy.

## Current install status

The Rust runtime is running on the maintainer's Omarchy machine. The public
plugin's novice `kyber setup` path still installs the established Python helper
and does not build or activate this Rust runtime. A complete Rust-first
installer and a supported Hyprland plugin build flow for other machines are
still future work. Do not describe a fresh plugin install as Rust-only yet.

The Rust CLI is used by the existing command wrapper only when the managed
`~/.config/kyber-cursor/native-runtime` marker and Rust binaries are present.
Without that opt-in runtime, existing Python commands remain the fallback.
The Linux compositor bridge uses Hyprland's internal plugin interfaces and
must be built against a compatible local Hyprland version. It is not a generic
desktop adapter.

## Source layout

- `crates/kyber-core`: Rust cursor rendering, settings, trail math and bundled
  audio pack.
- `crates/kyber-cli`: Linux cursor, color, sound, trail and status commands.
- `crates/kyber-ignite`: Linux runtime and compositor/audio coordination.
- `crates/kyber-native-ffi`: C ABI for rendering trails in the bridge.
- `../native-cursor`: Hyprland plugin source that forwards local pointer events
  and presents the synchronized cursor and trail.

The selected PCM recordings are embedded in the Rust binaries from
[`../assets/sounds/`](../assets/sounds/README.md). Keep that notice and the
manifest alongside them; their redistribution permission has not been
verified, and the MIT source license does not cover the audio files.

## Build the Rust components

From the repository checkout, with Rust/Cargo, a C++23 compiler, pkg-config,
and development headers matching the installed Hyprland version:

```sh
cd rust
cargo build --release --locked -p kyber-cli -p kyber-ignite -p kyber-native-ffi
cd ../native-cursor
make
```

Building does not install the service or load the compositor plugin. The
maintainer's activation helper is machine-specific and is intentionally not
presented as a general installer. Do not manually load the plugin into a live
desktop without a compatible build and a recovery plan.
