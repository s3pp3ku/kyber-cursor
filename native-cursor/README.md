# Hyprland cursor bridge

This Linux/Hyprland C++ plugin presents Rust-generated cursor frames and trail
rasters. Hyprland positions the cursor image itself, keeping the glow aligned
with the pointer. The bridge forwards button and movement events over a local
Unix socket to the Rust audio runtime; it does not read `/dev/input` or register
keyboard events.

The bridge calls Hyprland's internal C++ plugin interfaces. Its ABI can change
between Hyprland versions, so build against the headers for the exact local
version. This code is experimental and is not a general Hyprland plugin
installer or a supported adapter for other compositors.

## Build

Requirements: a C++23 compiler, `pkg-config`, Hyprland development headers,
Rust/Cargo, and the Rust source in `../rust`.

```sh
cd ../rust
cargo build --release --locked -p kyber-native-ffi
cd ../native-cursor
make FRAME_DIR="$HOME/.cache/kyber-cursor/native"
```

This produces the plugin locally; it does not load it into the running desktop.
Only load a plugin built for the installed Hyprland version, and keep a way to
unload it or restart the shell if it fails.

The Rust daily runtime uses `kyber-ignite --native-runtime FRAMES PLUGIN` and
`rust/systemd/native.conf`. A fresh plugin setup does not install or activate
this experimental path. See [`../rust/README.md`](../rust/README.md) for the
current migration status and dependencies.
