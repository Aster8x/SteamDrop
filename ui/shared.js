"use strict";
// Shared by the desktop window and Couch mode: controller input, moving focus with the D-pad,
// and game art with a title-card fallback.
window.SD = (() => {
  const { convertFileSrc } = window.__TAURI__.core;

  /* Gamepad (standard mapping, so Xbox and PlayStation pads both work: 0 A, 1 B, 3 Y, 4 LB, 5 RB,
     8 View/Share, 9 Menu/Options, 12-15 D-pad; axes 0/1 left stick). The Gamepad API has no button
     events, so pads are read every frame, and only while one is connected. */
  const BUTTONS = { 0: "a", 1: "b", 3: "y", 4: "lb", 5: "rb", 8: "view", 9: "menu", 12: "up", 13: "down", 14: "left", 15: "right" };
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

  return { startPad, move, art };
})();
