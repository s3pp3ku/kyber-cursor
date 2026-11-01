# ElevenLabs prompts for Kyber

The current live recordings received the user's "very nice" feedback. These
prompts are for optional new candidates; they are not a replacement selection.

## Shortest starting point

The earlier long copyable agent brief was a poor fit for the small Sound Effects
box. The user reported truncation, an awkward interface and disappointing
results. All primary prompts below are now under 200 characters.

If you choose to try again, start with just **hum**: set duration to **10 seconds**,
turn **Looping on**, and paste only the text in
[`elevenlabs-hum-short.txt`](elevenlabs-hum-short.txt). The current installed
recordings already received positive feedback, so generating replacements is
optional. A shorter prompt fixes the length issue; sound quality still needs
listening.

## If you are talking to an agent

Copy [`elevenlabs-agent-brief.txt`](elevenlabs-agent-brief.txt) into the agent's
chat. It asks the agent to use the Sound Effects generation tool. An agent needs
that capability to generate files; a conversational voice alone is not the sound
effects generator. ElevenLabs documents this distinction in its
[agent tooling guide](https://elevenlabs.io/docs/eleven-api/resources/agent-tooling).

## If you are using the Sound Effects page

Use the short role prompts below, **one at a time**, in the Sound Effects text
box. Set the duration and Looping using the controls. The long agent brief is
for a chat agent, rather than this small prompt box. ElevenLabs currently
lists a 450-character limit and four variants per web generation in its
[product guide](https://elevenlabs.io/docs/eleven-creative/playground/sound-effects).

Start with the hum and choose its sound character. Then generate the other
roles separately. A matching description is a target; separate generations may
still need listening and editing to fit together.

### Suggested starting settings

These are sound-design suggestions, not mandatory service settings.

| Role | Duration | Looping | Prompt influence |
| --- | ---: | --- | ---: |
| hum | 10 s | On | 55% |
| ignite | 1.3 s | Off | 65% |
| off | 1 s | Off | 65% |
| swipe | 0.6 s | Off | 55% |
| clash | 0.6 s | Off | 55% |

The API's v2 sound-effects model supports an explicit looping flag, duration,
and prompt influence. Increasing influence asks for closer prompt adherence;
it does not guarantee a match. See the
[API reference](https://elevenlabs.io/docs/api-reference/text-to-sound-effects/convert).
If influence is unavailable in the interface you use, keep the prompt and
adjust duration/looping there.

### Hum

```text
Deep layered lightsaber hum: motor drone, drifting electrical buzzes, slow beating undertones and fine crackle. Steady seamless loop, minimal air hiss. No impacts, voices or music.
```

### Ignite

```text
One lightsaber powering on. A low electrical thrum rises in pitch and grows into a full layered buzzing hum. Clear upward activation with fine crackle. No collision, explosion, speech or music.
```

### Off

```text
One lightsaber powering down. A full layered electrical hum falls in pitch and brightness, loses energy and fades to silence. Deep motor body and fine crackle. No impacts, speech or music.
```

### Swipe

```text
One energized blade swiping past: low electrical growl, dense buzzing body, brief passing pitch bend and fine electric sizzle. Little wind noise, short decay. No impact, speech or music.
```

### Clash

```text
One clash of energized blades: a sharp electrical crack, deep buzzing body and short sizzling decay. One compact impact with little echo. No repeated hits, explosion, speech or music.
```

## Useful feedback for the next generation

Change one main thing at a time. Examples to add to an agent conversation:

- **Hum is too thin:** "Strengthen the low-mid electrical body and add a second
  independently drifting buzz layer. Keep the crackle fine and quiet."
- **Hum is too airy:** "Reduce broadband air hiss and emphasize the electrical
  buzz, motor thrum and fine current crackle."
- **Hum sounds like one note:** "Separate the motor, electrical rasp and lower
  throb so their motion differs while the overall hum level stays steady."
- **Power-up sounds like a clash:** "Remove the collision transient. Make the
  upward pitch and energy buildup the main event."
- **Power-up motion is wrong:** "Make the pitch climb continuously through the
  activation, then settle briefly at the fully energized tone."
- **Shutdown motion is wrong:** "Start energized, descend continuously, and
  finish in silence."

For the small Sound Effects box, replace a less important phrase with the
correction rather than appending until the prompt exceeds its length limit.

## Bringing the results back

Download the chosen files, keep their prompts/settings and identify each role.
WAV is useful when offered; MP3 is fine too. ElevenLabs documents WAV at 48 kHz
and MP3 downloads, while its overview describes format limits for looping
outputs. We can prepare whichever it provides as the same portable PCM WAV
format used in Kyber, check the loop and compare the transitions before selecting
a replacement pack.

The machine-readable prompts are in
[`elevenlabs-prompts.json`](elevenlabs-prompts.json). No API requests, account
changes or audio uploads were made while preparing this brief.
