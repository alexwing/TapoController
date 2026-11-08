# SourceForge project listing — copy/paste

Project: https://sourceforge.net/projects/tapocontroller/
(Web admin is interactive — fill these in the SourceForge project admin pages.)

## Short summary ("Project Summary" / tagline)

TP-Link Tapo L530 Controller — local, no cloud

## Description

TapoController is a Windows desktop app that controls TP-Link Tapo L530 smart
bulbs entirely on your LAN — no cloud, no vendor SDK. It uses its own
reverse-engineered implementation of the Tapo local protocol (KLAP) and drives
one bulb or several in sync.

One-time requirement: enable "Third-Party Services" once in the Tapo mobile
app. After that, all control is local (no internet needed).

## Features (one per line)

- Full control: on/off, brightness, colours, RGB picker, white temperature with warm/neutral/cool presets.
- Several bulbs at once, synchronized or one by one; finds bulbs on the LAN and follows them by MAC when their IP changes.
- Disco mode: the bulbs react to whatever the PC is playing (no microphone needed), with a live preview, beat/BPM tracking and an on-bulb animation locked to the tempo.
- Ambilight via screen capture, per monitor: works with PotPlayer, VLC, browsers or games.
- Smooth rainbow mode rendered by the bulb itself (no flicker, no per-frame network traffic).
- Local HTTP + WebSocket API for plugins and automation.
- Windows tray, built-in help guide, 5 languages (English, Spanish, German, French, Portuguese).
- Installer (NSIS + MSI), standalone exe, portable zip and a command-line tool.

## Metadata

- License: MIT
- Operating system: Windows
- Categories: Home Automation, Desktop Environment
- Languages: English, Spanish, German, French, Portuguese
- Programming language: Rust, TypeScript
- Repository: https://github.com/alexwing/TapoController
- Releases: https://github.com/alexwing/TapoController/releases

## Release files (GitHub → SourceForge sync)

Releases are mirrored from GitHub automatically. Assets on each release:

- TapoController-v<ver>-Setup.exe       (installer, recommended)
- TapoController-v<ver>.msi             (MSI installer)
- TapoController-v<ver>-Standalone.exe  (single exe, no install)
- TapoController-v<ver>-Portable.zip    (app + CLI + example config)
- tapo-cli-v<ver>-windows-x64.exe       (command-line tool)

Set the default download to `TapoController-v<ver>-Setup.exe` (Files → ⓘ on
the file → "Default download for: Windows"). The sync otherwise leaves the
last uploaded file (often the CLI) as the big green Download button.

## Screenshots ("Project Samples")

In the repo under docs/, taken at 2× from the app UI with demo data:

- docs/screenshot-normal.png — Normal tab (control, multi-bulb)
- docs/screenshot-disco.png — Disco tab (audio preview)
- docs/screenshot-streaming.png — Streaming tab (Ambilight)
- docs/screenshot-settings.png — Settings › Bulbs

## Note on signing

Release binaries are not code-signed, so Windows SmartScreen shows an
"unknown publisher" prompt the first time (More info → Run anyway).
