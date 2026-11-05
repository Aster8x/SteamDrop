// Renders SteamDrop's own Steam artwork (assets/steam-art/*.svg) to the PNG sizes Steam uses,
// into src-tauri/steam-art/. Run it after changing an SVG: node scripts/render-steam-art.mjs
// Needs Playwright with Chromium (npm i -g playwright && npx playwright install chromium).
import { readFileSync, readdirSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const root = fileURLToPath(new URL("..", import.meta.url));
const src = root + "assets/steam-art/";
const out = root + "src-tauri/steam-art/";
const font = (file) => `url(data:font/ttf;base64,${readFileSync(root + "ui/fonts/" + file).toString("base64")})`;
const css = `
  @font-face { font-family: "Chakra Petch"; font-weight: 700; src: ${font("ChakraPetch-Bold.ttf")}; }
  @font-face { font-family: "Atkinson Hyperlegible"; font-weight: 700; src: ${font("AtkinsonHyperlegible-Bold.ttf")}; }
  html, body { margin: 0; background: transparent; } svg { display: block; }`;

mkdirSync(out, { recursive: true });
const browser = await chromium.launch();
for (const file of readdirSync(src).filter((f) => f.endsWith(".svg"))) {
  const svg = readFileSync(src + file, "utf8");
  const [, w, h] = svg.match(/width="(\d+)" height="(\d+)"/);
  const page = await browser.newPage({ viewport: { width: +w, height: +h } });
  await page.setContent(`<style>${css}</style>${svg}`);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: out + file.replace(".svg", ".png"), omitBackground: true });
  await page.close();
  console.log(`${file} -> ${w}x${h}`);
}
await browser.close();
