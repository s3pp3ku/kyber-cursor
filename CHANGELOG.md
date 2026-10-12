# Changelog

## 1.2.0
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
