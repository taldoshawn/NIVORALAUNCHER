# Native PvP overlays (Android)

Select Minecraft **1.8.9**, then open **Mods**. FPS, CPS, Keystrokes,
Coordinates and Armor are bundled native modules: enable each independently.
They require no Forge JAR and do not execute Java mods. The existing Zoom module
remains available (default key Z; remap it to a touchscreen button if needed).

The PvP entries are hidden for other versions and their runtime checks require
protocol 47. Switching versions preserves the profile's saved enable/disable
choices. New overlays are disabled by default. In-game settings for FPS,
Keystrokes, Coordinates and Armor are under **Options > Mods**.

CPS reports local input counts, not server-confirmed hits. Armor lists equipped
pieces and the held stack; this does not currently show durability percentages.
These overlays do not change attack range, damage, movement or server rules.

Validation: `python3 tools/test_android_bundled_mods.py` includes profile-switch
and independent saved-choice coverage. On-device overlay layout, touchscreen
CPS and multiplayer behavior still require Android validation.

## Remote configuration

Starting with the remote-mods APK, the launcher and active game check the official
`https://nivora-launcher.vercel.app/api/mods-config` endpoint once per minute in a
background worker. The owner edits and publishes **Mods PvP · configuração
remota** in the website's existing MFA-protected panel. Increase `revision` for
each change; to restore defaults, publish a higher revision with `mods: []`.

The five fixed PvP module IDs support titles, descriptions, availability and
allowlisted string defaults (such as `backdrop`, `show_cps` and `corner`). Corners
use `top_left`, `top_right`, `bottom_left`, `bottom_right`. Locally saved settings
always win over remote defaults. Availability can temporarily hide and disable
a module without deleting its saved enable choice.

The manifest cannot enable modules for the player, run scripts, add native
features, change combat or modify arbitrary files. New Rust functions and Forge
compatibility still require an APK update. Existing content-pack installation is
unchanged. Cached valid settings work offline; malformed, oversized or older
revisions are rejected. Requests have a five-second timeout, TLS verification,
no redirects and a 16 KiB limit. Depending on cache/refresh timing, changes can
take roughly 1–2 minutes; this is polling, not a push channel.
