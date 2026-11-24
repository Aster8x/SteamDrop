// Draws the pictures on SteamDrop's GitHub page (docs/) from the app's own code: the banner, the
// feature icons, the social preview, screenshots of both views with made-up games, and a short GIF
// of the tree. Every picture is drawn here, nothing is downloaded. Run it after changing the look:
// node scripts/render-docs.mjs
// Needs Playwright with Chromium (npm i -g playwright && npx playwright install chromium) and ffmpeg.
import { writeFileSync, readFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const root = fileURLToPath(new URL("..", import.meta.url));
const docs = (p) => `${root}docs/${p}`;
const save = (p, dataUrl) => { writeFileSync(docs(p), Buffer.from(dataUrl.split(",")[1], "base64")); console.log(`docs/${p}`); };
mkdirSync(docs("icons"), { recursive: true });
const browser = await chromium.launch();

/* ---------- pictures drawn on a page that has the app's drawing code ---------- */
const art = await browser.newPage();
await art.goto(`file://${root}ui/couch.html`);
await art.evaluate(async () => {
  document.documentElement.dataset.theme = "dark";
  await document.fonts.load('600 40px "Pixelify Sans"'); await document.fonts.load('700 40px "Atkinson Hyperlegible"');
});

// Made-up games' covers (32 x 48 pixels): a moonlit hill, a sunset sea, a lantern, a burrow, a mountain, a ringed planet.
const covers = await art.evaluate(() => {
  const paint = [
    (g) => { g.bands(0, 48, ["#1d2b53", "#29366f", "#3b5dc9"]); for (let i = 0; i < 18; i++) g.px((i * 37) % 32, (i * 13) % 26, "#f4f4f4"); g.disc(22, 11, 5, "#f4f4d0"); g.disc(24, 9, 4, "#29366f");
      for (let x = 0; x < 32; x++) { g.rect(x, 34 + Math.round(Math.sin(x / 5) * 2), 1, 14, "#257179"); g.rect(x, 40 + Math.round(Math.sin(x / 4 + 2) * 1.5), 1, 8, "#38b764"); } },
    (g) => { g.bands(0, 20, ["#ffcd75", "#ef7d57"]); g.disc(16, 19, 6, "#ffe9b0"); g.bands(20, 48, ["#41a6f6", "#3b5dc9", "#29366f"]); for (let y = 23; y < 48; y += 5) for (let x = (y % 2) * 3; x < 32; x += 8) g.rect(x, y, 4, 1, "#a7f0f0"); },
    (g) => { g.rect(0, 0, 32, 48, "#1a1c2c"); g.disc(16, 22, 13, "#5d275d"); g.disc(16, 22, 9, "#b13e53"); g.rect(12, 14, 8, 14, "#ffcd75"); g.rect(13, 16, 6, 10, "#fff6d3"); g.rect(11, 12, 10, 2, "#333c57"); g.rect(11, 28, 10, 2, "#333c57"); g.rect(15, 6, 2, 6, "#333c57"); },
    (g) => { g.bands(0, 48, ["#a7f070", "#38b764"]); for (let y = -12; y <= 12; y++) { const s = Math.round(18 * Math.sqrt(1 - (y * y) / 145)); g.rect(16 - s, 40 + y, 2 * s + 1, 1, "#73402c"); } g.disc(16, 41, 6, "#2a1a14");
      for (const [x, y, c] of [[6, 30, "#b13e53"], [26, 31, "#ef7d57"]]) { g.rect(x, y - 3, 1, 3, "#f4f4f4"); g.rect(x - 2, y - 5, 5, 2, c); g.rect(x - 1, y - 6, 3, 1, c); g.px(x - 1, y - 5, "#f4f4f4"); }
      g.rect(13, 38, 2, 2, "#f4f4f4"); g.rect(18, 38, 2, 2, "#f4f4f4"); },
    (g) => { g.bands(0, 48, ["#73eff7", "#41a6f6", "#a7f0f0"]); for (const [x, y] of [[2, 6], [18, 12]]) { g.rect(x + 2, y, 9, 2, "#f4f4f4"); g.rect(x, y + 2, 15, 3, "#f4f4f4"); g.rect(x + 5, y - 2, 5, 2, "#f4f4f4"); }
      for (let y = 0; y < 30; y++) g.rect(16 - Math.round(y * 0.55), 16 + y, Math.round(y * 1.1) + 1, 1, y < 7 ? "#f4f4f4" : (y % 4 ? "#566c86" : "#333c57")); g.rect(0, 44, 32, 4, "#257179"); },
    (g) => { g.rect(0, 0, 32, 48, "#0f1024"); for (let i = 0; i < 26; i++) g.px((i * 29) % 32, (i * 17) % 48, i % 4 ? "#8b9bb4" : "#f4f4f4"); g.disc(16, 24, 9, "#ef7d57"); g.disc(14, 22, 6, "#ffcd75");
      g.rect(3, 24, 26, 2, "#c9b8ff"); g.rect(5, 26, 22, 1, "#6a5acd"); g.disc(25, 9, 2, "#a7f0f0"); },
  ];
  return paint.map((p) => {
    const g = Grove.cv(32, 48); p(g);
    const o = document.createElement("canvas"); o.width = 384; o.height = 576;
    const x = o.getContext("2d"); x.imageSmoothingEnabled = false; x.drawImage(g.c, 0, 0, 384, 576);
    return o.toDataURL();
  });
});
const coverDir = mkdtempSync(`${tmpdir()}/steamdrop-covers-`);
const coverFiles = covers.map((c, i) => { const f = `${coverDir}/cover${i}.png`; writeFileSync(f, Buffer.from(c.split(",")[1], "base64")); return `file://${f}`; });

// Feature icons, 16 x 16, drawn pixel by pixel and saved 4 times as big.
const ICONS = {
  folder: ["                ", " aaaaa          ", "ayyyyya         ", "ayyyyyyaaaaaaaa ", "ayyyyyyyyyyyyyya", "aaaaaaaaaaaaaaaa", "awwwwwwwwwwwwwwa", "afffffffffffffya",
    "afffffffffffffya", "affffffbbfffffya", "afffffbccbffffya", "affffffbbfffffya", "ayyyyyyyyyyyyyya", "aaaaaaaaaaaaaaaa", "                ", "                "],
  art: ["                ", "aaaaaaaaaaaaaaaa", "ahhhhhhhhhhhhhha", "ahaaaaaaaaaaaaha", "ahauuuuuuuuwwaha", "ahauuuuuuuuwwaha", "ahauuuuuuuuuuaha", "ahauuuuuduuuuaha",
    "ahauuuuddduuuaha", "ahaucuddddduuaha", "ahaccccddddddaha", "ahabbbbbbbbbbaha", "ahaaaaaaaaaaaaha", "ahhhhhhhhhhhhhha", "aaaaaaaaaaaaaaaa", "                "],
  pad: ["                ", "                ", "                ", "   aaaaaaaaaa   ", "  avvvvvvvvvva  ", " avvavvvvvvvrva ", "avvaaavvvvvbvsva", "avvvavvvvvvvuvva",
    "avvvvvvvvvvvvvva", "avvvvvaaaavvvvva", "avvvva    avvvva", " avva      avva ", "  aa        aa  ", "                ", "                ", "                "],
  select: ["                ", " aaaaaa         ", " agggga aaaaaaaa", " aggbga ahhhhhha", " abgbga aaaaaaaa", " agbgga         ", " aaaaaa         ", "                ",
    " aaaaaa         ", " agggga aaaaaaaa", " aggbga ahhhhhha", " abgbga aaaaaaaa", " agbgga         ", " aaaaaa         ", "                ", "                "],
  safe: ["                ", "  aaaaaaaaaaaa  ", " acccccccbbbbba ", " accbbbbbbbbbda ", " acbbbbbbbbgbda ", " acbbbbbbbggbda ", " acbbgbbbggbbda ", " acbbggbggbbbda ",
    " acbbbgggbbbbda ", "  acbbbgbbbbda  ", "  abbbbbbbbbda  ", "   abbbbbbbda   ", "    abbbbbda    ", "     abbbda     ", "      aaaa      ", "                "],
  apple: ["                ", "         aaa    ", "       aacca    ", "       aiabda   ", "   aaaaiaaaaa   ", "  assssirrrrra  ", " asswwsrrrrrrra ", " aswwssrrrrrrra ",
    " asssrrrrrrrrqa ", " asrrrrrrrrrrqa ", " arrrrrrrrrrqqa ", "  arrrrrrrrqqa  ", "  aqrrrrrrqqqa  ", "   aqqaaaqqqa   ", "    aa   aaa    ", "                "],
  themes: ["                ", "    aaaaaaa     ", "  aagggggggaa   ", " aggrrggggffgga ", "aggrrrggggfffgga", "agggrggggggfggga", "aguugggaaaggggga", "auuuuga   aggga ",
    "agguuga   agbbga", "agggggaaaaagbbga", " aggggggggggggga", "  aaggggggggaa  ", "    aaaaaaaa    ", "                ", "                ", "                "],
  sound: ["                ", "       aaa      ", "       afaa     ", "       affaa    ", "       afaffa   ", "       afa aafa ", "       afa   afa", "       afa   aa ",
    "       afa      ", "       afa      ", "    aaaafa      ", "   affffffa     ", "  afwffffffa    ", "  affffffffa    ", "   affffffa     ", "    aaaaaa      "],
};
const INK = { a: "#0e161e", f: "#f2c14e", y: "#c8902e", w: "#fff3c0", g: "#f0e4c4", h: "#d2c4a0", b: "#4a9c54", c: "#96d26e", d: "#266842",
  r: "#c0392b", s: "#e8623c", q: "#7a1f20", i: "#684228", u: "#5cc8ff", v: "#8b9bb4", e: "#f08caa" };
for (const [name, rows] of Object.entries(ICONS)) {
  save(`icons/${name}.png`, await art.evaluate(([rows, INK]) => {
    const c = document.createElement("canvas"); c.width = c.height = 64;
    const x = c.getContext("2d");
    rows.forEach((row, y) => [...row].forEach((ch, i) => { if (ch !== " ") { x.fillStyle = INK[ch]; x.fillRect(i * 4, y * 4, 4, 4); } }));
    return c.toDataURL();
  }, [rows, INK]));
}

// A scene: the forest, the apple tree with apples and the logo, all on a grid of `k`-pixel squares,
// then the name and tagline. `textless` leaves the words out (the social preview's SVG adds them as text).
const scene = (o) => art.evaluate(async ({ w, h, k, ground, tree, logo, name, tagline, textless }) => {
  const P = Grove.palette(), g = Grove.cv(w, h);
  g.x.drawImage(Grove.picture(w, h, P, { ground }), 0, 0);
  const host = document.createElement("div"); host.style.width = `${tree.w}px`; document.body.append(host);
  const t = Grove.tree(host, { w: tree.w, h: tree.h, scale: tree.scale });
  t.motion(false, true); t.set([1, 2, 3, 4, 5, 6, 7]);
  g.x.drawImage(host.querySelector("canvas"), tree.x, h - ground - tree.h + 6);
  host.remove();
  g.x.drawImage(Grove.icon(32), logo[0], logo[1]);
  const o = document.createElement("canvas"); o.width = w * k; o.height = h * k;
  const x = o.getContext("2d"); x.imageSmoothingEnabled = false; x.drawImage(g.c, 0, 0, w * k, h * k);
  if (!textless) {
    x.textBaseline = "alphabetic";
    x.font = `600 ${name[2]}px "Pixelify Sans"`;
    x.fillStyle = P["sky-0"]; x.fillText("SteamDrop", name[0], name[1] + Math.round(name[2] / 12));
    x.fillStyle = "#f1ead2"; x.fillText("SteamDrop", name[0], name[1]);
    x.font = `700 ${tagline[2]}px "Atkinson Hyperlegible"`; x.fillStyle = "#c3eb8f";
    x.fillText("Drop a game folder in. Play it from Steam.", tagline[0], tagline[1]);
  }
  return o.toDataURL();
}, o);

// The banner at the top of the README.
save("banner.png", await scene({ w: 320, h: 96, k: 4, ground: 13, tree: { w: 100, h: 92, scale: 0.5, x: 16 }, logo: [116, 22], name: [632, 196, 100], tagline: [466, 276, 34] }));

// The social preview (1280 x 640, everything inside the centered 1200 x 600): a pixel picture with the
// words as real text in Pixelify Sans and Atkinson Hyperlegible, both embedded, so the SVG stands alone.
const picture = await scene({ w: 320, h: 160, k: 1, ground: 20, tree: { w: 150, h: 150, scale: 0.85, x: 6 }, logo: [160, 26], textless: true });
const font = (f) => readFileSync(`${root}ui/fonts/${f}`).toString("base64");
const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="640" viewBox="0 0 1280 640">
  <style>
    @font-face { font-family: "Pixelify Sans"; font-weight: 600; src: url(data:font/ttf;base64,${font("PixelifySans.ttf")}); }
    @font-face { font-family: "Atkinson Hyperlegible"; font-weight: 700; src: url(data:font/ttf;base64,${font("AtkinsonHyperlegible-Bold.ttf")}); }
  </style>
  <image width="1280" height="640" style="image-rendering: pixelated" href="${picture}"/>
  <text x="640" y="${330 + 8}" font-family="Pixelify Sans" font-weight="600" font-size="100" fill="#0c1418">SteamDrop</text>
  <text x="640" y="330" font-family="Pixelify Sans" font-weight="600" font-size="100" fill="#f1ead2">SteamDrop</text>
  <text font-family="Atkinson Hyperlegible" font-weight="700" font-size="42" fill="#c3eb8f">
    <tspan x="644" y="410">Drop a game folder in.</tspan>
    <tspan x="644" y="464">Play it from Steam.</tspan>
  </text>
</svg>
`;
writeFileSync(docs("social-preview.svg"), svg); console.log("docs/social-preview.svg");
const sp = await browser.newPage({ viewport: { width: 1280, height: 640 } });
await sp.goto(`file://${docs("social-preview.svg")}`); await sp.evaluate(() => document.fonts.ready); await sp.waitForTimeout(300);
await sp.screenshot({ path: docs("social-preview.png") }); console.log("docs/social-preview.png");
await sp.close(); await art.close();

/* ---------- the real app, with a stand-in for SteamDrop's backend and made-up games ---------- */
const NAMES = ["Moonlit Hills", "Tidewater", "Lanternlight", "Burrow & Bramble", "Skyreach", "Starfarer", "Night Ferry", "Sunset Shore", "Old Lamp", "Deep Burrow", "Cloud Peak", "Ring World"];
const game = (i) => ({ id: i + 1, name: NAMES[i], exe: `D:\\Games\\${NAMES[i]}\\game.exe`, folder: `D:\\Games\\${NAMES[i]}`, steam_appid: null, added: 100 - i,
  portrait: "x", hero: "x", in_steam: true, folder_missing: false });
const SETTINGS = { watched: ["D:\\Games"], excluded: [], auto_add: true, auto_restart: true, notify: true, run_on_startup: true, close_to_tray: true, steam_path: "",
  art_in_game_folder: false, theme: "dark", update_on_startup: true, update_daily: true, update_auto_install: false, skipped_version: "", last_seen_version: "0.1.0",
  steam_shortcut: true, steam_view: "auto", all_accounts: true, accounts: [], chime: true, chime_volume: 70, chime_sound: "soft", big_picture_at_sign_in: false,
  steam_restart_mode: "keep", zoom_window: 100, zoom_couch: 100, keep_big_picture: false, keep_big_picture_except: [], couch_setup_offered: true, couch_setup_undo: null,
  tree_breeze: true, tree_front_only: true };
const state = (n) => ({ settings: SETTINGS, games: Array.from({ length: n }, (_, i) => game(i)), found: [], gone: [],
  steam: { path: "C:\\Program Files (x86)\\Steam", users: 1, running: true, accounts: [] }, status: "", version: "0.1.0", update: null, whats_new: null, restart_needed: false });
async function app(file, n, viewport, open = []) {
  const page = await browser.newPage({ viewport, deviceScaleFactor: viewport.scale || 1 });
  await page.clock.install();
  await page.addInitScript(([S, covers, open]) => {
    window.__S = S; document.hasFocus = () => true;
    try { localStorage.setItem("steamdrop-open-sections", JSON.stringify(open)); } catch { /* fine */ }
    window.__TAURI__ = { core: { convertFileSrc: (p) => covers[(Number(p.split("/")[0]) - 1) % covers.length],
      invoke: async (cmd, args) => cmd === "get_state" ? JSON.parse(JSON.stringify(window.__S)) : cmd === "get_activity" ? [] : cmd === "get_sgdb_key" ? "" : null },
      event: { listen: async () => {} } };
  }, [state(n), coverFiles, open]);
  await page.goto(`file://${root}ui/${file}`);
  await page.evaluate(() => document.fonts.ready);
  await page.clock.runFor(1500);
  return page;
}
const shot = async (name, page) => { await page.screenshot({ path: docs(name) }); console.log(`docs/${name}`); await page.close(); };
await shot("screenshot-couch.png", await app("couch.html", 8, { width: 1920, height: 1080 }));
await shot("screenshot-empty.png", await app("couch.html", 0, { width: 1920, height: 1080 }));
await shot("screenshot-games.png", await app("index.html", 8, { width: 1280, height: 800, scale: 2 }));
const settings = await app("index.html", 8, { width: 1280, height: 800, scale: 2 }, ["appearance"]);
await settings.click(".tab[data-view=settings]"); await settings.clock.runFor(500);
await settings.evaluate(() => [...document.querySelectorAll("details.section")].find((d) => d.open).previousElementSibling.scrollIntoView());
await settings.clock.runFor(500);
await shot("screenshot-settings.png", settings);

// The GIF: the breeze, a new game blossoming into an apple, then a scroll. 12 frames a second, like the app.
const frames = mkdtempSync(`${tmpdir()}/steamdrop-gif-`);
const page = await app("couch.html", 11, { width: 1280, height: 720 });
let f = 0;
const clip = { x: 0, y: 0, width: 1280, height: 720 };
const film = async (n) => { for (let i = 0; i < n; i++) { await page.clock.runFor(84); await page.screenshot({ path: `${frames}/${String(f++).padStart(4, "0")}.png`, clip }); } };
await film(24);
await page.evaluate((g) => { window.__S.games.unshift(g); refresh(); }, game(11));
await film(30);
for (let i = 0; i < 6; i++) { await page.evaluate(() => document.querySelector("#screen").scrollBy({ top: 40, behavior: "instant" })); await film(1); }
await film(22);
await page.close();
execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-framerate", "12", "-i", `${frames}/%04d.png`, "-filter_complex",
  "[0]split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle", docs("tree.gif")]);
rmSync(frames, { recursive: true, force: true }); rmSync(coverDir, { recursive: true, force: true });
console.log("docs/tree.gif");
await browser.close();
