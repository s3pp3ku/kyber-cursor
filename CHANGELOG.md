# Changelog

## 1.2.0
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
