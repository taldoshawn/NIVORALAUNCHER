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
