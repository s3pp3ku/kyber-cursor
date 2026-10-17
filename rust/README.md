# Linux Rust runtime

This directory contains Kyber's Linux / Hyprland Rust implementation: the
cursor and glow renderer, settings CLI, audio daemon, compositor frame watcher,
and the Rust trail renderer called by the Hyprland bridge in `../native-cursor`.
These notes cover the Linux/Hyprland integration on Omarchy.

## Current install status

The standard Omarchy setup path builds and installs the Rust CLI and glow
runtime from this checkout. It uses Wayland layer-shell for the overlay and
read-only `/dev/input` mouse events; it does not compile or load the optional
Hyprland C++ bridge. The Rust CLI is selected by the existing `bin/kyber`
wrapper when the installer-managed `native-runtime` marker and binaries are
present. The shell script remains responsible for setup preview and
confirmation.

This is a Linux / Wayland implementation, not a portable desktop adapter. The
optional compositor bridge uses Hyprland internal interfaces and must be built
against a compatible local Hyprland version.

## Source layout

- `crates/kyber-core`: Rust cursor rendering, settings, trail math and bundled
  audio pack.
- `crates/kyber-cli`: Linux cursor, color, sound, trail and status commands.
- `crates/kyber-ignite`: Linux input, overlay and audio runtime.
- `crates/kyber-native-ffi`: C ABI for rendering trails in the bridge.
- `../native-cursor`: Hyprland plugin source that forwards local pointer events
  and presents the synchronized cursor and trail.

The selected PCM recordings are embedded in the Rust binaries from
[`../assets/sounds/`](../assets/sounds/README.md). Keep that notice and the
manifest alongside them; their redistribution permission has not been
verified, and the MIT source license does not cover the audio files.

## Build the Rust components

From the repository checkout, with Rust/Cargo, pkg-config and the ALSA, GBM,
DRM, Wayland and xkbcommon development libraries:

```sh
cd rust
cargo build --release --locked --workspace
cargo test --workspace --locked
```

The regular installer builds only `kyber-cli` and `kyber-ignite`. Building the
workspace also checks the optional native bridge support libraries, but does
not install the service or load a compositor plugin.
