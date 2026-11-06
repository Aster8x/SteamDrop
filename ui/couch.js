"use strict";
// Couch mode: a full-screen, controller-first view of SteamDrop.
// Xbox and PlayStation pads through the Gamepad API (standard mapping), plus keyboard:
// arrows = D-pad, Enter/Space = A, Escape/Backspace = B, Q/E or PageUp/PageDown = LB/RB,
// Ctrl+plus/minus/0 or Ctrl+wheel (LT/RT on a pad) = interface size.
const { invoke } = window.__TAURI__.core;
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
  return [selected.size ? selectionBar() : null, h("div", { class: "grid" }, S.games.map((g) =>
    h("button", { class: `tile f ${selected.has(g.id) ? "selected" : ""}`, "data-key": `game-${g.id}`, "aria-label": g.name, "aria-pressed": String(selected.has(g.id)), onclick: () => push({ name: "detail", id: g.id }) },
      SD.art(g, "portrait"),
      selected.has(g.id) ? h("span", { class: "check", "aria-hidden": "true" }, "✓") : null,
      h("span", { class: "label" }, g.name))))];
}

/* ---------- several games at once: X selects the focused game, holding X selects all ---------- */
const selected = new Set();
const nGames = (n) => `${n} game${n === 1 ? "" : "s"}`;
function clearSelection() { selected.clear(); stack = []; render(); }
function selectionBar() {
  const ids = [...selected];
  const n = ids.length;
  return h("div", { class: "selbar" }, h("h2", {}, `${n} selected`),
    button("sel-art", "Re-fetch art", () => run(() => invoke("refetch_art", { ids, restart: false }), clearSelection)),
    button("sel-exe", "Re-check game file", () => run(() => invoke("recheck_games", { ids, restart: false }), clearSelection)),
    button("sel-remove", "Remove from Steam", () => push({ name: "confirm", title: `Remove ${nGames(n)} from Steam?`,
      text: `${names(ids)}. Only the Steam shortcuts and their artwork are removed; the game files stay where they are.`,
      yes: "Remove from Steam", action: () => run(() => invoke("remove_games", { ids, restart: false }), clearSelection) })),
    button("sel-delete", n === 1 ? "Delete game" : "Delete games", () => push({ name: "bulkdelete", ids }), { kind: "danger" }),
    button("sel-clear", "Clear", clearSelection));
}
const names = (ids) => ids.map((id) => game(id)?.name).filter(Boolean).join(", ");

/** Deleting several games: lists them, then hold A for 2 seconds (and the PIN, if one is set). */
function bulkDeleteScreen(sc) {
  const note = S.couch_pin_set ? "(hold A, then PIN)" : "(hold A for 2 seconds)";
  return [h("div", { class: "pin wide" }, h("h1", {}, `Delete ${nGames(sc.ids.length)}?`),
    h("p", { class: "muted" }, "These folders go to the Recycle Bin and the games are removed from Steam. Folders on drives without a Recycle Bin are not deleted."),
    h("ul", { class: "names" }, sc.ids.map((id) => game(id)).filter(Boolean).map((g) => h("li", {}, g.name))),
    h("div", { class: "actions" },
      button("bulk-delete", `Delete ${nGames(sc.ids.length)}`, null, { kind: "danger", hold: true, note }),
      button("bulk-cancel", "Cancel", back)))];
}
function bulkDelete(ids) {
  const go = (pin) => run(() => invoke("delete_games", { ids, pin, restart: false }), clearSelection);
  stack.pop();
  if (S.couch_pin_set) push({ name: "pin", title: `Enter your PIN to delete ${nGames(ids.length)}`, done: go });
  else go("");
}
let xHold = null;
function xDown() {
  if (stack.length || tab !== "library") return;
  xHold = setTimeout(() => { xHold = null; S.games.forEach((g) => selected.add(g.id)); toast(`All ${nGames(S.games.length)} selected.`); render(); }, 1000);
}
function xUp() {
  if (!xHold) return;
  clearTimeout(xHold);
  xHold = null;
  const key = document.activeElement?.dataset?.key || "";
  if (!key.startsWith("game-")) return;
  const id = Number(key.slice(5));
  if (selected.has(id)) selected.delete(id); else selected.add(id);
  render();
}

function detailScreen({ id }) {
  const g = game(id);
  if (!g) { stack.pop(); return current(); }
  const hero = h("div", { class: "hero" }, g.hero ? SD.art(g, "hero") : null);
  const info = h("p", { class: "muted" }, g.folder_missing ? "The game file is missing." : g.in_steam ? "" : "Not in Steam right now.");
  invoke("get_info", { id }).then((i) => { if (i && i.description) info.textContent = i.description; }).catch(() => {});
  const deleteNote = S.couch_pin_set ? "(hold A, then PIN)" : "(hold A for 2 seconds)";
  return [hero, h("div", { class: "detail" },
    SD.art(g, "portrait", "cover"),
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
  const go = (pin) => run(() => invoke("delete_games", { ids: [g.id], pin, restart: false }), backToTab);
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
    toggle("keep_bp", "Keep Big Picture in front", "keep_big_picture"),
    button("shortcut", "Show SteamDrop in my Steam library", () => run(() => invoke("set_steam_shortcut", { on: !st.steam_shortcut })),
      { value: onoff(st.steam_shortcut), on: st.steam_shortcut }),
    button("pin", "PIN needed to delete games", () => S.couch_pin_set
      ? push({ name: "pin", title: "Enter your current PIN to turn it off", done: (old) => run(() => invoke("set_couch_pin", { old, new: "" })) })
      : push({ name: "pin", title: "Choose a 4-digit PIN", done: (pin) => run(() => invoke("set_couch_pin", { old: "", new: pin })) }),
      { value: onoff(S.couch_pin_set), on: S.couch_pin_set }),
    button("theme", "Theme", () => { push({ name: "themes" }); document.querySelector(`[data-key="theme-${SD.themeId(st.theme)}"]`)?.focus(); }, { value: SD.themeName(st.theme), on: true }),
    button("zoom", "Interface size", () => Z.reset(), { value: `${st.zoom_couch}%`, on: st.zoom_couch !== 100, note: "(LT smaller, RT bigger, A resets)" }),
    button("exit", "Leave Couch mode", leave)),
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

/** Themes: moving onto one previews it on the whole screen, A keeps it, B goes back to the saved one. */
function themesScreen() {
  const saved = SD.themeId(S.settings.theme);
  return [h("h1", {}, "Theme"), h("p", { class: "muted" }, "Move to a theme to try it. A keeps it, B goes back."),
    h("div", { class: "actions" }, SD.THEMES.map(([id, name]) =>
      h("button", { class: "btn f", "data-key": `theme-${id}`, "data-theme": id, onfocus: () => SD.applyTheme(id),
        onclick: () => { stack.pop(); save({ theme: id }); } },
        h("span", { class: "text" }, name), h("span", { class: "value on" }, id === saved ? "In use" : ""))))];
}

/** B on the Library: leave for Steam (the default), quit, or stay. */
function exitScreen() {
  return [h("div", { class: "pin" }, h("h1", {}, "Leave SteamDrop?"),
    h("p", { class: "muted" }, "Back to Steam keeps SteamDrop running, so it still adds new games."),
    h("div", { class: "actions" },
      button("exit-steam", "Back to Steam", () => { stack.pop(); backToSteam(); render(); }),
      button("exit-quit", "Quit SteamDrop", () => { stack.pop(); quitScreen(); }, { kind: "danger" }),
      button("exit-cancel", "Cancel", back)))];
}

/** One question about several games, each with its own checkbox: A selects or unselects. */
function checklistScreen(sc) {
  const picked = sc.items.filter((i) => i.on).map((i) => i.key);
  return [h("div", { class: "pin wide" }, h("h1", {}, sc.title), h("p", { class: "muted" }, sc.text),
    h("div", { class: "actions" },
      sc.items.map((i, n) => button(`item-${n}`, i.label, () => { i.on = !i.on; render(); }, { value: i.on ? "✓ Selected" : "Not selected", on: i.on, note: i.note })),
      button("list-yes", `${sc.yes} (${picked.length})`, () => { if (!picked.length) return toast("Select at least one game.", true); stack.pop(); sc.action(picked); }),
      button("list-no", sc.no, () => { stack.pop(); sc.cancel?.(); render(); })))];
}

const SCREENS = { library: libraryScreen, settings: settingsScreen, activity: activityScreen, detail: detailScreen, exes: exesScreen, confirm: confirmScreen, pin: pinScreen, leave: leaveScreen, exit: exitScreen, checklist: checklistScreen, themes: themesScreen, bulkdelete: bulkDeleteScreen };
const current = () => stack.length ? SCREENS[stack.at(-1).name](stack.at(-1)) : SCREENS[tab]();

/* ---------- control bar ---------- */
async function scanNow() {
  toast("Looking for new games...");
  try { toast((await invoke("scan_now")) || "Done."); } catch (e) { toast(String(e), true); }
}
const switchView = () => invoke("switch_view", { from: "couch" });
// Closes Couch mode (SteamDrop stays in the tray), so Steam shows Play again and Play reopens it.
// With changes waiting for a Steam restart, it asks first, like leaving Couch mode.
const backToSteam = () => S.restart_needed ? push({ name: "leave" }) : invoke("close_couch", { restart: false });
const quitScreen = () => push({ name: "confirm", title: "Stop adding new games?", text: "SteamDrop will stop watching your folders until you start it again.", yes: "Quit SteamDrop", action: () => invoke("quit") });

function controlBar() {
  const c = (key, badge, cls, label, action, kind = "") =>
    h("button", { class: `cbtn f ${kind}`, "data-key": key, onclick: action }, h("b", { class: cls }, badge), label);
  return [
    c("c-scan", "Y", "y", "Scan now", scanNow),
    c("c-view", "View", "", "Window view", switchView),
    c("c-hide", "Menu", "", "Back to Steam", backToSteam),
  ];
}

/* ---------- navigation ---------- */
function render() {
  const keep = document.activeElement?.dataset?.key;
  // The saved theme, except while trying themes on the Theme screen.
  if (stack.at(-1)?.name !== "themes") SD.applyTheme(S.settings.theme);
  for (const t of document.querySelectorAll(".tab")) t.classList.toggle("active", t.dataset.tab === tab);
  SD.fill($("#screen"), current());
  $("#controls").replaceChildren(...controlBar());
  const hints = [h("span", {}, h("b", { class: "a" }, "A"), "Select"), h("span", {}, h("b", { class: "b" }, "B"), stack.length ? "Back" : tab !== "library" ? "Library" : selected.size ? "Clear selection" : "Leave")];
  if (!stack.length && tab === "library" && S.games.length) hints.push(h("span", {}, h("b", { class: "x" }, "X"), "Select game (hold: all)"));
  if (!stack.length) hints.push(h("span", {}, h("b", {}, "LB"), h("b", {}, "RB"), "Switch"));
  hints.push(h("span", {}, h("b", {}, "LT"), h("b", {}, "RT"), "Size"));
  $("#hints").replaceChildren(...hints);
  const again = keep && document.querySelector(`.f[data-key="${CSS.escape(keep)}"]`);
  if (again) again.focus({ preventScroll: true }); else focusFirst();
}

function focusFirst() {
  if (document.activeElement?.classList.contains("f") && document.body.contains(document.activeElement)) return;
  document.querySelector("#screen .f")?.focus();
}

function push(sc) { stack.push(sc); render(); }
/** B: close the screen on top, go back to the Library, or (on the Library) ask how to leave. */
function back() {
  if (stack.length) { stack.pop(); render(); }
  else if (tab !== "library") { tab = "library"; render(); }
  else if (selected.size) clearSelection();
  else push({ name: "exit" });
}

/** Leaving: if changes are waiting for a Steam restart, offer to restart Steam once Couch mode has closed. */
function leave() {
  if (S.restart_needed) push({ name: "leave" });
  else push({ name: "confirm", title: "Leave Couch mode?", text: "SteamDrop keeps running in the tray on your desktop.", yes: "Leave", action: () => invoke("close_couch", { restart: false }) });
}

function leaveScreen() {
  return [h("div", { class: "pin" }, h("h1", {}, "Restart Steam to apply changes?"),
    h("p", { class: "muted" }, "Steam shows the changes you made after it restarts. SteamDrop can restart it right after Couch mode closes, and bring you back to Big Picture."),
    h("div", { class: "actions" },
      button("restart", "Restart now", () => invoke("close_couch", { restart: true })),
      button("later", "Later", () => invoke("close_couch", { restart: false })),
      button("cancel", "Cancel", back)))];
}
function backToTab() { stack = []; render(); }
function switchTab(step) {
  tab = TABS[(TABS.indexOf(tab) + step + TABS.length) % TABS.length];
  stack = [];
  render();
}

const Z = SD.zoom("zoom_couch", (patch) => save(patch));

async function save(patch) {
  try { await invoke("save_settings", { patch }); } catch (e) { toast(String(e), true); }
  await refresh();
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
  else if (action === "y") scanNow();
  else if (action === "view") switchView();
  else if (action === "menu") backToSteam();
  else if (action === "lb") switchTab(-1);
  else if (action === "rb") switchTab(1);
  else if (action === "x") xDown();
  else if (action === "lt") Z.step(-1);
  else if (action === "rt") Z.step(1);
  else if (action === "up") move(0, -1);
  else if (action === "down") move(0, 1);
  else if (action === "left") move(-1, 0);
  else if (action === "right") move(1, 0);
}
function release(action) { if (action === "a") holdEnd(); else if (action === "x") xUp(); }
const move = (dx, dy) => SD.move(dx, dy, [...document.querySelectorAll("#screen .f, #controls .f")]);

/* Keyboard */
const KEYS = { x: "x", X: "x", F11: "view", ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Enter: "a", " ": "a", Escape: "b", Backspace: "b", q: "lb", Q: "lb", PageUp: "lb", e: "rb", E: "rb", PageDown: "rb" };
document.addEventListener("keydown", (e) => {
  const action = KEYS[e.key];
  if (!action) return;
  e.preventDefault();
  if (e.repeat && !["up", "down", "left", "right"].includes(action)) return;
  press(action);
});
document.addEventListener("keyup", (e) => { if (KEYS[e.key]) { e.preventDefault(); release(KEYS[e.key]); } });
document.addEventListener("click", (e) => { if (e.target.closest?.("[data-hold]")) e.preventDefault(); }, true);


/* ---------- start ---------- */
async function refresh() {
  S = await invoke("get_state");
  Z.sync(S.settings.zoom_couch);
  render();
  maybeAsk();
}

/* New games and deleted game folders that arrive together are asked about together. */
const askedFound = new Set();
const askedGone = new Set();
function maybeAsk() {
  if (stack.some((sc) => sc.name === "checklist")) return;
  const gone = S.gone.map(game).filter((g) => g && !askedGone.has(g.id));
  if (gone.length) {
    gone.forEach((g) => askedGone.add(g.id));
    const n = gone.length;
    push({ name: "checklist", title: n === 1 ? `The folder of "${gone[0].name}" was deleted. Remove it from Steam?` : `${n} game folders were deleted. Remove them from Steam?`,
      text: "They were deleted or moved out of your watched folders. Only the Steam shortcuts and artwork are removed.",
      items: gone.map((g) => ({ key: g.id, label: g.name, on: true })), yes: "Remove selected", no: "Keep in Steam",
      action: (ids) => {
        const kept = gone.map((g) => g.id).filter((id) => !ids.includes(id));
        if (kept.length) invoke("keep_gone", { ids: kept });
        run(() => invoke("remove_games", { ids, restart: false }), backToTab);
      },
      cancel: () => invoke("keep_gone", { ids: gone.map((g) => g.id) }) });
    return;
  }
  const found = S.found.filter((f) => !f.installer_only && f.candidates.length && !askedFound.has(f.folder));
  if (!found.length) return;
  found.forEach((f) => askedFound.add(f.folder));
  const n = found.length;
  push({ name: "checklist", title: n === 1 ? `New game found: "${found[0].title}". Add it to Steam?` : `${n} new games found. Add them to Steam?`,
    text: "SteamDrop picked the file that starts each game. You can change it later in the game's details.",
    items: found.map((f) => ({ key: f.folder, label: f.title, on: true })), yes: "Add selected", no: "Not now",
    action: (folders) => run(() => invoke("add_games", { restart: false,
      games: found.filter((f) => folders.includes(f.folder)).map((f) => ({ folder: f.folder, exe: f.candidates[0].path, name: f.title })) }), backToTab) });
}
// "Delete game" fires "held" on the button; route it to that button's action.
document.addEventListener("held", (e) => {
  const sc = stack.at(-1);
  if (e.target.dataset.key === "delete" && sc?.name === "detail") deleteGame(game(sc.id));
  if (e.target.dataset.key === "bulk-delete" && sc?.name === "bulkdelete") bulkDelete(sc.ids);
});
listen("changed", refresh);
// Games added while this view was hidden or closed show up as soon as it's back.
window.addEventListener("focus", refresh);
document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
refresh().then(() => SD.startPad(press, release));
