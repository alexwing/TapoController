# SourceForge project listing — copy/paste

Project: https://sourceforge.net/p/tapocontroller
(Web admin is interactive — fill these in the SourceForge project pages.)

## Summary (one line, "Project Summary" field)

100% local (no-cloud) desktop controller for Tapo L530 bulbs: color, smooth
animation, screen-capture Ambilight (PotPlayer/VLC), local API.

## Short description (Features / "Description" box)

TapoController is a Windows desktop app to control a TP-Link Tapo L530 smart
bulb entirely on your LAN — no cloud, no vendor SDK. It speaks an own,
reverse-engineered implementation of the Tapo KLAP protocol.

Features:
- Full control: on/off, brightness, hue/saturation, RGB, white temperature, presets.
- Smooth animated mode using the bulb's native dynamic-effect engine.
- Ambilight via screen capture (per-monitor): works with PotPlayer, VLC,
  browsers, games — fade/instant, fps and brightness-cap configurable.
- Local HTTP + WebSocket API for plugins/automation.
- Windows system tray, 3-tab UI, English/Spanish (auto-detected).
- Signed installers (NSIS + MSI) and a portable standalone exe.

One-time requirement: enable "Third-Party Services" once in the Tapo mobile app.

## Metadata

- License: MIT
- Operating system: Microsoft Windows (64-bit)
- Category / Topics: Home Automation, Utilities, Desktop Environment
- Programming language: Rust, TypeScript
- Repository: https://github.com/alexwing/TapoController
- Releases: https://github.com/alexwing/TapoController/releases

## Release files (GitHub → SourceForge sync)

Use the same GitHub-release sync you use for your other project (SourceForge
project Admin → GitHub integration). Assets on each GitHub release:

- TapoController_<ver>_x64-setup.exe   (NSIS installer, recommended)
- TapoController_<ver>_x64_en-US.msi   (MSI installer)
- TapoController_<ver>_x64_standalone.exe (portable, no install)

Set the default download to the `*-setup.exe`.

## Screenshots to upload (in the repo, under docs/)

- docs/capture02.png — Normal tab (control + animated mode)
- docs/capture01.png — Streaming tab (Ambilight)

## Note on signing

Installers are Authenticode-signed (SHA-256 + RFC3161 timestamp) but with a
self-signed certificate, so Windows SmartScreen still shows an
"unknown publisher" prompt until the certificate is trusted. Mention this in
the description so users aren't surprised.
