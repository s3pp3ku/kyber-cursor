# Setting up Kyber Cursor (instructions for an AI assistant)

You are helping a person set up **Kyber Cursor** on their Omarchy (Hyprland)
computer. Assume they know very little about Linux. Talk to them in plain,
friendly English, one step at a time. Never assume they understand a term;
explain it in a few words.

## What Kyber Cursor is (say this to the person first)

"Kyber Cursor changes your mouse pointer into a glowing blade. It uses the color
of your current theme, and when you hold a mouse button it lights up and leaves a
fading trail. Sounds are optional. I'll set it up for you and ask before
anything that needs your password."

## Rules. Follow them exactly.

1. **Never run `sudo` or `pkexec` yourself.** If a step needs it, show the person
   the exact command, explain it in one sentence, and let them run it.
2. **Ask first, in plain words, before:** anything that needs their password;
   restarting the computer; restarting the desktop shell (`omarchy-restart-shell`,
   the top bar flickers for a few seconds); changing their bar layout. Wait for a
   clear yes each time. A yes to one is not a yes to another.
3. **Stay in their home folder.** Do not edit system files. Do not delete anything
   that is not Kyber's.
4. **Do not download or add sound, music, film or game audio from anywhere.**
   Kyber makes its own original sounds. If the person wants other sounds, point
   them to their own recordings or CC0 / royalty-free libraries, and use
   `kyber sounds add FILE ignite|off|clash`. Never add copyrighted clips to this
   repository.
5. If anything looks unexpected, or a command fails in a way you do not
   understand, **stop and tell the person** what happened. Do not improvise fixes.
6. Do not send the person's files, settings or command output anywhere.

## The steps

**Step 1: get the plugin** (skip if `~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor`
already exists):

```
omarchy plugin add https://github.com/s3pp3ku/kyber-cursor --enable
```

Tell the person this adds Kyber to Omarchy's plugin list. It is the normal way to
install an Omarchy plugin.

**Step 2: preview.** This changes nothing:

```
~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber setup
```

Read the preview to the person in your own words and ask: "Shall I go ahead?"
Also ask whether they want a small Kyber button on their top bar (optional; it
gives them a color picker and sound controls).

**Step 3: do it** (only after they say yes):

```
~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber setup --yes
```

Add `--bar` only if they asked for the button. This runs no `sudo`.

- If it says some programs are missing (exit code 3), show the person the
  `sudo pacman ...` line it prints, explain "this installs a few helper
  programs and needs your password, so please run it yourself", and wait. When
  they say it's done, run `kyber setup --yes` again.

**Step 4: the checklist.** `kyber setup` prints what is left. Go through it with
the person, asking before each item:

- **The click glow (optional):** it needs the person to be in a Linux group called
  `input`. Explain honestly: "This lets Kyber notice when you press a mouse
  button. The catch is that the permission applies to you, so any program you run
  could also read your keyboard and mouse input. Only do this if you are
  comfortable with that. If not, skip it: you still get the cursor and its color;
  only the glow, trail and sounds stay off." If they agree, they run
  `sudo usermod -aG input "$USER"` themselves and **restart the computer**.
- **Restart the shell once** (ask first): `omarchy-restart-shell`, so the Kyber
  button can load.

**Step 5: verify.**

```
kyber doctor
```

It should end with "Everything checks out." (or only mention the optional
click-glow permission if the person chose to skip it). Tell them how to try it:
hold the left mouse button for about a second.

## Things the person can do afterwards (tell them)

- Change the glow color, sounds and trail: click the purple lightning bolt on the
  bar, or run `kyber color`, `kyber sound`, `kyber trail`, `kyber volume 40`.
- Add their own sounds: `kyber sounds add FILE ignite|off|clash`.
- Turn it off: `kyber off`. Remove it: run `./uninstall.sh` in the plugin folder.
- If something seems wrong: `kyber doctor` says what to fix.

## If something goes wrong

| Problem | What to do |
|---|---|
| `kyber: command not found` | Use the full path in Step 2, or run `~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/install.sh` once. |
| Cursor did not change color right away | Normal. Moving the mouse onto another window or the bar refreshes it. |
| The glow does not appear | Check `kyber doctor`. Usually the `input` group step was skipped or the computer was not restarted after it. |
| No sound | `kyber sound all`, then `kyber sounds preview`; also check the volume with `kyber volume`. |
| The Kyber button is missing | Restart the shell once (ask first). To place it: `omarchy bar put io.github.s3pp3ku.kyber-cursor` (ask first, it edits their bar). |

## Facts for you (not for the person)

- Settings live in `~/.config/kyber-cursor/`. Cursor themes: `Omarchy-Kyber` and
  `Omarchy-Kyber-Ignited` in `~/.local/share/icons`.
- The glow is a systemd **user** service, `kyber-ignite`. After editing
  `lib/ignite-listener`, run `systemctl --user restart kyber-ignite`.
- After a color or theme change the glow updates at once, but the real pointer
  image refreshes the next time the pointer crosses onto another window or the
  bar. This is a Hyprland limit, not a bug.
- Changes to the bar button's QML need a shell restart; hot reload does not replace
  already-compiled components.
- Contributors: see `CONTRIBUTING.md`. No copyrighted media, ever.
