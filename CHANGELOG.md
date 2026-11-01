# Changelog

## Unreleased — aim (the blade faces your movement)

- New **Aim** mode, off by default: the blade turns to face the pointer's
  direction of travel and keeps its angle when the pointer stops (the
  Worms-cursor idea, maths ported from `otlet.worms-cursor`). The turn is
  smoothed over 8 px of travel with a 3° deadband and a 720°/s slew, so it
  reads as aiming rather than snapping. `kyber aim on|off|toggle`, a switch
  under "More settings", state in `<settings>/aim-mode`.
- Aim lives in the native overlay runtime only: the XCursor theme is a static
  bitmap and cannot rotate, so `kyber aim on` reports that and leaves the
  switch off on the legacy path. The overlay caches the whole cursor body once
  per (color, size), draws it turned about its tip every frame, and hides the
  compositor pointer only after the replacement is on screen. The pointer comes
  back on `kyber aim off`, SIGTERM/SIGINT, a panic and any other clean exit;
  SIGKILL cannot be caught and is documented.
- `kyber aim mirror on|off` (default on) reflects the artwork across the
  pointer axis whenever the unmirrored glyph would turn its lit edge away, so
  the light keeps reading from the same side as the blade spins.
- Idle cost: the loop ticks at 60 fps while aim is on but only uploads a frame
  when the picture can change, and damage stays around the cursor.
- `kyber config` gained `"aim"` and `"aimMirror"`; `kyber status` lists aim.

## Unreleased — lightsaber sparks

- Sparks now fly off the drag trail, drawn by Kyber itself on every platform
  (kyber-core), so Plume is no longer needed for them. They are hard cutting
  sparks: short streaks that shoot off the blade and stall within a few pixels.
  A quarter wear the glow color; the rest burn from orange to deep red like hot
  metal (`kyber sparks colors glow` keeps them all in the glow color).
- Over light backgrounds the glowing edges get a crisp outline in a deeper
  shade of the glow color, so they stand out from white pages.
- One effects switch replaces the trail switch in the popup: Full (trail,
  sparks, all sounds; the default), Minimal (trail, no sparks, ignite sounds)
  and Basic (just the glow, silent). `kyber effects full|minimal|basic`;
  middle-clicking the bar button cycles them.
- New settings under "More settings": how long sparks last
  (`kyber sparks life`, default 850 ms) and how close together they fly
  (`kyber sparks gap`, default 20 px), plus the separate sound modes, the trail
  switch and the other looks (Ribbon, Beads, Echoes, no trail; Bubbles or no
  particles). Comet + Sparks is the default.
- Fixed: the Beads and Echoes looks were sized wrongly; the glow overlay's
  trail ignored the glow color; trails were blurry on scaled (HiDPI) screens.
- Credit: the trail looks are adapted from Plume by Andrei C. Popa
  (https://github.com/qempexe/plume, MIT). See THIRD_PARTY_NOTICES.md.

## 1.4.0 — Linux Rust runtime source

- Published the Linux/Hyprland Rust runtime, CLI, portable rendering core, and
  native cursor bridge source.
- Documented that the Rust runtime is experimental and that fresh novice setup
  still uses the established Python helper; a Rust-first installer is not yet
  available.
- Kept this update scoped to Linux. Desktop implementations for other systems
  are not part of this release.

## 1.3.0 — Omarchy installation update

- Simplified the novice install instructions into one Omarchy plugin flow with
  a preview before setup, clear optional glow/bar steps, and user-run dependency
  installation when needed.
- Updated the plugin manifest to version 1.3.0 and made its short description
  focus on the cursor and trail.
- The separate audio section documents the selected recordings and their
  provenance; audio assets are not licensed by the source-code license.

## Selected sound pack — 2026-10-17

- Replaced generated default effects and the oscillator hum with eleven selected
  recordings. The current Omarchy helper uses power-up, power-down and clash;
  other prepared roles are retained for future runtime work.
- Added the recorded hold with a shared crossfade loop and release envelope.
- Changed setup and `sounds default` to copy the included recordings. The legacy
  installer backs up the old generated files; custom recordings are kept.
- Updated setup, sound documentation and provenance notes. Source code remains
  MIT; third-party recordings have a separate notice and are not covered by the
  code license.

## 1.2.0
- `kyber setup`: one command for the non-privileged setup. It previews by default and only acts with `--yes`; it never uses `sudo` or restarts anything, and leaves a checklist for the person. `AGENTS.md` is rewritten as a plain-English, conservative guide so a user can just point an AI assistant at it; the README has a copy-and-paste prompt.
- Updating from 1.0 or 1.1: run `omarchy plugin update io.github.s3pp3ku.kyber-cursor`, restart the shell once (`omarchy-restart-shell`) so the bar button loads, then `kyber doctor`. See the README's "Updating" section.
- Sounds: an original synthesized pack is generated at install (`kyber sounds default`), with `kyber sounds add|list|preview` for your own clips. No recordings are bundled and audio files are git-ignored.
- `kyber doctor` checks the setup and prints the fix for anything missing; `AGENTS.md` guides AI coding agents through setup.
- The demo video is now silent.
- Faster color changes: the cursor build runs on several cores (about 2.1 s down to about 0.9 s, byte-identical output), and in Random mode the next color is built in the background, so "Roll a new color" just swaps it in (about 0.13 s).
- The sound volume now follows a perceptual (cubic) curve. Before, a low setting such as 18% was still loud, because the percentage was a plain amplitude. If you set a volume in 1.1.0, raise it (about 55 gives what 18 used to).
- Glow color has three sources: Theme, Custom and Random, as three buttons in the popup (`kyber color theme|custom|random`, `kyber random`).
- Random picks only vivid, bright colors, a clearly different hue each time, and re-rolls on a theme change or at boot, never on a shell reload or plugin update.
- Settings from 1.1.0 carry over unchanged.

## 1.1.0
- Bar button with an options popup: color picker, sound mode, volume, drag trail and on/off.
- Custom glow color (`kyber color`), sound volume (`kyber volume`) and `kyber config`.
- Light backgrounds: the drag trail now uses a deeper shade so it stays visible.
- The installer skips the click-glow service when you are not in the `input` group.

## 1.0.0
- Kyber cursor theme: a dark spearhead pointer with a hilt, in the Omarchy accent. Full cursor family (pointer, links, busy, text, resize, grab and the rest), rebuilt on every `omarchy theme set`.
- Click glow: while a mouse button is held the blade ignites with side beams, haze, a slow pulse and a drag trail. Adapts to light and dark backgrounds.
- Optional sounds: drop your own clips in `~/.config/kyber-cursor/sounds/{ignite,off,clash}`. `kyber sound` cycles all / ignite / mute.
