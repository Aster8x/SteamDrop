"use strict";
// The Night Grove look: pixel art drawn in code on small canvases (whole pixels, the theme's
// colors) and scaled up without smoothing. Used by both views, and by scripts/render-art.mjs for
// the app icon, the Steam art and the installer pictures.
window.Grove = (() => {
  /* ---------- drawing ---------- */
  function cv(w, h) {
    const c = document.createElement("canvas");
    c.width = w; c.height = h;
    const x = c.getContext("2d");
    const g = {
      c, w, h, x,
      px(px, py, col) { x.fillStyle = col; x.fillRect(Math.round(px), Math.round(py), 1, 1); },
      rect(rx, ry, rw, rh, col) { x.fillStyle = col; x.fillRect(Math.round(rx), Math.round(ry), Math.round(rw), Math.round(rh)); },
      disc(cx, cy, r, col) { for (let dy = -r; dy <= r; dy++) { const s = Math.floor(Math.sqrt(r * r - dy * dy + r * 0.8)); g.rect(cx - s, cy + dy, 2 * s + 1, 1, col); } },
      line(x0, y0, x1, y1, col) { const n = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0)) || 1; for (let i = 0; i <= n; i++) g.px(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n, col); },
      // Horizontal color bands with a dithered edge between them.
      bands(y0, y1, cols) {
        const n = cols.length, hgt = (y1 - y0) / n;
        for (let y = y0; y < y1; y++) {
          const t = (y - y0) / hgt, i = Math.min(n - 1, Math.floor(t)), f = t - i;
          g.rect(0, y, w, 1, cols[i]);
          if (f > 0.72 && i < n - 1) for (let px = (y % 2); px < w; px += (f > 0.86 ? 2 : 4)) g.px(px, y, cols[i + 1]);
        }
      },
    };
    return g;
  }
  // Tiny repeatable random, so the same picture is drawn every time.
  function rnd(seed) { let s = seed; return () => (s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff; }

  /* ---------- colors: the theme's CSS variables, as canvas colors ---------- */
  const NAMES = ["sky-0", "sky-1", "sky-2", "sky-3", "far", "mid", "ground", "soil", "moss", "bark", "bark-d", "leaf-d", "leaf", "leaf-l", "leaf-hi", "fruit", "fruit-hi", "blossom", "glow", "star"];
  function palette(el = document.documentElement) {
    const probe = document.createElement("i");
    el.append(probe);
    const P = {};
    for (const n of NAMES) { probe.style.color = `var(--${n})`; P[n] = getComputedStyle(probe).color; }
    probe.remove();
    return P;
  }

  /* ---------- plants and land ---------- */
  function pine(g, cx, base, hgt, col) {
    g.rect(cx - 1, base - 3, 2, 3, col);
    for (let y = 0; y < hgt; y++) { const tier = y % Math.max(4, Math.round(hgt / 4)); const wdt = Math.round((y / hgt) * hgt * 0.34 + tier * 0.5) + 1; g.rect(cx - wdt, base - 3 - hgt + y, wdt * 2, 1, col); }
  }
  function grass(g, y, col, colL, seed = 3) { const r = rnd(seed); for (let x = 0; x < g.w; x++) { const hgt = 1 + Math.floor(r() * 3); g.rect(x, y - hgt, 1, hgt, r() > 0.7 ? colL : col); } }
  function stars(g, n, y1, col, seed) { const r = rnd(seed); for (let i = 0; i < n; i++) g.px(r() * g.w, r() * y1, col); }
  function hills(g, y, amp, col, seed, len) { const r = rnd(seed); const p = [r() * 6, r() * 6]; for (let x = 0; x < g.w; x++) { const hy = Math.round(y + Math.sin(x / len + p[0]) * amp + Math.sin(x / (len / 2.7) + p[1]) * amp * 0.4); g.rect(x, hy, 1, g.h - hy, col); } }
  function mushroom(g, x, y, P) { g.rect(x, y - 3, 1, 3, P.blossom); g.rect(x - 2, y - 5, 5, 2, P.fruit); g.rect(x - 1, y - 6, 3, 1, P.fruit); g.px(x - 1, y - 5, P["fruit-hi"]); }
  function flower(g, x, y, P) { g.rect(x, y - 3, 1, 3, P.leaf); g.px(x, y - 4, P.blossom); g.px(x - 1, y - 3, P.blossom); g.px(x + 1, y - 3, P.blossom); }

  /** The night sky, far woods and the ground, `ground` pixels high at the bottom. */
  function land(g, P, ground = 48) {
    const top = g.h - ground;
    g.bands(0, top - 7, [P["sky-0"], P["sky-1"], P["sky-2"], P["sky-3"]]);
    g.rect(0, top - 7, g.w, 7, P["sky-3"]);
    stars(g, g.w * g.h / 1900, top * 0.7, P.star, 5); stars(g, g.w * g.h / 6000, top * 0.55, P.blossom, 11);
    hills(g, top - 44, 9, P.far, 3, 70);
    for (let x = 20; x < g.w; x += 29) pine(g, x + (x % 11), top - 26 + (x % 7), 26 + (x % 5) * 5, P.far);
    hills(g, top - 17, 5, P.mid, 6, 50);
    for (let x = 9; x < g.w; x += 61) pine(g, x + (x % 13), top - 4, 20 + (x % 4) * 4, P.mid);
    g.rect(0, top, g.w, ground, P.ground); g.rect(0, top + 14, g.w, ground, P.soil); grass(g, top + 1, P.moss, P.leaf, 4);
    const r = rnd(21);
    for (let x = 40; x < g.w; x += 30 + Math.floor(r() * 50)) (r() > 0.5 ? mushroom : flower)(g, x, top + 2, P);
  }
  function moon(g, x, y, r, P) { g.disc(x, y, r, P.glow); g.disc(x + Math.round(r * 0.45), y - Math.round(r * 0.3), Math.round(r * 0.82), P["sky-0"]); }

  // Where the tree's trunk and leaves go, for a tree `size` pixels tall standing on `base`.
  const BLOBS = [[0, 0, 1.25], [-1.25, 0.35, 0.95], [1.3, 0.3, 1.0], [-0.65, -0.85, 0.95], [0.75, -0.9, 0.9], [0, 0.75, 1.0], [-1.8, 1.0, 0.6], [1.9, 0.95, 0.62]];
  function shape(cx, base, size) { const th = Math.round(size * 0.55); return { cx, base, size, th, top: base - th - size * 0.28, R: size * 0.2 }; }
  function drawTree(g, s, P) {
    const { cx, base, size, th, top, R } = s;
    for (let y = 0; y < th; y++) {
      const wdt = Math.round(2 + (y / th) * (size * 0.09) + (y > th - 4 ? (y - th + 4) : 0));
      g.rect(cx - wdt, base - th + y, wdt * 2, 1, P.bark); g.rect(cx + wdt - 2, base - th + y, 2, 1, P["bark-d"]);
    }
    g.line(cx - 1, base - th + 6, cx - size * 0.22, base - th - size * 0.1, P.bark);
    g.line(cx + 1, base - th + 4, cx + size * 0.25, base - th - size * 0.12, P["bark-d"]);
    for (const [dx, dy, k] of BLOBS) g.disc(Math.round(cx + dx * R), Math.round(top + dy * R + 1), Math.round(k * R), P["leaf-d"]);
    for (const [dx, dy, k] of BLOBS) g.disc(Math.round(cx + dx * R - 1), Math.round(top + dy * R - 1), Math.round(k * R - 1), P.leaf);
    for (const [dx, dy, k] of BLOBS) g.disc(Math.round(cx + dx * R - 2), Math.round(top + dy * R - k * R * 0.45), Math.round(k * R * 0.5), P["leaf-l"]);
    const r = rnd(7);
    for (let i = 0; i < size * 1.2; i++) { const a = r() * 6.28, d = r() * R * 1.9; g.px(cx + Math.cos(a) * d, top + Math.sin(a) * d * 0.8 - 2, r() > 0.5 ? P["leaf-hi"] : P["leaf-d"]); }
  }
  /** Places for `n` fruits in the crown: a few chosen by hand, then more scattered, never too close. */
  function spots(s, n) {
    const out = [[-1.3, 0.5], [0.9, -0.6], [0.1, 0.9], [-0.5, -0.9], [1.6, 0.7], [-1.9, 1.1], [0.5, 0.1], [-0.9, 0.0]].map(([x, y]) => [Math.round(s.cx + x * s.R), Math.round(s.top + y * s.R)]);
    const r = rnd(99), gap = Math.max(5, s.size / 16);
    for (let tries = 0; out.length < n && tries < 4000; tries++) {
      const [dx, dy, k] = BLOBS[Math.floor(r() * BLOBS.length)], a = r() * 6.28, d = r() * k * s.R * 0.8;
      const p = [Math.round(s.cx + dx * s.R + Math.cos(a) * d), Math.round(s.top + dy * s.R + Math.sin(a) * d)];
      if (out.every((o) => Math.hypot(o[0] - p[0], o[1] - p[1]) >= gap)) out.push(p);
    }
    return out.slice(0, n);
  }
  // One fruit. `grown` runs from 0 (a bud) through 0.5 (an open blossom) to 1 (a ripe fruit).
  function fruit(g, x, y, r, grown, P) {
    if (grown < 0.5) { const k = grown * 2; g.px(x, y, P.blossom); if (k > 0.3) { g.rect(x - 1, y, 3, 1, P.blossom); g.rect(x, y - 1, 1, 3, P.blossom); } if (k > 0.7) { g.px(x - 1, y - 1, P.blossom); g.px(x + 1, y + 1, P.blossom); g.px(x, y, P.glow); } return; }
    g.disc(x, y, grown < 0.75 ? Math.max(1, r - 1) : r, P.fruit); g.px(x - 1, y - 1, P["fruit-hi"]); g.px(x, y - r - 1, P.bark);
  }
  /** A young tree for an empty library: big enough to notice, with two buds waiting. */
  function sapling(g, cx, base, P, k = 1) {
    const q = (v) => Math.round(v * k);
    for (let y = 0; y < q(34); y++) g.rect(cx + Math.round(Math.sin(y / (9 * k)) * 1.5 * k) - q(1), base - y, y < q(12) ? q(3) : q(2), 1, y % 5 ? P.bark : P["bark-d"]);
    for (const [dx, dy, r] of [[-9, -22, 5], [8, -27, 6], [-2, -36, 7], [-10, -33, 4], [9, -37, 4]]) { g.disc(cx + q(dx), base + q(dy) + 1, q(r), P["leaf-d"]); g.disc(cx + q(dx) - 1, base + q(dy), q(r) - 1, P.leaf); g.disc(cx + q(dx) - 2, base + q(dy) - 2, Math.max(1, q(r) - 3), P["leaf-l"]); }
    fruit(g, cx + q(-4), base + q(-38), 2, 0.4, P); fruit(g, cx + q(9), base + q(-29), 2, 0.2, P);
    for (const [x, y] of [[-20, -30], [19, -44], [24, -16], [-16, -8]]) { g.px(cx + q(x), base + q(y), P.glow); g.px(cx + q(x) + 1, base + q(y), P["leaf-hi"]); }
  }

  /* ---------- the living tree ---------- */
  const still = () => matchMedia("(prefers-reduced-motion: reduce)").matches || document.hidden;
  const GROW = 1700, BOB = 420, FLY = 1900, FRAME = 80, MAX = 60;

  /**
   * A tree on its own canvas with one fruit per game id. `set(ids)` shows them (ids that are new
   * since the last call open as a blossom and ripen, with a few fireflies); `bob(id)` nudges that
   * game's fruit once. It only redraws while something is moving; at rest it uses no CPU.
   */
  function tree(host, { w, h, cx, size, ground = 0 }) {
    const g = cv(w, h);
    g.c.className = "grove-tree";
    host.replaceChildren(g.c);
    let P = null, base = null, s = null, ids = [], known = null, places = [];
    const moving = new Map();   // id -> { grow: start time } or { bob: start time }
    let flies = [], timer = 0;
    function paint(now) {
      g.x.clearRect(0, 0, w, h);
      g.x.drawImage(base.c, 0, 0);
      const r = Math.max(1, Math.round(s.size / 40));
      ids.slice(0, MAX).forEach((id, i) => {
        const m = moving.get(id), [x, y] = places[i];
        const grown = m?.grow != null ? Math.min(1, (now - m.grow) / GROW) : 1;
        const lift = m?.bob != null ? Math.round(Math.sin(Math.min(1, (now - m.bob) / BOB) * Math.PI) * 3) : 0;
        fruit(g, x, y - lift, r, grown, P);
      });
      for (const f of flies) { const t = (now - f.t) / FLY; g.px(f.x + Math.sin(t * 6 + f.p) * 5, f.y - t * 14, t < 0.8 ? P.glow : P["leaf-hi"]); }
    }
    function step() {
      const now = performance.now();
      for (const [id, m] of moving) if (now - (m.grow ?? m.bob) > (m.grow != null ? GROW : BOB)) moving.delete(id);
      flies = flies.filter((f) => now - f.t < FLY);
      paint(now);
      timer = moving.size || flies.length ? setTimeout(step, FRAME) : 0;
    }
    const run = () => { if (still()) { moving.clear(); flies = []; paint(performance.now()); } else if (!timer) step(); };
    // Nothing keeps moving behind another window or with "reduce motion" on: jump to the end.
    document.addEventListener("visibilitychange", () => { if (document.hidden && timer) { clearTimeout(timer); timer = 0; moving.clear(); flies = []; } else if (!document.hidden && base) paint(performance.now()); });
    return {
      /** Redraw for the current theme and games. */
      set(list) {
        P = palette();
        const n = Math.min(list.length, MAX);
        s = shape(cx, h - ground - (ground ? 1 : 0), size + Math.min(n, 12) * 2);
        base = cv(w, h);
        if (ground) { base.rect(0, h - ground, w, ground, P.ground); base.rect(0, h - ground + 8, w, ground, P.soil); grass(base, h - ground + 1, P.moss, P.leaf, 2); }
        if (n) drawTree(base, s, P); else sapling(base, cx, h - ground - 1, P, size / 52);
        places = spots(s, n);
        const now = performance.now();
        if (known) for (const id of list) if (!known.has(id)) {
          moving.set(id, { grow: now });
          const i = list.indexOf(id);
          if (places[i]) for (let k = 0; k < 6; k++) flies.push({ x: places[i][0] + (k - 3) * 3, y: places[i][1] + 4, t: now + k * 90, p: k });
        }
        known = new Set(list);
        ids = list;
        run();
      },
      bob(id) { if (!base || !ids.includes(id) || moving.has(id)) return; moving.set(id, { bob: performance.now() }); run(); },
    };
  }

  /** The backdrop of Couch mode: sky, woods and ground filling `canvas` at whole-pixel `scale`. */
  function backdrop(canvas, scale) {
    const g = cv(Math.ceil(innerWidth / scale), Math.ceil(innerHeight / scale)), P = palette();
    land(g, P);
    moon(g, Math.round(g.w * 0.34), 30, 11, P);
    canvas.width = g.w; canvas.height = g.h;
    canvas.getContext("2d").drawImage(g.c, 0, 0);
  }

  /* ---------- the icon and the pictures made from the same drawing ---------- */
  // 16 px by hand: at this size every pixel is chosen. L leaf, l light, D dark, F fruit, B bark, . sky, space clear.
  const ICON16 = [
    " .............. ", "......lll.......", "....llllLLL.....", "...lllLLLLLD....", "..LllLLLLLLDD...", "..LLLLLLLFFDD...", "..LLLLLLLFFDD...", "..DLLLLLLLLDD...",
    "...DDLLLLDDD....", ".....DBBDD......", "......BB........", "......BB........", "...BBBBBBBB.....", "....BBBBBB......", ".....BBBB.......", " .....BB....... ",
  ];
  /** The app icon on an n x n grid (16, 24 or 32): a round tree with one fruit; trunk and roots point down ("drop it in"). */
  function icon(n, P) {
    const g = cv(n, n);
    if (n === 16) {
      const col = { ".": P["sky-2"], L: P.leaf, l: P["leaf-l"], D: P["leaf-d"], F: P.fruit, B: P.bark };
      ICON16.forEach((row, y) => [...row].forEach((ch, x) => { if (col[ch]) g.px(x, y, col[ch]); }));
      return g.c;
    }
    const u = n / 16, R = (x, y, w, h, c) => g.rect(Math.round(x * u), Math.round(y * u), Math.max(1, Math.round(w * u)), Math.max(1, Math.round(h * u)), c);
    R(0, 0, 16, 16, P["sky-2"]);
    for (const [x, y] of [[0, 0], [n - 1, 0], [0, n - 1], [n - 1, n - 1]]) g.x.clearRect(x, y, 1, 1);
    g.disc(Math.round(8 * u), Math.round(6.2 * u), Math.round(4.6 * u), P["leaf-d"]);
    g.disc(Math.round(7.6 * u), Math.round(5.8 * u), Math.round(4.1 * u), P.leaf);
    g.disc(Math.round(6.6 * u), Math.round(4.6 * u), Math.round(2 * u), P["leaf-l"]);
    R(7, 9.5, 2, 3, P.bark); R(4.5, 12, 7, 1, P.bark); R(5.5, 13, 5, 1, P.bark); R(6.5, 14, 3, 1, P.bark); R(7.25, 15, 1.5, 1, P.bark);
    g.disc(Math.round(10 * u), Math.round(7 * u), Math.max(1, Math.round(1.1 * u)), P.fruit); g.px(Math.round(10 * u) - 1, Math.round(7 * u) - 1, P["fruit-hi"]);
    return g.c;
  }
  /** A whole scene (sky, woods, ground and a tree with fruit) for the Steam art and installer pictures. */
  function picture(w, h, P, { cx = w / 2, size = h * 0.56, ground = Math.round(h * 0.16), fruits = 6 } = {}) {
    const g = cv(w, h);
    land(g, P, ground);
    const s = shape(Math.round(cx), h - ground + 1, size);
    drawTree(g, s, P);
    spots(s, fruits).forEach(([x, y]) => fruit(g, x, y, Math.max(1, Math.round(size / 40)), 1, P));
    return g.c;
  }

  /** The young tree as a picture, for "nothing planted yet". */
  function young() {
    const g = cv(66, 58), P = palette();
    g.rect(0, 50, 66, 8, P.ground); g.rect(0, 55, 66, 3, P.soil); grass(g, 51, P.moss, P.leaf, 2);
    sapling(g, 33, 50, P);
    g.c.className = "grove-tree";
    return g.c;
  }

  return { palette, tree, backdrop, icon, picture, young };
})();
