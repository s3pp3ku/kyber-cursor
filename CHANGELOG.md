# Changelog

## 1.5.0 — Aim, sparks and a steadier glow

**Updating from 1.4 or earlier:** this release changes the Rust runtime, so
updating the plugin files is not enough. Run
`omarchy plugin update io.github.s3pp3ku.kyber-cursor`, then run the setup
command again with the options you chose before (it rebuilds `kyber` and
`kyber-ignite`), restart the shell once (`omarchy-restart-shell`), and finish
with `kyber doctor`.

### New

- **Aim** (off by default): the overlay runtime draws the whole cursor, and the
  blade turns to face the direction you move. It keeps that angle when you stop.
  The idea and the heading maths come from otlet's Worms cursor (see Credits).
  `kyber aim on|off|toggle`, plus a switch in the popup.
  - The glow is drawn with the blade in one picture, so the glow and the blade
    never drift apart.
  - `kyber aim mirror on|off` (default on) flips the artwork so the lit edge
    stays on the same side as the blade turns.
  - To keep the locked-together glow without the turn, write `off` to
    `~/.config/kyber-cursor/aim-turn`. The blade then stays in the normal
    pointer pose.
  - The cursor shows above bar popups. The compositor pointer comes back on
    `kyber aim off` and on any clean exit.
  - Aim needs the overlay runtime. The XCursor theme is a fixed image, so on
    that path `kyber aim on` says so and stays off.
- **Sparks** fly off the drag trail. Kyber draws them itself, so you do not
  need Plume. Most burn from orange to deep red like hot metal, and some wear
  the glow color (`kyber sparks colors glow` uses the glow color for all).
  `kyber sparks life MS` (default 850) and `kyber sparks gap PX` (default 20)
  change them.
- **Effects presets:** Full (trail, sparks, all sounds; the default), Minimal
  (trail, no sparks, ignite sounds) and Basic (only the glow, no sound).
  `kyber effects full|minimal|basic`, or middle-click the bar button.

### Changed

- The pointer stands more upright: the blade axis is 62 degrees (was 53.4).
  Cursor files rebuild by themselves on the next `kyber apply`.
- The trail is the comet only. The Ribbon, Beads, Echoes and Bubbles looks and
  the Plume bridge are removed. `kyber style` is now `comet|none` and
  `kyber sparks` is `sparks|off`.
- The popup is simpler: the on/off switch sits under the header, and the
  "More settings" fold is gone. The mouse wheel over a slider scrolls the panel
  and does not change the value.
- grab and grabbing cursors use the plain blade, so no glow shows behind a hand.
- The glow color fits the blade color better. Dark backgrounds keep the real
  hue, and light pages get a deeper, more saturated halo (yellow turns gold).
  Glowing edges on light pages get a thin outline in a deeper shade.
- The held glow leads the pointer by 20 ms to match the hardware pointer.
  Change it in `~/.config/kyber-cursor/glow-lead-ms`.

### Fixed

- The glow froze for about a quarter of a second every 5 seconds (an input
  device scan ran on the drawing loop).
- A slow memory leak in kyber-ignite (graphics buffers were not freed).
- A drag that moved to another screen left the glow and trail behind.
- White pages got the dark-background glow, because the check sampled the
  blade and not the page.
- `kyber apply` did not take the cursor theme back after another tool (such
  as the Omarchy cursor picker) pointed it at a different build.
- Trails were blurry on scaled (HiDPI) screens, and the glow overlay's trail
  ignored the glow color.

### Credits

- The trail ideas are adapted from Plume by Andrei C. Popa
  (https://github.com/qempexe/plume, MIT). Aim ports the idea and heading
  logic of otlet's Worms cursor. See THIRD_PARTY_NOTICES.md.

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
