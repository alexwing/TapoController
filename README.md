# TapoController

**100% local control** (no cloud) of a Tapo L530 smart bulb, with desktop app
(Tauri + React) and a local API for future plugins (VLC/PotPlayer ambilight,
music).

## Project Structure

```
crates/tapo-proto/   Core: custom protocol implementation (KLAP + legacy
                     securePassthrough), config, ControlService (persistent
                     session, reconnection, color rate-limiter).
crates/tapo-cli/     Diagnostic CLI / spike.
src-tauri/           Desktop app + embedded axum API server.
src/                 React/Vite/TypeScript frontend.
docs/API.md          Local API specification.
examples/            WebSocket /stream client example.
```

## One-Time Setup Requirement (enforced by bulb firmware)

The L530 with recent firmware ships with the **local API DISABLED**. Verified
against your bulb: `POST /app/handshake1` → `403 Forbidden`. There is no
software workaround. You must enable it **once** in the official Tapo app:

> Account (person icon) → **Third-Party Compatibility** → **ON**

After that, **all control is local over LAN** (no internet or cloud in operation).
Full details and diagnostics in `M0-FINDINGS.md`.

## Quick Start

```sh
# 1. Configuration
cp tapo-config.example.toml tapo-config.toml
#    edit host/username/password (TP-Link account linked to the bulb)

# 2. Diagnostics (no credentials needed) — tells you exactly what's wrong
cargo run -p tapo-cli -- doctor
#    -> KlapGatedOff  = enable "Third-Party Compatibility" in Tapo app
#    -> KlapReady     = ready, add credentials and test:
cargo run -p tapo-cli -- -u "<email>" -p "<pass>" info
cargo run -p tapo-cli -- -u "<email>" -p "<pass>" color --hue 120 --sat 100

# 3. Desktop app
npm install
npm run tauri dev        # or: npm run build && cargo run -p tapo-controller

# 4. Local API (with app open)
curl http://127.0.0.1:7755/health
node examples/ambilight-demo.mjs
```

## Status

| Milestone | Status |
|-----------|--------|
| M0 protocol (KLAP + passthrough, auto-detection, CLI) | ✅ complete code, 8/8 tests, live tested |
| M1 config + ControlService (reconnection, rate-limiter) | ✅ |
| M2 Tauri app + IPC + React UI | ✅ compiles, frontend build OK |
| M3 API server (REST + WS) + docs + examples | ✅ integration test OK |
| M4 polish: auto-refresh UI, `doctor`/diagnostics (CLI + UI) | ✅ |
| Windows tray (on/off/show/exit) + minimize to tray | ✅ |
| Animated mode (rainbow, adjustable speed) | ✅ verified on bulb |
| **Real end-to-end verification** | ✅ **physical control confirmed** (power/RGB/brightness/temp) |

### Technical details discovered against real hardware

- Model `L530EA`, firmware `1.4.2`, **KLAP** protocol.
- The embedded **"SHIP 2.0" server is case-sensitive in HTTP headers**:
  rejects (`400`) lowercase headers emitted by hyper/reqwest. That's why
  `tapo-proto` uses a **custom raw HTTP/1.1 client** (`http.rs`) with
  Title-Case headers and `Content-Length` reading (firmware ignores
  `Connection: close`).
- `handshake1` requires **random seed** (rejects zero-filled body with `400`).
- This firmware's credential scheme: **`sha256( sha1(user) || sha1(pass) )`**
  (not the standard `sha256(sha256·)` KLAP v2). `resolve_auth` tries all known
  variants against `server_hash` and picks the matching one.

`cargo test --workspace` → green. `npm run build` → green.

## Testing

```sh
cargo test --workspace      # KLAP crypto, RGB↔HSV, rate-limiter, API server
npm run build               # typecheck + frontend build
```
