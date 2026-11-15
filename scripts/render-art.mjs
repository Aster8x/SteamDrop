// Draws SteamDrop's own pictures from the same pixel-art code the app uses (ui/grove.js): the app
// icon from the hand-drawn logo (src-tauri/icons), its Steam artwork (src-tauri/steam-art) and the
// installer's pictures (src-tauri/windows), the logo standing on the forest floor. Run it after
// changing the art: node scripts/render-art.mjs
// Needs Playwright with Chromium (npm i -g playwright && npx playwright install chromium).
import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const root = fileURLToPath(new URL("..", import.meta.url));
const browser = await chromium.launch();
const page = await browser.newPage();
await page.goto(`file://${root}ui/couch.html`);
await page.evaluate(() => document.fonts.load('600 40px "Pixelify Sans"'));

// Everything is drawn small and enlarged by whole numbers, so pixels stay square.
const png = (what, w, h, text) => page.evaluate(([what, w, h, text]) => {
  const P = Grove.palette();
  const out = document.createElement("canvas");
  out.width = w; out.height = h;
  const x = out.getContext("2d");
  x.imageSmoothingEnabled = false;
  if (what.icon) x.drawImage(Grove.icon(what.icon), 0, 0, w, h);
  else if (what.scene) { const k = what.scale; x.drawImage(Grove.picture(Math.ceil(w / k), Math.ceil(h / k), P, what.scene), 0, 0, Math.ceil(w / k) * k, Math.ceil(h / k) * k); }
  // [x, y, scale]: the logo enlarged by a whole number, so every pixel stays square and sharp.
  if (what.logo) x.drawImage(Grove.icon(32), what.logo[0], what.logo[1], 32 * what.logo[2], 32 * what.logo[2]);
  if (text) {
    x.font = `600 ${text.size}px "Pixelify Sans"`; x.textAlign = text.align || "center"; x.textBaseline = "middle";
    x.fillStyle = P["sky-0"]; x.fillText("SteamDrop", text.x, text.y + Math.round(text.size / 12));
    x.fillStyle = "#f1ead2"; x.fillText("SteamDrop", text.x, text.y);
  }
  return [out.toDataURL().split(",")[1], [...x.getImageData(0, 0, w, h).data]];
}, [what, w, h, text]);
const save = (path, b64) => { writeFileSync(root + path, Buffer.from(b64, "base64")); console.log(path); };

// The app icon: 16, 24 and 32 px are drawn by hand; 48 is the 24 px one doubled, larger ones enlarge the 32 px one.
const grid = { 16: 16, 24: 24, 32: 32, 48: 24, 64: 32, 128: 32, 256: 32, 512: 32 };
const icons = {};
for (const size of Object.keys(grid)) icons[size] = Buffer.from((await png({ icon: grid[size] }, +size, +size))[0], "base64");
save("src-tauri/icons/32x32.png", icons[32].toString("base64"));
save("src-tauri/icons/128x128.png", icons[128].toString("base64"));
save("src-tauri/icons/128x128@2x.png", icons[256].toString("base64"));
save("src-tauri/icons/icon.png", icons[512].toString("base64"));
// An .ico is a small table followed by the PNG files themselves.
const sizes = [16, 24, 32, 48, 64, 256];
const head = Buffer.alloc(6 + 16 * sizes.length);
head.writeUInt16LE(1, 2); head.writeUInt16LE(sizes.length, 4);
let at = head.length;
sizes.forEach((s, i) => {
  const o = 6 + i * 16;
  head[o] = head[o + 1] = s % 256; head.writeUInt16LE(1, o + 4); head.writeUInt16LE(32, o + 6);
  head.writeUInt32LE(icons[s].length, o + 8); head.writeUInt32LE(at, o + 12);
  at += icons[s].length;
});
save("src-tauri/icons/icon.ico", Buffer.concat([head, ...sizes.map((s) => icons[s])]).toString("base64"));

// Steam artwork for SteamDrop's own tile, in the sizes Steam uses: the forest, the logo standing on its
// floor (the logo's grass strip ends 4 of its 32 rows above the bottom) and the name.
const stand = (groundTop, scale) => groundTop - 28 * scale;
save("src-tauri/steam-art/portrait.png", (await png({ scene: { size: 0, ground: 24 }, scale: 6, logo: [172, stand(900 - 24 * 6, 8) + 8, 8] }, 600, 900, { x: 300, y: 330, size: 100 }))[0]);
save("src-tauri/steam-art/wide.png", (await png({ scene: { size: 0, ground: 17 }, scale: 4, logo: [60, stand(430 - 17 * 4, 7) + 7, 7] }, 920, 430, { x: 600, y: 200, size: 104 }))[0]);
save("src-tauri/steam-art/hero.png", (await png({ scene: { size: 0 }, scale: 4 }, 1920, 620))[0]);
save("src-tauri/steam-art/logo.png", (await png({ logo: [100, 232, 8] }, 1280, 720, { x: 400, y: 368, size: 150, align: "left" }))[0]);
save("src-tauri/steam-art/icon.png", icons[256].toString("base64"));

// The installer's pictures are .bmp files: a 54-byte header, then rows from the bottom up, blue first.
async function bmp(path, w, h, what) {
  const [, rgba] = await png(what, w, h);
  const row = Math.ceil(w * 3 / 4) * 4, out = Buffer.alloc(54 + row * h);
  out.write("BM"); out.writeUInt32LE(out.length, 2); out.writeUInt32LE(54, 10); out.writeUInt32LE(40, 14);
  out.writeInt32LE(w, 18); out.writeInt32LE(h, 22); out.writeUInt16LE(1, 26); out.writeUInt16LE(24, 28); out.writeUInt32LE(row * h, 34);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = (y * w + x) * 4, o = 54 + (h - 1 - y) * row + x * 3;
    out[o] = rgba[i + 2]; out[o + 1] = rgba[i + 1]; out[o + 2] = rgba[i];
  }
  save(path, out.toString("base64"));
}
await bmp("src-tauri/windows/sidebar.bmp", 164, 314, { scene: { size: 0, ground: 25 }, scale: 2, logo: [18, stand(314 - 25 * 2, 4) + 4, 4] });
await bmp("src-tauri/windows/header.bmp", 150, 57, { scene: { size: 0, ground: 9 }, scale: 1, logo: [112, stand(57 - 9, 1) + 1, 1] });
await browser.close();
