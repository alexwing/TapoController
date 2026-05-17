// Procedurally renders the app icon (no deps): a glowing bulb with an
// ambilight RGB ring on a rounded dark-purple tile. Supersampled for AA.
// Outputs PNGs + a multi-size .ico into src-tauri/icons/.
import { writeFileSync } from "node:fs";
import zlib from "node:zlib";

const OUT = "src-tauri/icons";
const SIZES = [16, 32, 48, 64, 128, 256, 512];
const SS = 4; // supersampling

// ---- tiny PNG encoder ----
const crcTab = (() => {
  const t = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function chunk(type, data) {
  const tb = Buffer.concat([Buffer.from(type), data]);
  let c = ~0 >>> 0;
  for (let i = 0; i < tb.length; i++) c = crcTab[(c ^ tb[i]) & 255] ^ (c >>> 8);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE((~c) >>> 0);
  return Buffer.concat([len, tb, crc]);
}
function encodePng(size, rgba) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(
      raw,
      y * (size * 4 + 1) + 1,
      y * size * 4,
      y * size * 4 + size * 4
    );
  }
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---- drawing helpers ----
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
function mix(c1, c2, t) {
  return [
    lerp(c1[0], c2[0], t),
    lerp(c1[1], c2[1], t),
    lerp(c1[2], c2[2], t),
  ];
}
// rounded-rect signed distance (negative inside)
function rrectSDF(px, py, cx, cy, hw, hh, r) {
  const qx = Math.abs(px - cx) - (hw - r);
  const qy = Math.abs(py - cy) - (hh - r);
  const ax = Math.max(qx, 0);
  const ay = Math.max(qy, 0);
  return Math.hypot(ax, ay) + Math.min(Math.max(qx, qy), 0) - r;
}

// Render one size (already at supersampled resolution N), return RGBA buffer.
function renderAt(N) {
  const buf = Buffer.alloc(N * N * 4);
  const cx = N / 2;
  const cyBulb = N * 0.40;
  const R = N * 0.225; // bulb glass radius (smaller, leaves room)
  const tile = N * 0.46; // half-size of background tile
  const ambR = R * 1.5; // ambilight ring radius (fits inside tile)

  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const o = (y * N + x) * 4;
      let r = 0,
        g = 0,
        b = 0,
        a = 0;

      // background rounded tile with vertical purple gradient
      const sd = rrectSDF(x, y, cx, N / 2, tile, tile, N * 0.16);
      const tileA = clamp(0.5 - sd, 0, 1);
      if (tileA > 0) {
        const gy = y / N;
        const bg = mix([28, 22, 46], [18, 16, 30], gy); // #1c162e -> #12101e
        r = bg[0];
        g = bg[1];
        b = bg[2];
        a = tileA;
      }

      // ambilight ring: hue around the bulb (red->green->blue->magenta)
      const dx = x - cx;
      const dy = y - cyBulb;
      const dist = Math.hypot(dx, dy);
      const ringW = R * 0.16;
      const ring = 1 - clamp(Math.abs(dist - ambR) / ringW, 0, 1);
      if (ring > 0 && a > 0) {
        let ang = Math.atan2(dy, dx) / (2 * Math.PI) + 0.5; // 0..1
        const stops = [
          [255, 60, 60],
          [255, 190, 40],
          [70, 220, 110],
          [60, 170, 255],
          [150, 90, 240],
          [255, 60, 60],
        ];
        const fpos = ang * (stops.length - 1);
        const i = Math.floor(fpos);
        const cc = mix(stops[i], stops[i + 1], fpos - i);
        const k = ring * 0.95 * a;
        r = lerp(r, cc[0], k);
        g = lerp(g, cc[1], k);
        b = lerp(b, cc[2], k);
      }

      // glow halo inside the ring
      if (a > 0) {
        const glow = clamp(1 - dist / (ambR * 1.05), 0, 1) ** 2 * 0.35;
        r = lerp(r, 168, glow * a);
        g = lerp(g, 130, glow * a);
        b = lerp(b, 247, glow * a); // accent #a882f7-ish
      }

      // bulb glass: radial gradient warm-white -> accent purple
      const edge = clamp((R - dist) / (N * 0.012), 0, 1); // AA edge
      if (edge > 0 && a > 0) {
        const tg = clamp(dist / R, 0, 1);
        const glass = mix([255, 247, 224], [124, 58, 237], tg ** 0.85);
        r = lerp(r, glass[0], edge);
        g = lerp(g, glass[1], edge);
        b = lerp(b, glass[2], edge);
        // filament: simple loop
        const fy = (y - cyBulb) / R;
        const fx = (x - cx) / R;
        const fil =
          1 -
          clamp(
            Math.abs(Math.hypot(fx, fy + 0.05) - 0.42) / 0.06,
            0,
            1
          );
        if (fil > 0 && fy < 0.35) {
          r = lerp(r, 255, fil * 0.8 * edge);
          g = lerp(g, 225, fil * 0.8 * edge);
          b = lerp(b, 150, fil * 0.8 * edge);
        }
      }

      // tapered neck + screw base (metallic, threaded)
      const neckTop = cyBulb + R * 0.58;
      const baseBottom = cyBulb + R * 1.7;
      if (a > 0 && y >= neckTop && y <= baseBottom + 2) {
        const ty = clamp((y - neckTop) / (baseBottom - neckTop), 0, 1);
        // half width: from under the glass, narrow, then the wider screw base
        const halfW =
          ty < 0.32
            ? lerp(R * 0.52, R * 0.4, ty / 0.32)
            : lerp(R * 0.4, R * 0.46, (ty - 0.32) / 0.68);
        const msd = rrectSDF(x, y, cx, (neckTop + baseBottom) / 2,
          halfW, (baseBottom - neckTop) / 2, R * 0.10);
        const mA = clamp(0.5 - msd, 0, 1);
        if (mA > 0) {
          const stripe = 0.5 + 0.5 * Math.sin((y) * (26 / R));
          const shade = 1 - Math.abs(x - cx) / (halfW + 0.001); // cylinder
          let bc = mix([70, 72, 84], [150, 154, 168], 0.35 + 0.65 * shade);
          bc = mix(bc, [60, 62, 72], ty > 0.32 ? stripe * 0.5 : 0);
          r = lerp(r, bc[0], mA);
          g = lerp(g, bc[1], mA);
          b = lerp(b, bc[2], mA);
        }
      }

      buf[o] = clamp(Math.round(r), 0, 255);
      buf[o + 1] = clamp(Math.round(g), 0, 255);
      buf[o + 2] = clamp(Math.round(b), 0, 255);
      buf[o + 3] = clamp(Math.round(a * 255), 0, 255);
    }
  }
  return buf;
}

// downsample NxN (N = size*SS) -> size, box filter
function downsample(big, N, size) {
  const out = Buffer.alloc(size * size * 4);
  const f = N / size;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let r = 0,
        g = 0,
        bl = 0,
        a = 0;
      for (let yy = 0; yy < f; yy++) {
        for (let xx = 0; xx < f; xx++) {
          const o = ((y * f + yy) * N + (x * f + xx)) * 4;
          const al = big[o + 3] / 255;
          r += big[o] * al;
          g += big[o + 1] * al;
          bl += big[o + 2] * al;
          a += al;
        }
      }
      const n = f * f;
      const oo = (y * size + x) * 4;
      if (a > 0) {
        out[oo] = Math.round(r / a);
        out[oo + 1] = Math.round(g / a);
        out[oo + 2] = Math.round(bl / a);
      }
      out[oo + 3] = Math.round((a / n) * 255);
    }
  }
  return out;
}

const pngs = {};
for (const s of SIZES) {
  const big = renderAt(s * SS);
  const small = downsample(big, s * SS, s);
  pngs[s] = encodePng(s, small);
}
writeFileSync(`${OUT}/icon.png`, pngs[512]);
writeFileSync(`${OUT}/128x128@2x.png`, pngs[256]);
writeFileSync(`${OUT}/128x128.png`, pngs[128]);
writeFileSync(`${OUT}/32x32.png`, pngs[32]);

// multi-size ICO (PNG-compressed entries)
const icoSizes = [16, 32, 48, 64, 128, 256];
const dir = Buffer.alloc(6);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2);
dir.writeUInt16LE(icoSizes.length, 4);
let off = 6 + icoSizes.length * 16;
const ents = [];
for (const s of icoSizes) {
  const e = Buffer.alloc(16);
  e[0] = s >= 256 ? 0 : s;
  e[1] = s >= 256 ? 0 : s;
  e.writeUInt16LE(1, 4);
  e.writeUInt16LE(32, 6);
  e.writeUInt32LE(pngs[s].length, 8);
  e.writeUInt32LE(off, 12);
  off += pngs[s].length;
  ents.push(e);
}
writeFileSync(
  `${OUT}/icon.ico`,
  Buffer.concat([dir, ...ents, ...icoSizes.map((s) => pngs[s])])
);

console.log("icons written:", SIZES.join(","), "+ icon.ico");
