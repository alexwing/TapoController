# TapoController — API local (v1)

Servidor HTTP/WebSocket embebido en la app de escritorio. Pensado para plugins
externos (VLC/PotPlayer ambilight, sincronía con música, automatizaciones).

- Base: `http://127.0.0.1:7755` (configurable en `[api]` de `tapo-config.toml`).
- Comparte el **mismo** `ControlService` que la UI: sesión y rate-limiting unificados.
- CORS abierto (uso local).
- Todas las respuestas REST: `{"ok":true}` o `{"ok":false,"error":"..."}`
  (`502` si el dispositivo falla, `400` si la petición es inválida).

## REST

| Método | Ruta           | Cuerpo JSON                              | Acción |
|--------|----------------|------------------------------------------|--------|
| GET    | `/health`      | —                                        | Ping del servidor |
| GET    | `/state`       | —                                        | Estado del dispositivo |
| POST   | `/power`       | `{"on": true}`                           | Encender/apagar |
| POST   | `/brightness`  | `{"value": 1..100}`                      | Brillo |
| POST   | `/color`       | `{"hue":0..360,"saturation":0..100}` **o** `{"r":0..255,"g":..,"b":..}` | Color |
| POST   | `/color_temp`  | `{"kelvin": 2500..6500}`                 | Blanco cálido/frío |
| POST   | `/white`       | `{"kelvin":2500..6500,"brightness":1..100}` | Blanco a máx. lúmenes (LEDs blancos) |
| POST   | `/animation`   | `{"on":true,"speed":1..100}`             | Modo animado (arcoíris) |
| GET    | `/stream`      | (WebSocket upgrade)                      | Flujo de color alta frecuencia |

### Ejemplos curl

```sh
curl http://127.0.0.1:7755/health
curl http://127.0.0.1:7755/state
curl -X POST http://127.0.0.1:7755/power       -H 'Content-Type: application/json' -d '{"on":true}'
curl -X POST http://127.0.0.1:7755/brightness  -H 'Content-Type: application/json' -d '{"value":70}'
curl -X POST http://127.0.0.1:7755/color       -H 'Content-Type: application/json' -d '{"r":255,"g":80,"b":0}'
curl -X POST http://127.0.0.1:7755/color_temp  -H 'Content-Type: application/json' -d '{"kelvin":4000}'
```

## WebSocket `/stream` (ambilight / música)

Conecta a `ws://127.0.0.1:7755/stream` y envía frames de color tan rápido como
quieras. El servidor **descarta los intermedios** y aplica el cap (`stream.max_hz`)
y el suavizado (`stream.smoothing`). Esto evita saturar la bombilla.

Formatos de frame aceptados:
- Texto JSON: `{"r":255,"g":120,"b":0}`
- Binario: 3 bytes `[r,g,b]`

> Realidad de latencia: el round-trip KLAP en LAN es ~30-150 ms. El stream es
> "ambiental", no sincronía exacta cuadro a cuadro a 60 fps. Ese es el motivo de
> que exista el pipeline de coalescing+suavizado.

Ejemplo Node (sin dependencias, Node 21+ trae `WebSocket` global):

```js
// node examples/ambilight-demo.mjs
const ws = new WebSocket("ws://127.0.0.1:7755/stream");
ws.onopen = () => {
  let t = 0;
  setInterval(() => {
    t += 0.05;
    const r = Math.round((Math.sin(t) * 0.5 + 0.5) * 255);
    const g = Math.round((Math.sin(t + 2) * 0.5 + 0.5) * 255);
    const b = Math.round((Math.sin(t + 4) * 0.5 + 0.5) * 255);
    ws.send(JSON.stringify({ r, g, b }));
  }, 16); // ~60 fps de entrada; el servidor lo limita solo
};
```

## Futuro plugin VLC/PotPlayer

El plugin solo necesita: muestrear el frame de vídeo, calcular un color medio
(o por zonas), y volcarlo al WebSocket `/stream`. No necesita conocer el
protocolo Tapo: la app hace todo el trabajo y protege la bombilla.
