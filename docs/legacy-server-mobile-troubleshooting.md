# NIVORA Android — multiplayer 1.8.9 troubleshooting

This document describes the **experimental** protocol-47 (Java 1.8–1.8.9)
compatibility path. It is not a claim that arbitrary servers, proxy stacks,
Forge plugins or modern versions are supported.

## Symptoms

- The connection display remains at "Connecting to the server..." and the
  server closes the transport with "End of stream".
- A large custom Pojav HUD (for example, overlapping buttons named SEC)
  intercepts Cancel or Back to Server List.

## What the Android fix changes

The transport now has two visible stages:

1. **Logging in to Minecraft 1.8.9** — TCP established, login negotiation active.
2. **Joining Minecraft 1.8.9 world** — login accepted, entering play.

If a peer closes the socket without a protocol disconnect message, the dialog
now identifies the phase. An EOF is **not** by itself proof of any particular
server-side issue; it may indicate protocol mismatch, proxy rejection, server
configuration, account/authentication failure or a remote network problem.

During the blocking connection/disconnected screens, the saved custom HUD
remains intact but its visual overlays and touch capture are suspended.
Taps on **Cancel** and **Back to Server List** take precedence even when custom
buttons overlap the same physical area.

## Reproducing / validating

- Choose Minecraft **1.8.9** in the launcher before testing a known Java
  protocol-47 server; modern-only servers are a separate unsupported target.
- Start the connection with a custom Pojav HUD enabled. Ensure Cancel works
  on first tap without toggling virtual-mouse mode.
- If the server disconnects, ensure Back to Server List works on first tap.
- Compare the stage shown on failure; inspect the local diagnostic logs
  without posting access tokens, passwords or account session files.
- Retest a genuinely compatible known-good server and a server that rejects
  the client. Protocol fixture tests cannot prove connectivity to every
  public Minecraft server.
- Verify skin/profile data, imported HUD presets and world files remain
  unchanged after exiting to the launcher.

## Validation status

The source tests cover the network stage labels, mobile modal priorities,
and protecting the normal Android package identity. The CI APK build and
real-device/real-server connectivity must be assessed separately. Never
describe the connection issue as fully resolved based solely on compilation.
