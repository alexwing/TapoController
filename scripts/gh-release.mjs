// Creates/updates a GitHub release and uploads installer assets.
// Token comes ONLY from env GH_TOKEN (never logged). No secrets in this file.
import { readFileSync, statSync } from "node:fs";

const TOKEN = process.env.GH_TOKEN;
const REPO = "alexwing/TapoController";
// Version comes from tauri.conf.json so a bump needs no edits here.
const VERSION = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8")
).version;
const TAG = `v${VERSION}`;
// Optional release notes: env RELEASE_NOTES or notes/<tag>.md.
const NOTES = process.env.RELEASE_NOTES;
if (!TOKEN) {
  console.error("NO_TOKEN");
  process.exit(1);
}

const ASSETS = [
  {
    path: `target/release/bundle/nsis/TapoController_${VERSION}_x64-setup.exe`,
    name: `TapoController_${VERSION}_x64-setup.exe`,
  },
  {
    path: `target/release/bundle/msi/TapoController_${VERSION}_x64_en-US.msi`,
    name: `TapoController_${VERSION}_x64_en-US.msi`,
  },
  {
    // Portable standalone executable (no installer).
    path: "target/release/tapo-controller.exe",
    name: `TapoController_${VERSION}_x64_standalone.exe`,
  },
];

const body = NOTES || [
  `TapoController ${VERSION} — 100% local control (no cloud) of a Tapo L530 bulb.`,
  "",
  "- Desktop app (Tauri 2 + React) with an own reverse-engineered Tapo protocol (KLAP).",
  "- Smooth animated mode (bulb's native effect) and screen-capture Ambilight",
  "  (PotPlayer/VLC/any player): monitor selector, fade/instant, fps, brightness cap.",
  "- Local API (REST + WebSocket) for plugins. Windows tray. 3-tab UI + ES/EN i18n.",
  "",
  "Windows x64 installers, Authenticode-signed (self-signed: SmartScreen will warn",
  "the first time → More info → Run anyway).",
  "",
  "One-time requirement: enable “Third-Party Services” once in the Tapo app.",
].join("\n");

const gh = (url, opts = {}) =>
  fetch(url, {
    ...opts,
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...(opts.headers || {}),
    },
  });

let rel = await gh(`https://api.github.com/repos/${REPO}/releases`, {
  method: "POST",
  body: JSON.stringify({
    tag_name: TAG,
    name: `TapoController ${TAG}`,
    body,
    draft: false,
    prerelease: false,
  }),
});

let data = await rel.json();
if (!rel.ok) {
  console.error(`create status ${rel.status}: ${JSON.stringify(data.errors || data.message)}`);
  // Tag/release may already exist -> fetch it.
  const ex = await gh(
    `https://api.github.com/repos/${REPO}/releases/tags/${TAG}`
  );
  if (!ex.ok) {
    console.error("could not get existing release");
    process.exit(1);
  }
  data = await ex.json();
}
const relId = data.id;
console.log(`release id=${relId} (${data.html_url})`);

// Remove any same-named assets so re-runs don't 422.
const existing = await (
  await gh(`https://api.github.com/repos/${REPO}/releases/${relId}/assets`)
).json();
for (const a of Array.isArray(existing) ? existing : []) {
  if (ASSETS.some((x) => x.name === a.name)) {
    await gh(
      `https://api.github.com/repos/${REPO}/releases/assets/${a.id}`,
      { method: "DELETE" }
    );
    console.log(`deleted old asset ${a.name}`);
  }
}

for (const a of ASSETS) {
  const sz = statSync(a.path).size;
  const buf = readFileSync(a.path);
  const up = await gh(
    `https://uploads.github.com/repos/${REPO}/releases/${relId}/assets?name=${encodeURIComponent(
      a.name
    )}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/octet-stream" },
      body: buf,
    }
  );
  console.log(`upload ${a.name} (${sz} bytes) -> ${up.status}`);
  if (!up.ok) console.error(JSON.stringify(await up.json()));
}

console.log(`Release: https://github.com/${REPO}/releases/tag/${TAG}`);
