# NIVORA release candidate audit

Source baseline: `ec19413` on `codex/android-correcoes-apk`. App and site remain independent repositories. Android application ID and data directories are preserved.

## Findings and targeted corrections

- Local player entity metadata ignored the authoritative fire bit. The local predicted lava timer could continue after the server extinguished it. Track the bit per connection, reset on login/respawn without KEEP_ENTITY_DATA, and reconcile the player timer after simulation. Legitimate burning remains enabled.
- Protocol 47 used the modern velocity cleanup (0.003, horizontal vector combined) and modern jump behavior retaining greater vertical velocity. Use the legacy 0.005 component threshold and explicit jump impulse in this mode only; disable the modern wind-charge preservation branch. Legacy keyboard sneak/use scaling now precedes diagonal normalization. These are identified divergences, not proof all lagback is resolved.
- Item-use identity was captured on the first tick instead of at interaction time. A plugin's early soup-to-bowl slot update could become the use identity. Capture at start and stop use as soon as the authoritative hand item changes. No client-side healing, invented plugin detection, or universal instant-consumption override.

## Validation boundary

Targeted arithmetic/input regression tests, existing protocol fixtures and HUD tests are included in the Android workflow. Local Python APK audit tests: 8 passed. Full Android build status must be checked on the exact commit.

Pending: death in lava and respawn on a real protocol-47 server; same-dimension transfer; reconnect; legitimate server fire; repeated jumps, sprint/diagonal near obstacles and liquids; delayed HG slot responses; complete legacy combat comparison; physical Android IME and multitouch; sustained CPU/GPU/frame-time profiling. No FPS improvement is claimed. Existing emulator evidence predates this candidate and cannot validate these changes.

Release must remain experimental. Do not describe this as full 1.8.9 engine parity. The renderer and resource target remain modern; local worlds remain modern.
