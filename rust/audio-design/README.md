# Sound decisions

## Current private package defaults

On 2026-10-17 the owner explicitly chose the existing Downloads recordings for
all future private, noncommercial packages, then confirmed **"Use the downloaded
hum clips"**. This supersedes the previous generated defaults and the earlier
approved complete synthetic hold blend. No further synthetic auditions are needed.

| Sound | Selected recordings |
| --- | --- |
| Power-up | lightsaber-ignite, lightsaber-ignite-burst, sith-lightsaber-1 |
| Power-down | lightsaber-off, sith-deactivate-1 |
| Clash | lightsaber-clash-02, lightsaber-clash-04 |
| Swipe | lightsaber-whoosh-01, lightsaber-whoosh-04 |
| Hold / hum | lightsaber-hum, lightsaber-hummm |

[`default-pack.json`](default-pack.json) records the decision and shared asset
manifest. The portable core embeds the prepared mono PCM16 / 44.1 kHz WAVs.
Their pitch and sound character are retained; playback crossfades the hold loop
and applies gentle entry/release. Platform adapters use the same defaults.
The source code's MIT license does not license the third-party recordings.
See [`../../assets/sounds/README.md`](../../assets/sounds/README.md).

## Historical auditions

[`hold-hum.json`](hold-hum.json) retains hashes for the earlier approved
revision 3 complete synthetic blend. It is preserved as history, not selected
for current packages. [`ignition.json`](ignition.json) records the two rejected
synthetic ignition revisions and the switch to recordings. All audition folders
remain intact. The rejected rising draft is not awaiting selection.
