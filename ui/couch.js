"use strict";
// Couch mode: a full-screen, controller-first view of SteamDrop.
// Xbox and PlayStation pads through the Gamepad API (standard mapping), plus keyboard:
// arrows = D-pad, Enter/Space = A, Escape/Backspace = B, Q/E or PageUp/PageDown = LB/RB.
const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const TABS = ["library", "settings", "activity"];
const HOLD_MS = 2000;
let S = null;
let tab = "library";
let stack = [];          // screens opened on top of the tab, e.g. [{ name: "detail", id }]
let busy = false;

/* ---------- DOM helper (text is always set as text) ---------- */
function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v === false || v == null) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid);
  return el;
}
const $ = (s) => document.querySelector(s);
const art = (p, v) => p ? `${convertFileSrc(p)}?v=${v}` : null;
const game = (id) => S.games.find((g) => g.id === id);

/** A focusable button. `key` keeps focus on the same button after a re-render. */
function button(key, label, action, opts = {}) {
  return h("button", { class: `btn f ${opts.kind || ""}`, "data-key": key, "data-hold": opts.hold ? "1" : null, onclick: opts.hold ? null : action },
    opts.hold ? h("span", { class: "fill" }) : null,
    h("span", { class: "text" }, label, opts.note ? h("small", {}, ` ${opts.note}`) : null),
    opts.value != null ? h("span", { class: `value ${opts.on ? "on" : ""}` }, opts.value) : null);
}

let toastTimer = null;
function toast(text, error = false) {
  const t = $("#toast");
  t.textContent = text;
  t.classList.toggle("error", error);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, error ? 7000 : 4500);
}

/** Runs an app command, shows its message, refreshes. */
async function run(fn, after) {
  if (busy) return;
  busy = true;
  toast("Working on it...");
  try {
    const msg = await fn();
    toast(typeof msg === "string" && msg ? msg : "Done.");
    if (after) after();
  } catch (e) {
    toast(String(e), true);
  } finally {
    busy = false;
    await refresh();
  }
}

/* ---------- screens ---------- */
function libraryScreen() {
  if (!S.games.length) {
    return [h("div", { class: "empty" }, h("h1", {}, "No games yet"),
      h("p", { class: "muted" }, "Copy a game folder into one of SteamDrop's watched folders and it shows up here. You can choose watched folders in SteamDrop on your desktop."))];
  }
  return [h("div", { class: "grid" }, S.games.map((g) => {
    const img = art(g.portrait, g.added);
    return h("button", { class: `tile f ${img ? "" : "noart"}`, "data-key": `game-${g.id}`, onclick: () => push({ name: "detail", id: g.id }) },
      img ? h("img", { src: img, alt: "" }) : null,
      h("span", { class: "label" }, g.name));
  }))];
}

function detailScreen({ id }) {
  const g = game(id);
  if (!g) { stack.pop(); return current(); }
  const hero = h("div", { class: "hero" });
  if (g.hero) hero.style.backgroundImage = `url("${art(g.hero, g.added)}")`;
  const info = h("p", { class: "muted" }, g.folder_missing ? "The game file is missing." : g.in_steam ? "" : "Not in Steam right now.");
  invoke("get_info", { id }).then((i) => { if (i && i.description) info.textContent = i.description; }).catch(() => {});
  const deleteNote = S.couch_pin_set ? "(hold A, then PIN)" : "(hold A for 2 seconds)";
  return [hero, h("div", { class: "detail" },
    h("img", { class: "cover", src: art(g.portrait, g.added) || "", alt: "" }),
    h("div", {}, h("h1", {}, g.name), info,
      h("div", { class: "actions" },
        button("refresh", "Find artwork again", () => run(() => invoke("refresh_game", { id, steamAppid: null, restart: false }))),
        button("exe", "Change game file", () => push({ name: "exes", id })),
        button("remove", "Remove from Steam", () => push({ name: "confirm", title: `Remove "${g.name}" from Steam?`,
          text: "Only the Steam shortcut and its artwork are removed. The game's files stay where they are.",
          yes: "Remove from Steam", action: () => run(() => invoke("remove_game", { id, restart: false }), backToTab) })),
        button("delete", "Delete game", () => deleteGame(g), { kind: "danger", hold: true, note: deleteNote })),
      h("p", { class: "muted" }, "Changes to Steam's library show after Steam restarts.")))];
}

function deleteGame(g) {
  const go = (pin) => run(() => invoke("delete_game", { id: g.id, pin }), backToTab);
  if (S.couch_pin_set) push({ name: "pin", title: `Enter your PIN to delete "${g.name}"`, done: go });
  else go("");
}

function exesScreen({ id }) {
  const g = game(id);
  const list = h("div", { class: "actions" }, h("p", { class: "muted" }, "Looking for game files..."));
  invoke("exe_candidates", { folder: g.folder }).then((cands) => {
    const rel = (p) => p.toLowerCase().startsWith(g.folder.toLowerCase()) ? p.slice(g.folder.length).replace(/^[\\/]/, "") : p;
    list.replaceChildren(...(cands.length ? cands.slice(0, 8).map((c, i) => button(`exe-${i}`, rel(c.path),
      () => run(() => invoke("add_game", { folder: g.folder, exe: c.path, name: g.name, restart: false }), backToTab),
      { value: c.path.toLowerCase() === g.exe.toLowerCase() ? "Current" : null, on: true })) : [h("p", {}, "No game files found in this folder.")]));
    focusFirst();
  });
  return [h("h1", {}, "Change game file"), h("p", { class: "muted" }, `Which file starts "${g.name}"?`), list];
}

function confirmScreen(sc) {
  return [h("div", { class: "pin" }, h("h1", {}, sc.title), h("p", { class: "muted" }, sc.text),
    h("div", { class: "actions" }, button("yes", sc.yes, () => { stack.pop(); sc.action(); }, { kind: "danger" }), button("no", "Cancel", back)))];
}

/** Four-digit PIN pad: A types, B cancels. */
function pinScreen(sc) {
  sc.digits = sc.digits || "";
  const dots = h("div", { class: "dots" }, [0, 1, 2, 3].map((i) => h("i", { class: i < sc.digits.length ? "on" : "" })));
  const press = (d) => {
    if (d === "del") sc.digits = sc.digits.slice(0, -1);
    else if (sc.digits.length < 4) sc.digits += d;
    if (sc.digits.length === 4) { const pin = sc.digits; stack.pop(); sc.done(pin); }
    render();
  };
  const keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "del", "0", "back"];
  return [h("div", { class: "pin" }, h("h2", {}, sc.title), dots,
    h("div", { class: "pad" }, keys.map((k) => k === "back" ? button("k-back", "Cancel", back)
      : button(`k-${k}`, k === "del" ? "Delete" : k, () => press(k)))))];
}

function settingsScreen() {
  const st = S.settings;
  const onoff = (v) => v ? "On" : "Off";
  const toggle = (key, label, field) => button(key, label, () => save({ [field]: !st[field] }), { value: onoff(st[field]), on: st[field] });
  return [h("h1", {}, "Settings"), h("div", { class: "actions" },
    toggle("auto_add", "Add new games without asking", "auto_add"),
    toggle("auto_restart", "Restart Steam automatically", "auto_restart"),
    toggle("notify", "Show notifications", "notify"),
    toggle("art_folder", "Also save art in the game folder", "art_in_game_folder"),
    button("shortcut", "SteamDrop in the Steam library", () => run(() => invoke("set_steam_shortcut", { on: !st.steam_shortcut })),
      { value: onoff(st.steam_shortcut), on: st.steam_shortcut }),
    button("pin", "PIN needed to delete games", () => S.couch_pin_set
      ? push({ name: "pin", title: "Enter your current PIN to turn it off", done: (old) => run(() => invoke("set_couch_pin", { old, new: "" })) })
      : push({ name: "pin", title: "Choose a 4-digit PIN", done: (pin) => run(() => invoke("set_couch_pin", { old: "", new: pin })) }),
      { value: onoff(S.couch_pin_set), on: S.couch_pin_set }),
    button("exit", "Leave Couch mode", () => invoke("close_couch"))),
    h("p", { class: "muted" }, `SteamDrop ${S.version}. More settings are in SteamDrop on your desktop.`)];
}

function activityScreen() {
  const list = h("div", { class: "list" });
  invoke("get_activity").then((lines) => {
    const rows = lines.slice(-100).reverse().map((line, i) => {
      const text = line.slice(21);
      return h("div", { class: `row f ${text.startsWith("Problem:") ? "problem" : ""}`, tabindex: "-1", "data-key": `log-${i}` },
        h("time", {}, line.slice(0, 16)), h("span", {}, text));
    });
    list.replaceChildren(...(rows.length ? rows : [h("p", { class: "muted" }, "Nothing has happened yet.")]));
    focusFirst();
  });
  return [h("h1", {}, "Activity"), list];
}

const SCREENS = { library: libraryScreen, settings: settingsScreen, activity: activityScreen, detail: detailScreen, exes: exesScreen, confirm: confirmScreen, pin: pinScreen };
const current = () => stack.length ? SCREENS[stack.at(-1).name](stack.at(-1)) : SCREENS[tab]();

/* ---------- navigation ---------- */
function render() {
  const keep = document.activeElement?.dataset?.key;
  for (const t of document.querySelectorAll(".tab")) t.classList.toggle("active", t.dataset.tab === tab);
  $("#screen").replaceChildren(...current());
  const hints = [h("span", {}, h("b", { class: "a" }, "A"), "Select"), h("span", {}, h("b", { class: "b" }, "B"), stack.length ? "Back" : "Leave")];
  if (!stack.length) hints.push(h("span", {}, h("b", {}, "LB"), h("b", {}, "RB"), "Switch"));
  $("#hints").replaceChildren(...hints);
  const again = keep && document.querySelector(`.f[data-key="${CSS.escape(keep)}"]`);
  if (again) again.focus({ preventScroll: true }); else focusFirst();
}

function focusFirst() {
  if (document.activeElement?.classList.contains("f") && document.body.contains(document.activeElement)) return;
  document.querySelector("#screen .f")?.focus();
}

function push(sc) { stack.push(sc); render(); }
function back() {
  if (stack.length) { stack.pop(); render(); }
  else push({ name: "confirm", title: "Leave Couch mode?", text: "SteamDrop keeps running in the tray on your desktop.", yes: "Leave", action: () => invoke("close_couch") });
}
function backToTab() { stack = []; render(); }
function switchTab(step) {
  tab = TABS[(TABS.indexOf(tab) + step + TABS.length) % TABS.length];
  stack = [];
  render();
}

async function save(patch) {
  try { await invoke("save_settings", { settings: { ...S.settings, ...patch } }); } catch (e) { toast(String(e), true); }
  await refresh();
}

/** Moves focus to the nearest focusable element in a direction. */
function move(dx, dy) {
  const items = [...document.querySelectorAll("#screen .f")];
  const cur = document.activeElement;
  if (!items.includes(cur)) { items[0]?.focus(); return; }
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

/* Hold A on a "hold" button (Delete game) for two seconds to confirm. */
let hold = null;
function holdStart() {
  const el = document.activeElement;
  if (!el?.dataset.hold || hold) return false;
  const fill = el.querySelector(".fill");
  const start = performance.now();
  hold = { el, raf: 0 };
  const tick = (t) => {
    const p = Math.min(1, (t - start) / HOLD_MS);
    fill.style.width = `${p * 100}%`;
    if (p >= 1) { const target = el; holdEnd(); target.dispatchEvent(new CustomEvent("held", { bubbles: true })); return; }
    hold.raf = requestAnimationFrame(tick);
  };
  hold.raf = requestAnimationFrame(tick);
  return true;
}
function holdEnd() {
  if (!hold) return;
  cancelAnimationFrame(hold.raf);
  const fill = hold.el.querySelector(".fill");
  if (fill) fill.style.width = "0";
  hold = null;
}

function press(action) {
  if (action === "a") {
    if (!holdStart()) document.activeElement?.classList.contains("f") && document.activeElement.click();
  } else if (action === "b") back();
  else if (action === "lb") switchTab(-1);
  else if (action === "rb") switchTab(1);
  else if (action === "up") move(0, -1);
  else if (action === "down") move(0, 1);
  else if (action === "left") move(-1, 0);
  else if (action === "right") move(1, 0);
}
function release(action) { if (action === "a") holdEnd(); }

/* Keyboard */
const KEYS = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Enter: "a", " ": "a", Escape: "b", Backspace: "b", q: "lb", Q: "lb", PageUp: "lb", e: "rb", E: "rb", PageDown: "rb" };
document.addEventListener("keydown", (e) => {
  const action = KEYS[e.key];
  if (!action) return;
  e.preventDefault();
  if (e.repeat && (action === "a" || action === "b" || action === "lb" || action === "rb")) return;
  press(action);
});
document.addEventListener("keyup", (e) => { if (KEYS[e.key]) { e.preventDefault(); release(KEYS[e.key]); } });
document.addEventListener("click", (e) => { if (e.target.closest?.("[data-hold]")) e.preventDefault(); }, true);

/* Gamepad (standard mapping: 0 A, 1 B, 4 LB, 5 RB, 12-15 D-pad; axes 0/1 left stick).
   The Gamepad API has no events for buttons, so it's read every frame while Couch mode is open. */
const BUTTONS = { 0: "a", 1: "b", 4: "lb", 5: "rb", 12: "up", 13: "down", 14: "left", 15: "right" };
const REPEATS = new Set(["up", "down", "left", "right"]);
const held = new Map(); // action -> next repeat time
function pollPads(t) {
  const down = new Set();
  for (const pad of navigator.getGamepads?.() || []) {
    if (!pad) continue;
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
  requestAnimationFrame(pollPads);
}

/* ---------- start ---------- */
async function refresh() {
  S = await invoke("get_state");
  render();
}
// "Delete game" fires "held" on the button; route it to that button's action.
document.addEventListener("held", (e) => {
  const sc = stack.at(-1);
  if (e.target.dataset.key === "delete" && sc?.name === "detail") deleteGame(game(sc.id));
});
listen("changed", refresh);
refresh().then(() => requestAnimationFrame(pollPads));
