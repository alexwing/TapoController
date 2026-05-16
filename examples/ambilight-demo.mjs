// Demo: push a smooth color cycle into the local API's /stream WebSocket.
// Run the desktop app first (API enabled), then:  node examples/ambilight-demo.mjs
// Node 21+ has a global WebSocket. On older Node, run with --experimental-websocket.

const URL = process.env.TAPO_API ?? "ws://127.0.0.1:7755/stream";
const ws = new WebSocket(URL);

ws.addEventListener("open", () => {
  console.log("connected ->", URL, "(Ctrl+C to stop)");
  let t = 0;
  setInterval(() => {
    t += 0.05;
    const r = Math.round((Math.sin(t) * 0.5 + 0.5) * 255);
    const g = Math.round((Math.sin(t + 2.094) * 0.5 + 0.5) * 255);
    const b = Math.round((Math.sin(t + 4.188) * 0.5 + 0.5) * 255);
    ws.send(JSON.stringify({ r, g, b }));
  }, 16); // ~60 fps in; the server coalesces + rate-limits to the bulb
});

ws.addEventListener("error", (e) => console.error("ws error:", e.message ?? e));
ws.addEventListener("close", () => console.log("closed"));
