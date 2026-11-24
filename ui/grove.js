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
  const NAMES = ["sky-0", "sky-1", "sky-2", "sky-3", "far", "mid", "ground", "soil", "moss", "bark-0", "bark-d", "bark-m", "bark", "bark-l",
    "leaf-0", "leaf-d", "leaf-m", "leaf", "leaf-l", "leaf-hi", "fruit-d", "fruit-m", "fruit", "fruit-hi", "blossom", "blossom-c", "outline", "fly", "fly-hi", "glow", "star"];
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

  /* ---------- the Orchard Apple: one apple per game ---------- */
  // Its own random and smooth noise, so the tree is the same every time.
  function rng(seed) { let s = seed >>> 0; return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296); }
  function noise(seed) {
    const r = rng(seed), T = new Float32Array(256 * 256);
    for (let i = 0; i < T.length; i++) T[i] = r();
    const at = (x, y) => T[((y & 255) << 8) | (x & 255)];
    const sm = (t) => t * t * (3 - 2 * t);
    return (x, y) => {
      const xi = Math.floor(x), yi = Math.floor(y), xf = sm(x - xi), yf = sm(y - yi);
      const a = at(xi, yi), b = at(xi + 1, yi), c = at(xi, yi + 1), d = at(xi + 1, yi + 1);
      return a + (b - a) * xf + (c - a) * yf + (a - b - c + d) * xf * yf;
    };
  }
  // The tree's color ramps, darkest first, from the theme.
  function colors() {
    const P = palette();
    return {
      L: [P["leaf-0"], P["leaf-d"], P["leaf-m"], P.leaf, P["leaf-l"], P["leaf-hi"]],
      B: [P["bark-0"], P["bark-d"], P["bark-m"], P.bark, P["bark-l"]],
      F: [P["fruit-d"], P["fruit-m"], P.fruit, P["fruit-hi"]],
      petal: P.blossom, heart: P["blossom-c"], O: P.outline, fly: [P.fly, P["fly-hi"]],
    };
  }

  // Built once per theme and size; a frame only moves the cached leaf clusters.
  function build(C, w, h, scale) {
    const r = rng(492), N = noise(20), S = (v) => v * scale;
    const base = { x: Math.round(w / 2), y: h - 4 };
    const T = { C, w, h, base, clusters: [], branches: [], spots: [] };
    const trunkH = Math.round(S(62)), w0 = S(6.5);
    const bend = (t) => Math.round(S(-2) * Math.sin(t * Math.PI * 0.7));
    const halfW = (t) => w0 * (1 - 0.45 * t) + S(7) * Math.exp(-t * trunkH / S(4.5));
    T.trunk = { trunkH, bend, halfW };
    const top = { x: base.x + bend(1), y: base.y - trunkH };
    // Limbs from the trunk up into the crown.
    for (const [side, at, dx, dy, th] of [[-0.3, 0.7, -22, -26, 3.4], [0.3, 0.7, 20, -28, 3.4], [0, 0.96, 2, -30, 2.6]]) {
      const sx = base.x + bend(at) + Math.round(side * halfW(at)), sy = base.y - Math.round(trunkH * at);
      T.branches.push({ x0: sx, y0: sy, x1: sx + S(dx), y1: sy + S(dy), th: S(th), mx: sx + S(dx) * 0.45 + S(dx > 0 ? 6 : -6), my: sy + S(dy) * 0.6 });
    }
    // Leaf clusters: lumps along the crown's edge make a scalloped outline; big darker ones fill the inside.
    const crown = { cx: top.x, cy: top.y - S(30), rx: S(46), ry: S(40) }, EDGE = 9, LUMP = 0.42, INNER = 4;
    const placed = [];
    for (let i = 0; i < EDGE; i++) {
      const a = Math.PI * 0.8 + Math.PI * 1.4 * (i + 0.5) / EDGE + (r() - 0.5) * 0.18;
      const x = crown.cx + Math.cos(a) * crown.rx * 0.78, y = crown.cy + Math.sin(a) * crown.ry * 0.78;
      const rad = Math.min(crown.rx, crown.ry) * LUMP * (0.85 + r() * 0.35) * (Math.sin(a) > 0.3 ? 0.85 : 1.15);
      placed.push({ x, y, rad, layer: Math.sin(a) > 0.2 ? 2 : 1, seed: Math.floor(r() * 1e6) });
    }
    placed.push({ x: crown.cx, y: crown.cy, rad: crown.ry * 0.7, layer: 0, seed: Math.floor(r() * 1e6) });
    for (let i = 0; i < INNER; i++) {
      const a = (i / INNER) * Math.PI * 2 + r() * 0.4;
      placed.push({ x: crown.cx + Math.cos(a) * crown.rx * 0.42, y: crown.cy + Math.sin(a) * crown.ry * 0.35, rad: crown.ry * 0.58, layer: 0, seed: Math.floor(r() * 1e6) });
    }
    for (let i = 0; i < 3; i++) {
      const a = Math.PI * (1.25 + i * 0.25) + (r() - 0.5) * 0.2;
      placed.push({ x: crown.cx + Math.cos(a) * crown.rx * 0.55, y: crown.cy + Math.sin(a) * crown.ry * 0.95, rad: Math.min(crown.rx, crown.ry) * LUMP, layer: 0, seed: Math.floor(r() * 1e6) });
    }
    placed.sort((a, b) => a.layer - b.layer || a.y - b.y);
    for (const p of placed) T.clusters.push(cluster(p, C));
    T.top = Math.min(...T.clusters.map((c) => c.y));
    // Places for apples on the front clusters, ordered so each new one lands far from the others.
    const fr = rng(12);
    for (let tries = 0; tries < 4000 && T.spots.length < MAX; tries++) {
      const c = T.clusters[Math.floor(fr() * T.clusters.length)];
      if (c.layer === 0) continue;
      const a = fr() * Math.PI * 2, d = c.rad * (0.15 + fr() * 0.5);
      const x = Math.round(c.x + Math.cos(a) * d), y = Math.round(c.y + Math.sin(a) * d * 0.8 + c.rad * 0.15);
      if (T.spots.every((s) => Math.hypot(s.x - x, s.y - y) > S(9))) T.spots.push({ cluster: c, x, y });
    }
    const pool = T.spots.splice(0), cx0 = crown.cx, cy0 = crown.cy + crown.ry * 0.3;
    pool.sort((a, b) => Math.hypot(a.x - cx0, a.y - cy0) - Math.hypot(b.x - cx0, b.y - cy0));
    if (pool.length) T.spots.push(pool.shift());
    while (pool.length) {
      let best = 0, bd = -1;
      pool.forEach((p, i) => { const d = Math.min(...T.spots.map((q) => Math.hypot(p.x - q.x, p.y - q.y))); if (d > bd) { bd = d; best = i; } });
      T.spots.push(pool.splice(best, 1)[0]);
    }
    T.static = trunk(T, N);
    return T;
  }

  // One cluster as a sprite: lumps lit from the upper left, small leaf shapes, a light rim and a dark outline.
  function cluster(p, C) {
    const L = C.L, r = rng(p.seed), R = Math.ceil(p.rad * 1.3) + 3;
    const g = cv(R * 2, R * 2), cx = R, cy = R;
    const blobs = [{ x: 0, y: p.rad * 0.08, r: p.rad * 0.7 }];
    const k = 6 + Math.floor(r() * 3);
    for (let i = 0; i < k; i++) { const a = (i / k) * Math.PI * 2 + r() * 0.5; blobs.push({ x: Math.cos(a) * p.rad * 0.5, y: Math.sin(a) * p.rad * 0.42 - p.rad * 0.06, r: p.rad * (0.42 + r() * 0.14) }); }
    blobs.sort((a, b) => a.y - b.y);
    const tone = new Int8Array(g.w * g.h).fill(-1), owner = new Int8Array(g.w * g.h).fill(-1);
    blobs.forEach((b, bi) => {
      for (let y = Math.floor(cy + b.y - b.r); y <= cy + b.y + b.r; y++) for (let x = Math.floor(cx + b.x - b.r); x <= cx + b.x + b.r; x++) {
        if (x < 0 || y < 0 || x >= g.w || y >= g.h) continue;
        const dx = (x - cx - b.x) / b.r, dy = (y - cy - b.y) / b.r, d = Math.hypot(dx, dy);
        const wob = 1 + 0.1 * Math.sin(Math.atan2(dy, dx) * 7 + bi * 1.7);
        if (d > wob) continue;
        const lit = -(dx * -0.6 + dy * -0.8) + (1 - d) * 0.35;
        let t = lit > 0.62 ? 4 : lit > 0.22 ? 3 : lit > -0.25 ? 2 : 1;
        if (d > wob - 0.16 && lit < -0.1) t = 1;
        tone[y * g.w + x] = t; owner[y * g.w + x] = bi;
      }
    });
    for (let y = 0; y < g.h; y++) for (let x = 0; x < g.w; x++) {
      const i = y * g.w + x, t = tone[i];
      if (t < 2) continue;
      const row = Math.floor(y / 3), col = Math.floor((x + (row % 2) * 2) / 4), lx = (x + (row % 2) * 2) % 4, ly = y % 3;
      const hh = Math.sin(row * 12.9898 + col * 78.233 + p.seed) * 43758.5453 % 1;
      if (Math.abs(hh) < 0.45) continue;
      if (ly === 0 && (lx === 1 || lx === 2)) tone[i] = Math.min(5, t + (t >= 3 && Math.abs(hh) > 0.8 ? 2 : 1));
      else if (ly === 1 && lx === 2) tone[i] = Math.max(1, t - 1);
    }
    const out = (xx, yy) => xx < 0 || yy < 0 || xx >= g.w || yy >= g.h || tone[yy * g.w + xx] < 0;
    for (let y = 0; y < g.h; y++) for (let x = 0; x < g.w; x++) {
      const i = y * g.w + x, t = tone[i];
      if (t < 0) continue;
      if (out(x, y + 1) || out(x + 1, y) || out(x - 1, y) || out(x, y - 1)) { g.px(x, y, C.O); continue; }
      if ((out(x, y - 2) || out(x - 2, y)) && t >= 2) { g.px(x, y, L[Math.min(5, t + 1)]); continue; }
      const below = owner[(y + 1) * g.w + x];
      if (below >= 0 && below < owner[i] && tone[(y + 1) * g.w + x] >= 0 && owner[(y + 2) * g.w + x] !== owner[i]) { g.px(x, y, L[0]); continue; }
      g.px(x, y, L[t]);
    }
    return { ...p, sprite: g.c, ox: cx, oy: cy, phase: r() * 6.28 };
  }

  // Roots and the trunk with its grain and a knot, lit from the left: drawn once.
  function trunk(T, N) {
    const g = cv(T.w, T.h), B = T.C.B, { base } = T, { trunkH, bend, halfW } = T.trunk, rr = rng(31);
    for (const side of [-1, 1, -0.5, 0.6]) {
      const len = 10 + rr() * 10;
      for (let i = 0; i < len; i++) {
        const x = base.x + side * (halfW(0) * 0.6 + i * 1.1), y = base.y - 2 + Math.round(i * 0.18), th = Math.max(1, 3 - i / 5);
        for (let k = 0; k < th; k++) g.px(x, y - k, k === 0 ? B[1] : B[2 + (side < 0 ? 1 : 0)]);
      }
    }
    for (let y = 0; y < trunkH + 2; y++) {
      const t = y / trunkH, hw = halfW(Math.min(1, t)), cxl = base.x + bend(Math.min(1, t));
      for (let x = Math.floor(cxl - hw); x <= cxl + hw; x++) {
        const u = (x - cxl) / hw, gr = N(x * 0.9, (base.y - y) * 0.13);
        let k = u < -0.55 ? 4 : u < -0.05 ? 3 : u < 0.5 ? 2 : 1;
        if (gr > 0.72) k = Math.max(0, k - 2); else if (gr < 0.18 && k < 4) k += 1;
        if (Math.abs(u) > 0.92) k = 0;
        g.px(x, base.y - y, B[k]);
      }
    }
    const kx = base.x + bend(0.42) + 1, ky = base.y - Math.round(trunkH * 0.42);
    g.rect(kx - 1, ky - 2, 3, 1, B[0]); g.rect(kx - 2, ky - 1, 1, 2, B[0]); g.rect(kx + 2, ky - 1, 1, 2, B[1]); g.rect(kx - 1, ky + 1, 3, 1, B[1]); g.px(kx, ky - 1, B[4]);
    return g.c;
  }
  // A limb from trunk to crown, its tip following the crown as it sways.
  function limb(g, b, tipDx, B) {
    const n = Math.ceil(Math.hypot(b.x1 - b.x0, b.y1 - b.y0));
    for (let i = 0; i <= n; i++) {
      const t = i / n, u = 1 - t;
      const x = u * u * b.x0 + 2 * u * t * b.mx + t * t * (b.x1 + tipDx), y = u * u * b.y0 + 2 * u * t * b.my + t * t * b.y1;
      const th = Math.max(1, Math.round(b.th * (1 - t * 0.7)));
      for (let k = 0; k < th; k++) g.px(x, y - k, k === 0 ? B[1] : k === th - 1 ? B[4] : B[3]);
    }
  }
  // An apple; `grown` below 1 is a small green-to-red bud.
  function apple(g, C, x, y, grown) {
    const F = C.F;
    if (grown < 1) { const k = grown < 0.5 ? 1 : 2; g.rect(x - k + 1, y - k + 1, k, k, F[1]); return; }
    g.rect(x - 2, y - 1, 5, 3, F[1]); g.rect(x - 1, y - 2, 3, 1, F[1]); g.rect(x - 1, y + 2, 3, 1, F[0]);
    g.px(x - 2, y + 1, F[0]); g.px(x + 2, y + 1, F[0]); g.px(x + 2, y, F[0]);
    g.px(x - 1, y - 1, F[2]); g.px(x - 1, y - 2, F[3]); g.px(x, y - 1, F[2]);
    g.px(x, y - 3, C.B[1]); g.px(x + 1, y - 4, C.L[4]); g.px(x + 2, y - 4, C.L[3]);
  }
  function blossom(g, C, x, y, k) {
    g.px(x, y, C.petal);
    if (k > 0.3) { g.px(x - 1, y, C.petal); g.px(x + 1, y, C.petal); g.px(x, y - 1, C.petal); g.px(x, y + 1, C.petal); }
    if (k > 0.6) { g.px(x - 1, y - 1, C.petal); g.px(x + 1, y + 1, C.petal); g.px(x + 1, y - 1, C.petal); g.px(x - 1, y + 1, C.petal); g.px(x, y, C.heart); }
  }

  /* ---------- the living tree ---------- */
  // At most 12 frames a second, and only while it may move (see live()).
  const FRAME = 84, MAX = 40, GROW = 1700, BOB = 400, FLY = 1900;
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  let drawn = 0;

  /**
   * The Orchard Apple in `host`, one apple per game id (a young tree with no games). `set(ids)`: new
   * ids blossom into apples, removed ids lose theirs. `bob(id)`, `scroll(dy)`, `gust()` react;
   * clicking it shakes it. `motion(breeze, front)` applies the settings.
   */
  function tree(host, { w, h, scale = 1 }) {
    const g = cv(w, h), button = document.createElement("button");
    g.c.className = "grove-tree"; g.c.setAttribute("aria-hidden", "true");
    button.type = "button"; button.className = "grove-shake"; button.setAttribute("aria-label", "Shake the tree");
    button.append(g.c); host.replaceChildren(button);
    let T = null, look = "", slots = new Map(), known = null, breeze = true, front = true, timer = 0, last = 0, dropped = 0;
    const st = { t: 0, lean: 0, leanV: 0, rustle: 0, gust: 0, shake: 0, leaves: [], flies: [], grow: new Map(), bob: new Map() };
    const r = rng(99);
    // Drawing only while it shows, is in front (unless that setting is off) and may move at all.
    const calm = () => !breeze || reduced.matches;
    const live = () => !calm() && !document.hidden && (!front || document.hasFocus()) && host.getClientRects().length > 0;
    function drop(count) {
      for (let i = 0; i < count; i++) {
        const c = T.clusters[Math.floor(r() * T.clusters.length)];
        st.leaves.push({ x: c.x + (r() - 0.5) * c.rad, y: c.y + c.rad * (0.1 + r() * 0.5), vx: (r() - 0.5) * 0.2, ph: r() * 6, age: 0, life: 70 + r() * 50, col: 2 + Math.floor(r() * 3) });
      }
    }
    function sway(c) {
      if (calm()) return 0;
      const hf = Math.max(0.2, (T.base.y - c.y) / (T.base.y - T.top));
      const breath = Math.sin(st.t * 0.0016 + c.phase) * 0.9 + Math.sin(st.t * 0.0041 + c.phase * 2) * 0.35;
      const rust = st.rustle * Math.sin(st.t * 0.03 + c.phase * 3), shake = st.shake * Math.sin(st.t * 0.045 + c.phase);
      return Math.round((breath * (1 + st.gust * 2.2) + st.lean * 3 + rust * 1.4 + shake * 2.2) * hf);
    }
    // Moves everything on by `dt` ms and draws.
    function frame(dt) {
      drawn++;
      const C = T.C, f = dt / 83;
      st.t += dt;
      st.leanV += (-st.lean * 0.08 - st.leanV * 0.25) * f; st.lean += st.leanV * f;
      st.rustle *= Math.pow(0.9, f); st.gust *= Math.pow(0.93, f); st.shake *= Math.pow(0.86, f);
      if (dt && r() < 0.012 * f * (1 + st.gust * 6)) drop(1);
      g.x.clearRect(0, 0, w, h);
      const offs = T.clusters.map(sway);
      g.x.drawImage(T.static, 0, 0);
      const crownDx = Math.round(offs.reduce((a, b) => a + b, 0) / offs.length);
      T.branches.forEach((b) => limb(g, b, crownDx, C.B));
      T.clusters.forEach((c, i) => g.x.drawImage(c.sprite, Math.round(c.x - c.ox) + offs[i], Math.round(c.y - c.oy)));
      for (const [id, i] of slots) {
        const s = T.spots[i], dx = offs[T.clusters.indexOf(s.cluster)], b = st.bob.get(id), gr = st.grow.get(id);
        if (gr != null) {
          const k = (st.t - gr) / GROW;
          if (k < 0.45) blossom(g, C, s.x + dx, s.y, k / 0.45); else apple(g, C, s.x + dx, s.y, k < 0.7 ? 0.5 : k < 1 ? 0.9 : 1);
          if (k >= 1) st.grow.delete(id);
        } else apple(g, C, s.x + dx, s.y - (b != null ? Math.round(Math.sin(Math.min(1, (st.t - b) / BOB) * Math.PI) * 2) : 0), 1);
        if (b != null && st.t - b > BOB) st.bob.delete(id);
      }
      // Falling leaves drift and turn, then fade out with a dither.
      st.leaves = st.leaves.filter((l) => (l.age += f) < l.life);
      for (const l of st.leaves) {
        l.y += 0.32 * f; l.x += (Math.sin(l.age * 0.12 + l.ph) * 0.35 + st.lean * 0.4 + st.gust * 0.5 + l.vx) * f;
        if (l.age / l.life > 0.75 && (Math.round(l.age) + Math.round(l.x)) % 2) continue;
        g.px(l.x, l.y, C.L[l.col]); if (Math.sin(l.age * 0.25 + l.ph) > 0) g.px(l.x + 1, l.y, C.L[l.col]); else g.px(l.x, l.y + 1, C.L[1]);
      }
      st.flies = st.flies.filter((q) => st.t - q.t < FLY);
      for (const q of st.flies) {
        const k = (st.t - q.t) / FLY; if (k < 0) continue;
        const x = q.x + Math.sin(k * 7 + q.p) * 4, y = q.y - k * 16;
        if ((Math.round(st.t / 120) + q.p) % 3) { g.px(x, y, C.fly[0]); g.px(x + 1, y, C.fly[1]); }
      }
    }
    function step() {
      timer = 0;
      if (!T || !live()) return;
      const now = performance.now();
      frame(Math.min(FRAME * 2, now - last)); last = now;
      timer = setTimeout(step, FRAME);
    }
    const wake = () => { if (!timer && T && live()) { last = performance.now() - FRAME; step(); } };
    // After any change: animate if it may move; with the animation off show the end state; behind
    // other windows draw nothing (it catches up when SteamDrop comes back in front).
    function changed() {
      if (!T) return;
      if (live()) return wake();
      if (!calm()) return;
      st.lean = st.leanV = st.rustle = st.gust = st.shake = 0; st.leaves = []; st.flies = []; st.grow.clear(); st.bob.clear();
      frame(0);
    }
    const react = (fn) => { if (T && live()) { fn(); wake(); } };
    button.addEventListener("click", () => react(() => { st.shake = 1; drop(1 + Math.floor(r() * 2)); }));
    addEventListener("focus", wake);
    document.addEventListener("visibilitychange", wake);
    reduced.addEventListener("change", changed);
    // Whole screen pixels at any size; showing the tree again (Couch mode's Library) wakes it.
    new ResizeObserver(() => { if (host.clientWidth) { crisp(g.c, host.clientWidth); wake(); } }).observe(host);
    return {
      set(list) {
        const C = colors(), young = !list.length, key = JSON.stringify(C) + young;
        if (key !== look) { look = key; T = build(C, w, h, young ? scale * 0.55 : scale); }
        for (const id of slots.keys()) if (!list.includes(id)) { slots.delete(id); st.grow.delete(id); st.bob.delete(id); }
        const free = T.spots.map((_, i) => i).filter((i) => ![...slots.values()].includes(i));
        for (const id of list) {
          if (slots.has(id) || !free.length) continue;
          const i = free.shift(), s = T.spots[i];
          slots.set(id, i);
          if (known && !known.has(id)) {
            st.grow.set(id, st.t);
            for (let k = 0; k < 6; k++) st.flies.push({ x: s.x + (k - 3) * 3, y: s.y + 3, t: st.t + k * 90, p: k });
          }
        }
        known = new Set(list);
        changed();
      },
      bob(id) { if (slots.has(id) && !st.grow.has(id)) react(() => st.bob.set(id, st.t)); },
      scroll(dy) {
        react(() => {
          st.leanV = Math.max(-0.5, Math.min(0.5, st.leanV + Math.sign(dy) * 0.25)); st.rustle = 1;
          // One handful of leaves per scroll, not one per scroll event.
          if (performance.now() - dropped > 700) { dropped = performance.now(); drop(3 + Math.floor(r() * 3)); }
        });
      },
      gust() { react(() => { st.gust = 1; drop(2); }); },
      motion(b, f) { if (b === breeze && f === front) return; breeze = b; front = f; changed(); },
      button,
    };
  }

  /** The young tree as a still picture, for "nothing planted yet". */
  function young() {
    const C = colors(), T = build(C, 90, 96, 0.55), g = cv(90, 96);
    g.x.drawImage(T.static, 0, 0);
    T.branches.forEach((b) => limb(g, b, 0, C.B));
    T.clusters.forEach((c) => g.x.drawImage(c.sprite, Math.round(c.x - c.ox), Math.round(c.y - c.oy)));
    g.c.className = "grove-tree"; g.c.setAttribute("aria-hidden", "true");
    return crisp(g.c, 180);
  }

  /** Shows canvas `c` about `size` CSS pixels wide, enlarged by a whole number of screen pixels so it stays sharp. */
  function crisp(c, size) {
    if (size) c.dataset.size = size;
    const k = Math.max(1, Math.round(c.dataset.size * devicePixelRatio / c.width));
    c.style.width = `${k * c.width / devicePixelRatio}px`; c.style.height = `${k * c.height / devicePixelRatio}px`;
    return c;
  }
  // Windows scaling and the interface size change the screen pixel size.
  addEventListener("resize", () => document.querySelectorAll("canvas[data-size]").forEach((c) => crisp(c)));

  /** The backdrop of Couch mode: sky, woods and ground filling `canvas` at whole-pixel `scale`. */
  function backdrop(canvas, scale) {
    const g = cv(Math.ceil(innerWidth / scale), Math.ceil(innerHeight / scale)), P = palette();
    land(g, P);
    moon(g, Math.round(g.w * 0.34), 30, 11, P);
    canvas.width = g.w; canvas.height = g.h;
    canvas.style.width = `${g.w * scale}px`; canvas.style.height = `${g.h * scale}px`;
    canvas.getContext("2d").drawImage(g.c, 0, 0);
  }

  /* ---------- the logo, and the pictures made from the same drawing ---------- */
  // The "SD" logo with a vine and a flower on a strip of grass, hand-drawn pixel by pixel at 32, 24
  // and 16 px (the small ones are simplified so they stay readable). It keeps its own colors in
  // every theme. a outline, g/h cream light/dark, c/b green light/dark, d dark green, e/f flower, i soil.
  const LOGO_COLORS = { a: "#0e161e", b: "#4a9c54", c: "#96d26e", d: "#266842", e: "#f08caa", f: "#f2c14e", g: "#f0e4c4", h: "#d2c4a0", i: "#684228" };
  const LOGO32 = [
    "                                ",
    "                                ",
    "                                ",
    "                      a         ",
    "                  aa aba        ",
    "                 acbabca   aa   ",
    "                  aadaaba aefa  ",
    "                  ada  adaaeea  ",
    "   aaaaaaaaaa   aadaaaaaabcaa   ",
    "  agggggggggga acccccccccca     ",
    " aaggggggggggaaaccccccccccaa    ",
    "aggggaaaaaaggggaccccaaaacccca   ",
    "agggga    aggggacccca  accccaa  ",
    "agggga     aaaaacccca   aacccca ",
    "aggggaaaaaa    acccca    acccca ",
    " aahhhhhhhha   abbbba    abbbba ",
    "  ahhhhhhhhaaaaabbbba    abbbba ",
    "   aaaaaaaahhhhabbbba    abbbba ",
    " aaaa     ahhhhabbbba   aabbbba ",
    "ahhhha    ahhhhabbbba  abbbbaa  ",
    "ahhhhaaaaaahhhhabbbbaaaabbbba   ",
    " aahhhhhhhhhhaaabbbbbbbbbbaa    ",
    "  ahhhhhhhhhha abbbbbbbbbba     ",
    "   aaaaaaaaaa   aaaaaaaaaa      ",
    " aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ",
    "adddddddddddddddddddddddddddddda",
    "aiiiiiiiiiiiiiiiiiiiiiiiiiiiiiia",
    " aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ",
    "                                ",
    "                                ",
    "                                ",
    "                                ",
  ];
  const LOGO24 = [
    "                        ",
    "             a   a      ",
    "            aca aca  aa ",
    "            ada ada aefa",
    "            ada  adaaeea",
    "  aaaaaaaa  adaaaadaaaa ",
    " aggggggggaaccccccca    ",
    "agggaaaagggacccccccaa   ",
    "aggga  agggacccaaaccca  ",
    "aggga   aaaaccca  accca ",
    "agggaaaa   accca  accca ",
    " aahhhhhha abbba  abbba ",
    "  ahhhhhhaaabbba  abbba ",
    "   aaaaahhhabbba  abbba ",
    " aaa   ahhhabbba  abbba ",
    "ahhha  ahhhabbba abbba  ",
    "ahhhaaaahhhabbbaaabbba  ",
    " aahhhhhhhaabbbbbbbbaa  ",
    "  ahhhhhhhaabbbbbbbba   ",
    "   aaaaaaa  aaaaaaaa    ",
    " aaaaaaaaaaaaaaaaaaaaaa ",
    "adddddddddddddddddddddda",
    "aiiiiiiiiiiiiiiiiiiiiiia",
    " aaaaaaaaaaaaaaaaaaaaaa ",
  ];
  const LOGO16 = [
    "         a   aa ",
    "        aca aefa",
    "         adaaeea",
    "          d  aa ",
    " aaaaaaaaaaaaa  ",
    "aggggggacccccca ",
    "aggaaaaaccaaacca",
    "aggaaaaaccaaacca",
    "ahhhhhhabbaaabba",
    " aaaahhabbaaabba",
    "aaaaahhabbaaabba",
    "ahhhhhhabbbbbba ",
    " aaaaaaaaaaaaaa ",
    "adddddddddddddda",
    "aiiiiiiiiiiiiiia",
    " aaaaaaaaaaaaaa ",
  ];
  const LOGOS = { 16: LOGO16, 24: LOGO24, 32: LOGO32 };
  /** The logo on its own n x n grid (16, 24 or 32 pixels); enlarge it only by whole numbers. */
  function icon(n) {
    const g = cv(n, n);
    LOGOS[n].forEach((row, y) => [...row].forEach((ch, x) => { if (ch !== " ") g.px(x, y, LOGO_COLORS[ch]); }));
    return g.c;
  }
  /** A whole scene (sky, woods and ground) for SteamDrop's own pictures. */
  function picture(w, h, P, { ground = Math.round(h * 0.16) } = {}) {
    const g = cv(w, h);
    land(g, P, ground);
    return g.c;
  }

  return { palette, tree, backdrop, icon, picture, young, crisp, get drawn() { return drawn; } };
})();
