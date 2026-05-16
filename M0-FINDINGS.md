# M0 — Resultado del spike de protocolo

## Lo que ya está hecho y verificado (código)

- Workspace Cargo: `tapo-proto` (núcleo) + `tapo-cli` (spike).
- **Transporte KLAP v2** propio: handshake1/2, sesión, AES-128-CBC + firma.
- **Transporte legacy securePassthrough** propio: RSA-1024 + AES-128-CBC + `login_device`.
- **Autodetección** de protocolo (`detect_protocol`).
- API de dispositivo: `get_device_info`, power, brillo, HSV, color_temp, RGB→HSV.
- CLI de diagnóstico: `info / power / brightness / color / rgb / temp`.
- `cargo build` verde. `cargo test -p tapo-proto` → **6/6 OK** (vectores cripto incluidos).

## Sondeo en vivo contra la bombilla real (192.168.1.226)

| Prueba | Resultado |
|---|---|
| `ping` | OK, ~38 ms (bombilla viva en LAN) |
| `detect_protocol` | Detecta correctamente el dispositivo |
| `GET /app/handshake1` | 200 |
| **`POST /app/handshake1`** | **403 Forbidden** (consistente, cualquier body/headers) |
| `POST /app/request?seq=1` | 400 (endpoint KLAP existe) |
| `POST /app` (handshake legacy / securePassthrough / component_nego) | `{"error_code":1003}` |
| `Server` header | `SHIP 2.0` (firmware reciente) |

## Diagnóstico (bloqueante, verificado empíricamente + documentado)

La bombilla habla **KLAP**, pero su firmware reciente trae la **API local DESACTIVADA
de fábrica**: `POST /app/handshake1` → **403 Forbidden**. Esto **no se puede sortear
por software** — lo impone el firmware cerrado. Es un comportamiento conocido y
documentado tras las actualizaciones de firmware Tapo 1.4.x (afecta a P100/P110/P115/
L530, integraciones como Home Assistant, etc.).

**Única forma de desbloquearlo**: activarlo **una vez** en la app oficial Tapo:
`Yo / icono de cuenta → Servicios de terceros (Third-Party Services / "Tapo Lab →
Third-Party Compatibility") → ACTIVAR`. Eso exige iniciar sesión en la cuenta TP-Link
a la que está vinculada la bombilla. Tras activarlo, **el control es 100% local por LAN**
(sin nube en operación), pero:

1. Hace falta activar ese ajuste en la app (requiere la cuenta vinculada).
2. Para el `auth_hash` de KLAP siguen haciendo falta las credenciales de esa cuenta
   (o factory-reset + aprovisionamiento local con credenciales propias — pero el
   firmware reciente puede seguir bloqueando la API local hasta activarla en la app;
   no garantizado).

## Conclusión

El objetivo de M0 (de-risk antes de construir la UI) se ha cumplido: el código del
protocolo está completo y probado, y hemos descubierto **antes de invertir en la UI**
que esta bombilla concreta, con su firmware actual, **no es controlable localmente sin
una interacción única con la app/cuenta Tapo**. El requisito "cero cuenta TP-Link bajo
ninguna circunstancia" no es satisfacible con este hardware/firmware. Se requiere una
decisión del usuario para continuar.
