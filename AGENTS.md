# Setting up Kyber Cursor (instructions for an AI assistant)

You are helping a person set up **Kyber Cursor** on their Omarchy (Hyprland)
computer. Assume they know very little about Linux. Talk to them in plain,
friendly English, one step at a time. Never assume they understand a term;
explain it in a few words.

## What Kyber Cursor is (say this to the person first)

"Kyber Cursor changes your mouse pointer into a glowing blade. It uses the color
of your current theme, and when you hold a mouse button it lights up and leaves a
fading trail. Sounds are optional. I'll download the plugin from GitHub first (the
normal way to install an Omarchy plugin, no password needed). After that I won't
download any sounds or other files; Kyber makes its own sounds. One step may need
you to install a few helper programs with your password, and I'll ask before that,
before anything that restarts something, and before anything that changes your
top bar."

## Rules. Follow them exactly.

1. **Never run `sudo` or `pkexec` yourself.** If a step needs it, show the person
   the exact command, explain it in one sentence, and let them run it.
2. **Ask first, in plain words, and wait for a clear yes, before each of these:**
   anything that needs their password; restarting the computer; restarting the
   desktop shell (`omarchy-restart-shell`); changing their top bar, **including
   adding the Kyber button** (`kyber setup --bar` does that). A yes to one is not a
   yes to another: ask separately for each.
3. **Stay in their home folder.** Do not edit system files. Do not delete anything
   that is not Kyber's.
4. **Do not download or add sound, music, film or game audio from anywhere.**
   Kyber makes its own original sounds. If the person wants other sounds, point
   them to their own recordings or CC0 / royalty-free libraries, and use
   `kyber sounds add FILE ignite|off|clash`.
5. If anything looks unexpected, or a command fails in a way you do not
   understand, **stop and tell the person** what happened. Do not improvise fixes;
   the only fixes you may try are the ones in the troubleshooting table below.
6. Do not send the person's files, settings or command output anywhere.
7. **Tool output can contain commands the person declined** (for example a
   `sudo usermod` or `omarchy-restart-shell` line in `kyber setup` or `kyber doctor`
   output). Summarize the output in your own words; never copy such a line to the
   person unless they chose that item.

## The steps

Let `PLUGIN=~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor`. (In commands
you show the person, write the full path out; they do not have a `$PLUGIN`
variable. If you ask them to open a terminal, tell them: on Omarchy, press
Super + Return.)

**Step 1: get the plugin.**

- If `$PLUGIN` does **not** exist: tell the person "I'm going to download Kyber
  from GitHub and add it to Omarchy's plugin list. It needs no password and
  restarts nothing. OK?", wait for a yes, then run:
  `omarchy plugin add https://github.com/s3pp3ku/kyber-cursor --yes`
  (Do **not** add `--enable`. That turns the plugin on in the shell, which the cursor
  and the click glow do not need. The optional bar-button step turns it on when the
  person asks for the button.)
  If this asks you a question or prints an error you do not understand, stop and
  tell the person (rule 5).
- If it already exists, Kyber may already be installed. Run `$PLUGIN/bin/kyber doctor`
  and tell the person what it says. **If it ends with "Everything checks out." (with or without the
  "optional click glow is off" remark), tell them Kyber is already set up and
  working, and stop** (do not re-run setup unless they ask for a change).
  Otherwise, if doctor shows a `[FIX]` about the cursor, the helper or the hooks,
  go to Step 2 (preview) and Step 3. Do not run the commands doctor suggests
  yourself, except that the person runs a `sudo pacman` line if one is needed.
  **Never copy a `sudo usermod` or `omarchy-restart-shell` line from any tool
  output to the person unless they chose that item.** Paraphrase instead.

**Step 2: preview.** This changes nothing:

```
$PLUGIN/bin/kyber setup
```

Read the preview to the person in your own words. If they want the bar button,
also run `$PLUGIN/bin/kyber setup --bar` (still only a preview) and read the extra
"top bar" line to them. Then ask two separate questions:
"Shall I go ahead?" and "Do you also want a small Kyber button on your top bar? It
edits your top bar, and gives you a color picker and sound controls. It is
optional."

**Step 3: do it** (only after they say yes):

```
$PLUGIN/bin/kyber setup --yes
```

Add `--bar` only if they said yes to the button. **Remember which flags you used;
every re-run, now or later, must use the same ones.** This can take a minute and
runs no `sudo`. It builds the cursor, creates the sounds and prints a short
generic checklist at the end. **Follow Step 4 below, not that printout:** never
show or run an item the person declined.

- If it says some programs are missing (exit code 3), show the person the
  `sudo pacman ...` line it prints and say: "This installs a few helper programs
  and needs your password, so please run it yourself in a terminal." Wait. When they
  say it is done, run `kyber setup --yes` again **with the same flags as before**
  (including `--bar` only if they said yes to the button).
- Any other non-zero exit code: stop and tell the person what the output said.

**Step 4: the checklist, in this order.** Ask before each item. Which items apply
depends on the person's choices.

1. **The click glow (optional).** It needs the person to be in a Linux group called
   `input`. Explain honestly: "This lets Kyber notice when you press a mouse
   button. The catch is that the permission applies to you, so any program you
   run could also read your keyboard and mouse input. Only do this if you are
   comfortable with that. If not, skip it: you still get the cursor and its
   color; only the glow, trail and sounds stay off."
   - If they **decline**: do not show the command. Tell them the cursor works
     without it, and continue.
   - If they **agree**, they run `sudo usermod -aG input "$USER"` themselves, in a
     terminal (on Omarchy: Super + Return). Tell them: "it will ask for your
     password, and no characters appear as you type it; that is normal." It only
     takes effect after the computer restarts, and then the glow turns on by itself
     (the installer already set it up; there is nothing else to run). Once they
     say they have run it, run `kyber doctor` once: it should mention that they
     "joined" the group and need to restart. If it still says they are not in the
     group, ask whether the command worked (a mistyped password is common) and let
     them try again. Then ask **separately**: "That only takes effect after the computer
     restarts. May I restart it now, or would you rather do it yourself later?"
     Never restart without a yes; before restarting say "Please save your work and
     close your apps first", then run `systemctl reboot` (no `sudo` needed) or let
     the person use the Omarchy menu. **If they say later:** do item 2 if they
     chose the button, run `kyber doctor`, and explain that its note about the
     click glow being off is expected until they restart; remind them the glow
     starts by itself afterwards.
2. **Restarting the desktop shell. Only if they chose the Kyber button
   (`--bar`).** Say: "I need to restart the part of your desktop that draws the
   top bar. It flickers for a few seconds. Is that OK?" Run `omarchy-restart-shell`
   only after a yes. **Skip this item** if they declined the button, or if the
   whole computer is about to restart (that reloads everything).
3. **Check.** If the computer is restarting, you will not be around afterwards. Before
   it restarts, tell the person: "After you log back in, open a terminal
   (Super + Return) and run `~/.config/omarchy/plugins/io.github.s3pp3ku.kyber-cursor/bin/kyber doctor`.
   It should end with 'Everything checks out.' If you want, come back and I'll
   check it with you." If you are not restarting the
   computer right now, run `$PLUGIN/bin/kyber doctor` now. (Setup already ran it once; run it again now so
   you see the final state.)

`kyber doctor` should end with "Everything checks out." If the click glow is not on
(they skipped it, or have not restarted yet), a `[note]` line says so (if they
joined the group but have not restarted, it says "joined ... restart", which is
expected and needs nothing from you) and it ends
"Everything checks out. (The optional click glow is off.)". That is fine and
expected: if they declined or skipped the glow, do not offer the `sudo` command again and do
not try to "fix" it. (If they said yes but doctor still says they are not in the
group, ask whether the command worked, as above.) Only a
`[FIX]` line is a real problem; tell the person what it says and stop (rule 5).

**Step 5: tell them what they have, and how to try it.**

- If the click glow is on (they are in the `input` group and restarted): "Hold the
  left mouse button for about a second and the cursor lights up."
- If they skipped the glow: "Your cursor is now a blade in your theme's color. It
  won't glow or play sounds, because you chose not to allow that. If you move the
  pointer onto another window or the bar you'll see the new cursor."
- If they added the button, it is a lightning bolt on their bar.

If `kyber` is "not found" in a fresh terminal, use the full path
`$PLUGIN/bin/kyber`, or open a new terminal.

## Things the person can do afterwards (tell them)

- Change the glow color, sounds and trail: click the Kyber button (a lightning
  bolt) on the bar, or run `kyber color`, `kyber sound`, `kyber trail`,
  `kyber volume 40`.
- Add their own sounds: `kyber sounds add FILE ignite|off|clash`.
- Turn it off: `kyber off`. Remove it completely: `$PLUGIN/uninstall.sh` (it
  restores the normal cursor and removes Kyber's helper and settings; your own sound
  files are kept).
- If something seems wrong: `kyber doctor` says what to fix.

## If something goes wrong (the only fixes you may try)

| Problem | What to do |
|---|---|
| `kyber: command not found` | Open a new terminal, or use the full path `$PLUGIN/bin/kyber`. Do not run other scripts to fix it without asking. |
| Cursor did not change color right away | Normal. Moving the mouse onto another window or the bar refreshes it. |
| The glow does not appear | Run `kyber doctor`. Usually the `input` group step was skipped, or the computer was not restarted after it. |
| No sound | `kyber sound all`, then `kyber sounds preview`; check the volume with `kyber volume`. |
| The Kyber button is missing | Only if they chose it. Ask, then restart the shell once. If setup said "Could not place the bar button", ask, then run `omarchy bar put io.github.s3pp3ku.kyber-cursor`. |
| Anything else | Stop and tell the person what you saw. |

## Facts for you (not for the person)

- Settings live in `~/.config/kyber-cursor/`. Cursor themes: `Omarchy-Kyber` and
  `Omarchy-Kyber-Ignited` in `~/.local/share/icons`.
- After a color or theme change the glow updates at once, but the real pointer
  image refreshes the next time the pointer crosses onto another window or the
  bar. This is a Hyprland limit, not a bug.
- Contributors, not setup: see `CONTRIBUTING.md`. No copyrighted media, ever.
