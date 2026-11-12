"use strict";
// Shared by the desktop window and Couch mode: controller input, moving focus with the D-pad,
// and game art with a title-card fallback.
window.SD = (() => {
  const { convertFileSrc } = window.__TAURI__.core;

  const { invoke } = window.__TAURI__.core;

  /* Gamepad (standard mapping, so Xbox and PlayStation pads both work: 0 A, 1 B, 2 X, 3 Y, 4 LB, 5 RB,
     6 LT, 7 RT, 8 View/Share, 9 Menu/Options, 12-15 D-pad; axes 0/1 left stick). The Gamepad API has
     no button events, so pads are read every frame, and only while one is connected. */
  const BUTTONS = { 0: "a", 1: "b", 2: "x", 3: "y", 4: "lb", 5: "rb", 6: "lt", 7: "rt", 8: "view", 9: "menu", 12: "up", 13: "down", 14: "left", 15: "right" };
  const REPEATS = new Set(["up", "down", "left", "right"]);

  function startPad(press, release) {
    const held = new Map(); // action -> next repeat time
    let running = false;
    const poll = (t) => {
      const pads = [...(navigator.getGamepads?.() || [])].filter(Boolean);
      const down = new Set();
      for (const pad of pads) {
        for (const [i, action] of Object.entries(BUTTONS)) if (pad.buttons[i]?.pressed) down.add(action);
        const [x = 0, y = 0] = pad.axes;
        if (y < -0.55) down.add("up"); if (y > 0.55) down.add("down");
        if (x < -0.55) down.add("left"); if (x > 0.55) down.add("right");
      }
      for (const action of down) {
        if (!held.has(action)) { press(action); held.set(action, t + 380); }
        else if (REPEATS.has(action) && t >= held.get(action)) { press(action); held.set(action, t + 120); }
      }
      for (const action of [...held.keys()]) if (!down.has(action)) { held.delete(action); release(action); }
      running = pads.length > 0;
      if (running) requestAnimationFrame(poll);
    };
    const start = () => { if (!running) { running = true; requestAnimationFrame(poll); } };
    window.addEventListener("gamepadconnected", start);
    start();
  }

  /** Moves focus to the nearest of `items` in a direction (dx, dy are -1, 0 or 1). */
  function move(dx, dy, items) {
    const cur = document.activeElement;
    if (!items.includes(cur)) { items[0]?.focus(); items[0]?.scrollIntoView({ block: "nearest" }); return; }
    const a = cur.getBoundingClientRect();
    const ax = a.left + a.width / 2, ay = a.top + a.height / 2;
    let best = null, bestScore = Infinity;
    for (const el of items) {
      if (el === cur) continue;
      const b = el.getBoundingClientRect();
      const ddx = b.left + b.width / 2 - ax, ddy = b.top + b.height / 2 - ay;
      const along = dx ? ddx * dx : ddy * dy;           // distance in the wanted direction
      const across = Math.abs(dx ? ddy : ddx);          // drift to the side
      if (along <= 4) continue;
      const score = along + across * 2.5;
      if (score < bestScore) { bestScore = score; best = el; }
    }
    if (best) { best.focus({ preventScroll: true }); best.scrollIntoView({ block: "nearest", inline: "nearest" }); }
  }

  /** A game's art (slot "portrait" or "hero"), or a plain title card if it's missing or won't load. */
  function art(g, slot, className = "") {
    const card = () => { const d = document.createElement("div"); d.className = `titlecard ${className}`; d.textContent = g.name; return d; };
    if (!g[slot]) return card();
    const img = document.createElement("img");
    img.className = className;
    img.alt = "";
    img.src = `${convertFileSrc(`${g.id}/${slot}`, "art")}?v=${g.added}`;
    img.addEventListener("error", () => img.replaceWith(card()), { once: true });
    return img;
  }

  /* "Interface size": Ctrl+plus, Ctrl+minus, Ctrl+0 and Ctrl+wheel (LT/RT on a controller call step),
     50 to 200 percent in steps of 10, saved per view in the setting `field`. */
  function zoom(field, save) {
    let level = null;
    const set = (l) => {
      l = Math.max(50, Math.min(200, Math.round(l / 10) * 10));
      if (l === level) return level;
      level = l;
      invoke("set_zoom", { level }).catch(() => {});
      return level;
    };
    const change = (l) => { const before = level; if (set(l) !== before) save({ [field]: level }); };
    document.addEventListener("keydown", (e) => {
      if (!e.ctrlKey || e.altKey) return;
      if (e.key === "+" || e.key === "=" || e.code === "NumpadAdd") change(level + 10);
      else if (e.key === "-" || e.code === "NumpadSubtract") change(level - 10);
      else if (e.key === "0" || e.code === "Numpad0") change(100);
      else return;
      e.preventDefault();
    });
    document.addEventListener("wheel", (e) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      change(level + (e.deltaY < 0 ? 10 : -10));
    }, { passive: false });
    return {
      /** Call with the saved level after each refresh (applies it the first time and after changes elsewhere). */
      sync: (saved) => set(saved || 100),
      step: (d) => change(level + d * 10),
      reset: () => change(100),
      get level() { return level; },
    };
  }

  /* Color themes (the colors themselves are in themes.css). */
  const THEMES = [["dark", "SteamDrop Dark"], ["light", "SteamDrop Light"], ["dracula", "Dracula"], ["nord", "Nord"],
    ["catppuccin", "Catppuccin Mocha"], ["gruvbox", "Gruvbox Dark"], ["tokyonight", "Tokyo Night"], ["terminal", "Terminal"]];
  const themeId = (id) => THEMES.some((t) => t[0] === id) ? id : "dark";
  const themeName = (id) => THEMES.find((t) => t[0] === themeId(id))[1];
  const applyTheme = (id) => { document.documentElement.dataset.theme = themeId(id); };

  /* "Couch setup": everything a controller-only player needs, turned on in one go. */
  const COUCH_SETUP = [
    ["Automatic mode: new games are added, Steam restarts and you're told, all by itself", { auto_add: true, auto_restart: true, notify: true }],
    ["Keep Big Picture in front of other apps", { keep_big_picture: true }],
    ["Steam always comes back in Big Picture after SteamDrop restarts it", { steam_restart_mode: "bigpicture" }],
    ["Steam opens in Big Picture when Windows starts", { big_picture_at_sign_in: true }],
    ["SteamDrop starts with Windows", { run_on_startup: true }],
    ["Updates install by themselves (never while you're playing)", { update_auto_install: true }],
  ];
  /** The settings change for Couch setup, remembering the old values so it can be undone. */
  function couchSetup(settings) {
    const patch = Object.assign({}, ...COUCH_SETUP.map((s) => s[1]));
    const before = Object.fromEntries(Object.keys(patch).map((k) => [k, settings[k]]));
    return { ...patch, couch_setup_undo: settings.couch_setup_undo || before };
  }
  const couchSetupUndo = (settings) => ({ ...settings.couch_setup_undo, couch_setup_undo: null });

  /** Replaces an element's content; leaves out null and false (replaceChildren would show them as text). */
  const fill = (el, ...kids) => el.replaceChildren(...kids.flat().filter((k) => k != null && k !== false));

  return { startPad, move, art, zoom, THEMES, themeId, themeName, applyTheme, fill, COUCH_SETUP, couchSetup, couchSetupUndo };
})();
