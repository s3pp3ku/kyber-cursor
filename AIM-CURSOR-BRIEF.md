# Brief: an "Aim" (worm) cursor option for Kyber

Work only in this copy: `~/Projects/kyber-cursor-aim`. Do NOT touch
`~/Projects/kyber-cursor`, `~/.local/share/kyber-cursor`, the installed plugin
at `~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor`, or any running
service. The owner builds and installs himself after looking at the result.

## Goal

The blade (spearhead + hilt, tip = hotspot) **turns to face the direction of
travel** and keeps its angle when the pointer stops — the Worms cursor idea
(`~/.config/omarchy/plugins/otlet.worms-cursor/watch.py`, class `Aim`).

Surface it as:
- `kyber aim on|off|toggle` in `bin/kyber` (+ docs at the top of that file),
  state file `<state>/aim-mode` (`on`/`off`), printed by `kyber config` as
  `"aim":"on"` and by `kyber status`
- a toggle in `Panel.qml` under More settings, driven by the same bar-widget
  `act()` path the other switches use
- default **off**; nothing about the current look changes when it is off

## The hard part, state it plainly in your report

Kyber's pointer is a static XCursor theme, so it cannot rotate. Aim therefore
only exists in the native Linux runtime (`rust/crates/kyber-ignite`, the
`kyber-ignite` daemon that already owns the glow `wlr-layer-shell` overlay):

1. rasterize/reuse the blade artwork once per (accent, size) — the existing
   `kyber_core::render` + `art`/`overlay` path already produces the layers and
   the body SVG; cache it, do not rasterize per frame
2. each frame, draw the whole cursor (body, not just the bloom) at the cursor
   position, rotated about the tip by the aim angle; `tiny_skia` transforms
   over the cached pixmap are fine
3. hide the compositor's pointer while aim is on, and restore it when aim goes
   off or the daemon exits: `hyprctl eval 'hl.config({ cursor = { invisible = true } })'`
   and back to `false` (also do this on SIGTERM/SIGINT and on any fatal error —
   losing the pointer is much worse than losing the effect)
4. render continuously (~60fps) while aim is on, even with no button held;
   today the loop waits on input. Reuse the dmabuf triple-buffer path and
   damage only the cursor's footprint (plus the old one). Keep idle CPU
   modest — no busy loop, no full-screen damage. Trail/sparks still work while
   aim is on.
5. if the runtime is not the native one (legacy XCursor path), `kyber aim on`
   must say so honestly and leave the state off

## Mirroring

The glyph is asymmetric (one lit edge strip, beams that sweep back, an offset
hilt), so a pure rotation flips which side the light is on as it turns. Add
`kyber aim mirror on|off` (state `aim-mirror`, default **on** if it looks
better): mirror the artwork across the pointer axis when the aim points into
the half-plane where the unmirrored glyph would put the lit edge away from the
viewer. Keep the geometry builders transform-based — no duplicated constants.

## Acceptance criteria

- `cd rust && cargo test` passes (there is a `run_tests.sh`; use it if it
  works), `cargo build --release` succeeds
- aim maths unit-tested: turn on movement, hold when still, 3° hysteresis,
  720°/s slew, 350°→10° takes the short way
- a rendered proof, human-inspectable: use `rust/crates/kyber-preview` (see
  `examples/check_frames.rs`) or the `--export-native` / `--demo` paths to
  write PNG frames of the cursor at 0/45/90/135/180/225/270/315 degrees, once
  with mirror off and once on, into `~/Downloads/kyber-aim-preview/<mirror>/`.
  Name them `heading-000.png` … `heading-315.png`. These are what the owner
  judges the work by, so make sure the blade really is in each frame.
- no change to the default look, sounds, or the XCursor build when aim is off
- docs: README + CHANGELOG line, and the `bin/kyber` usage header

## Report back

Files changed, how aim is rendered, test output (verbatim summary), the frame
directory, and anything still open (e.g. what happens when the daemon is
killed with SIGKILL — say it plainly rather than pretending it is handled).
