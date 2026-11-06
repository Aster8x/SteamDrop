"use strict";
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const REPO = "https://github.com/Aster8x/SteamDrop";
const LEGAL = "SteamDrop is an independent, open source tool for adding games stored on your own PC to your Steam library. It does not provide, host, or link to any games, and does not encourage or enable illegal access to copyrighted material. Only use it with games you have the right to use. SteamDrop is not affiliated with any cracking scene groups, nor with Valve Corporation or any game publisher. Steam is a trademark of Valve Corporation. All artwork and game information belong to their respective owners and are fetched at runtime from public sources. Provided as-is, with no warranty of any kind; use at your own risk.";

let S = null;          // last state from the app
let view = "games";
let busy = false;
let checkMsg = null;   // { text, ok } after "Check now"
let dialogFor = null;  // which automatic dialog is open: "whats-new" or "update-<version>"
let sgdbKey = null;    // what's in the key box (loaded from Windows Credential Manager once)
let keyMsg = null;     // { text, ok } after "Test key" or "Save key"
const SGDB_API_PAGE = "https://www.steamgriddb.com/profile/preferences/api";

/* ---------- tiny DOM helper (text is always set as text, never as HTML) ---------- */
function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v === false || v == null) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else if (k in el && typeof v !== "string") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid);
  return el;
}
const $ = (sel) => document.querySelector(sel);
const btn = (label, onclick, kind = "", extra = {}) =>
  h("button", { class: `btn ${kind}`, onclick, disabled: busy && !extra.always, ...extra }, label);
const logo = () => $(".brand svg").cloneNode(true);

/* ---------- state and status ---------- */
async function refresh() {
  S = await invoke("get_state");
  Z.sync(S.settings.zoom_window);
  SD.applyTheme(S.settings.theme);
  render();
  maybeShowDialogs();
}

/* ---------- updates ---------- */
/** Inline Markdown from the changelog: **bold**, `code`, and [links](...) shown as plain text. */
function inline(text) {
  return text.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").split(/(\*\*[^*]+\*\*|`[^`]+`)/).map((p) =>
    p.length > 4 && p.startsWith("**") && p.endsWith("**") ? h("strong", {}, p.slice(2, -2))
      : p.length > 2 && p.startsWith("`") && p.endsWith("`") ? h("code", {}, p.slice(1, -1)) : p);
}

/** Changelog Markdown (### headings, - bullets, paragraphs) as readable elements. */
function markdown(md) {
  const out = [];
  let list = null;
  for (const raw of md.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) { list = null; continue; }
    if (line.startsWith("### ")) { list = null; out.push(h("h4", {}, inline(line.slice(4)))); }
    else if (/^[-*] /.test(line)) {
      if (!list) { list = h("ul", {}); out.push(list); }
      list.append(h("li", {}, ...inline(line.slice(2))));
    } else { list = null; out.push(h("p", {}, ...inline(line))); }
  }
  return out;
}

function notesView(sections) {
  return h("div", { class: "notes" }, sections.map((sec) => h("section", {},
    h("h3", {}, `Version ${sec.version}`, sec.date ? h("span", { class: "muted small" }, ` · ${sec.date}`) : null),
    markdown(sec.body || "No details were given for this version."))));
}

async function showUpdate(u) {
  dialogFor = `update-${u.version}`;
  const choice = await ask("An update is ready", [
    h("p", { class: "update-versions" }, `Current: ${u.current}`, h("span", { "aria-hidden": "true" }, "  →  "), h("strong", {}, `New: ${u.version}`)),
    notesView(u.notes),
  ], [
    { label: "Skip this version", value: "skip" },
    { label: "Remind me later", value: "later" },
    { label: "Update now", value: "now", kind: "primary" },
  ]);
  dialogFor = null;
  if (choice === "now") {
    busy = true;
    showStatus("Downloading the update. SteamDrop will close and open again by itself.");
    try { await invoke("install_update"); } catch (e) { busy = false; showStatus(String(e), true); }
  } else {
    await invoke("dismiss_update", { skip: choice === "skip" });
  }
}

async function showWhatsNew(w) {
  dialogFor = "whats-new";
  await ask(`What's new in ${w.version}`, [h("div", { class: "notes" }, markdown(w.body))], [{ label: "Got it", value: "ok", kind: "primary" }]);
  dialogFor = null;
  await invoke("ack_whats_new");
}

/** Opens "What's new" (after an update) or the update window, but never over another dialog. */
function maybeShowDialogs() {
  if (dialogFor || $("#dialog").open) return;
  if (S.whats_new) showWhatsNew(S.whats_new);
  else if (S.update) showUpdate(S.update);
  else if (S.gone.some((id) => !askedGone.has(id))) showGone();
  else if (addable().length > 1 && addable().some((f) => !askedFound.has(f.folder))) showFoundBatch();
}

/* ---------- batches: new games and deleted game folders that arrive together ---------- */
const askedFound = new Set();  // folders already shown in a "new games found" question
const askedGone = new Set();   // games already shown in a "folders were deleted" question
const addable = () => S.found.filter((f) => !f.installer_only && f.candidates.length);

/** A list of ticked checkboxes; `selected()` gives the keys still ticked. */
function checklist(items, label) {
  const el = h("div", { class: "exe-list", role: "group", "aria-label": label },
    items.map((i) => h("label", { class: "exe-option" },
      h("input", { type: "checkbox", checked: true, value: String(i.key) }),
      h("span", {}, h("strong", {}, i.name), h("br", {}), h("span", { class: "path muted small" }, i.note)))));
  return { el, selected: () => [...el.querySelectorAll("input:checked")].map((c) => c.value) };
}

async function showGone() {
  dialogFor = "gone";
  const games = S.gone.map((id) => S.games.find((g) => g.id === id)).filter(Boolean);
  games.forEach((g) => askedGone.add(g.id));
  const n = games.length;
  const list = checklist(games.map((g) => ({ key: g.id, name: g.name, note: g.folder })), "Games whose folder was deleted");
  const choice = await ask(n === 1 ? `The folder of "${games[0].name}" was deleted. Remove it from Steam?` : `${n} game folders were deleted. Remove them from Steam?`,
    [h("p", {}, "They were deleted or moved out of your watched folders. Only the Steam shortcuts and artwork are removed. Games you keep stay under \"Missing games\"."), list.el],
    [{ label: "Keep in Steam", value: "keep" }, { label: "Remove selected", value: "remove", kind: "primary" }]);
  dialogFor = null;
  const ids = choice === "remove" ? list.selected().map(Number) : [];
  const kept = games.map((g) => g.id).filter((id) => !ids.includes(id));
  if (choice !== null && kept.length) await invoke("keep_gone", { ids: kept });
  if (!ids.length) return refresh();
  const restart = await askRestart("the change");
  if (restart === null) return;
  await run(() => invoke("remove_games", { ids, restart }));
}

async function showFoundBatch() {
  dialogFor = "found";
  const items = addable();
  items.forEach((f) => askedFound.add(f.folder));
  const list = checklist(items.map((f) => ({ key: f.folder, name: f.title, note: relative(f.folder, f.candidates[0].path) })), "New games");
  const choice = await ask(`${items.length} new games found. Add them to Steam?`,
    [h("p", {}, "Untick any you don't want yet. SteamDrop picked the file that starts each game; to change one, press \"Not now\" and use that game's card on the Games page."), list.el],
    [{ label: "Not now", value: null }, { label: "Add selected", value: "add", kind: "primary" }]);
  dialogFor = null;
  const picked = choice === "add" ? items.filter((f) => list.selected().includes(f.folder)) : [];
  if (!picked.length) return refresh();
  const restart = await askRestart(picked.length === 1 ? "the new game" : "the new games");
  if (restart === null) return;
  await run(() => invoke("add_games", { restart, games: picked.map((f) => ({ folder: f.folder, exe: f.candidates[0].path, name: f.title })) }));
}

async function checkNow() {
  checkMsg = { ok: true, text: "Checking for updates..." };
  render();
  try {
    const found = await invoke("check_now");
    checkMsg = found ? null : { ok: true, text: `You're on the latest version (${S.version}).` };
  } catch (e) {
    checkMsg = { ok: false, text: String(e) };
  }
  render();
}

const checkLine = () => checkMsg ? h("p", { class: checkMsg.ok ? "muted" : "error", role: "status" }, checkMsg.text) : null;

function showStatus(text, isError = false) {
  const el = $("#status");
  el.hidden = !text;
  el.textContent = text || "";
  el.classList.toggle("busy", busy && !isError);
  el.classList.toggle("error", isError);
}

/** Runs an app command with the busy spinner; shows errors in plain words. */
async function run(fn) {
  busy = true;
  render();
  showStatus("Working on it...");
  try {
    const msg = await fn();
    busy = false;
    showStatus(typeof msg === "string" ? msg : "");
    return msg;
  } catch (e) {
    busy = false;
    showStatus(String(e), true);
  } finally {
    busy = false;
    await refresh();
  }
}

const Z = SD.zoom("zoom_window", (patch) => saveSettings(patch));

async function saveSettings(patch) {
  S.settings = { ...S.settings, ...patch };
  try {
    await invoke("save_settings", { patch });
  } catch (e) {
    showStatus(String(e), true);
  }
  await refresh();
}

/* ---------- dialogs ---------- */
function ask(title, body, buttons) {
  const dlg = $("#dialog");
  return new Promise((resolve) => {
    const done = (v) => { dlg.close(); resolve(v); };
    dlg.replaceChildren(h("div", { class: "inner" },
      h("h2", {}, title),
      ...(Array.isArray(body) ? body : [h("p", {}, body)]),
      h("div", { class: "actions" },
        buttons.map((b) => h("button", { class: `btn ${b.kind || ""}`, onclick: () => done(b.value) }, b.label)))));
    dlg.oncancel = () => resolve(null);
    if (dlg.open) dlg.close();
    dlg.showModal();
    // Enter should do the main action, never "Skip" or "Cancel".
    dlg.querySelector(".actions .primary")?.focus();
  });
}

/** Steam only reads its shortcuts when it starts. Returns true/false, or null if the user cancelled. */
async function askRestart(what) {
  if (!S.steam.running || S.settings.auto_restart) return true;
  return ask("Steam needs to restart",
    `Steam is open right now. To show ${what}, Steam has to close for a moment and open again. Nothing in your library is lost.`,
    [
      { label: "Cancel", value: null },
      { label: "Do it, restart Steam later", value: false },
      { label: "Restart Steam now", value: true, kind: "primary" },
    ]);
}

/* ---------- actions ---------- */
async function addGame(folder, exe, name) {
  const restart = await askRestart("the new game");
  if (restart === null) return;
  await run(() => invoke("add_game", { folder, exe, name, restart }));
}

async function removeGame(game) {
  const body = [h("p", {}, `Only the Steam shortcut and its artwork are removed. The game's files stay exactly where they are.`)];
  const choice = await ask(`Remove "${game.name}" from Steam?`, body, [
    { label: "Cancel", value: null },
    { label: "Remove from Steam", value: "yes", kind: "danger" },
  ]);
  if (!choice) return;
  const restart = await askRestart("the change");
  if (restart === null) return;
  $("#dialog").close();
  await run(() => invoke("remove_game", { id: game.id, restart }));
}

async function refreshGame(game, steamAppid = null) {
  const restart = await askRestart("the new artwork");
  if (restart === null) return;
  $("#dialog").close();
  await run(() => invoke("refresh_game", { id: game.id, steamAppid, restart }));
}

async function addGameFolder() {
  const path = await invoke("pick_folder");
  if (path) await invoke("inspect_folder", { path });
}

async function addWatchFolder() {
  const path = await invoke("pick_folder");
  if (!path) return;
  const count = await invoke("count_subfolders", { path });
  let includeExisting = false;
  if (count > 0) {
    const n = `${count} game${count === 1 ? "" : "s"}`;
    const choice = await ask("What about games already in this folder?",
      `This folder already has ${n} in it. SteamDrop can leave them alone and only add games that arrive from now on, or add these too. You can still add them later with "Add existing games now".`,
      [
        { label: `Also add the ${n} already here`, value: "all" },
        { label: "Only new games from now on", value: "new", kind: "primary" },
      ]);
    if (choice === null) return;
    includeExisting = choice === "all";
  }
  await invoke("add_watch_folder", { path, includeExisting });
}

/* ---------- views ---------- */
function render() {
  if (!S) return;
  for (const t of document.querySelectorAll(".tab")) {
    if (t.dataset.view === view) t.setAttribute("aria-current", "page");
    else t.removeAttribute("aria-current");
  }
  for (const v of ["games", "settings", "about"]) $(`#view-${v}`).hidden = v !== view;
  const light = S.settings.theme === "light";
  $("#theme").textContent = light ? "☾" : "☀";
  const target = $(`#view-${view}`);
  // Re-rendering replaces the elements; keep the same spot focused for keyboard and controller users.
  const at = focusables().indexOf(document.activeElement);
  target.replaceChildren(...({ games: gamesView, settings: settingsView, about: aboutView })[view]());
  if (at >= 0 && document.activeElement === document.body) focusables()[Math.min(at, focusables().length - 1)]?.focus({ preventScroll: true });
}

function relative(folder, path) {
  return path.toLowerCase().startsWith(folder.toLowerCase()) ? path.slice(folder.length).replace(/^[\\/]/, "") : path;
}
const mb = (bytes) => bytes >= 1048576 ? `${Math.round(bytes / 1048576)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;

function foundCard(f) {
  if (f.installer_only) {
    return h("div", { class: "panel found installer" },
      h("h2", {}, "Only an installer found"),
      h("p", { class: "muted path" }, f.folder),
      h("p", {}, `"${f.title}" holds an installer, not a ready-to-play game. Run the installer first, then put the installed game folder here.`),
      h("div", { class: "row" }, btn("OK", () => invoke("dismiss_found", { folder: f.folder }), "primary")));
  }
  const nameInput = h("input", { type: "text", value: f.title, "aria-label": "Name in Steam" });
  const options = f.candidates.slice(0, 6).map((c) => ({ path: c.path, size: c.size }));
  const group = `exe-${Math.random().toString(36).slice(2)}`;
  const list = h("div", { class: "exe-list", role: "radiogroup" });
  const addOption = (o, i, checked) => list.append(h("label", { class: "exe-option" },
    h("input", { type: "radio", name: group, value: o.path, checked }),
    h("span", { class: "path" }, relative(f.folder, o.path)),
    o.size != null ? h("span", { class: "muted small" }, mb(o.size)) : null,
    i === 0 ? h("span", { class: "badge" }, "Best match") : null));
  options.forEach((o, i) => addOption(o, i, i === 0));
  const choose = async () => {
    const path = await invoke("pick_exe", { folder: f.folder });
    if (path) addOption({ path, size: null }, -1, true);
  };
  const add = () => {
    const picked = list.querySelector("input:checked");
    if (!picked) return showStatus("Choose the file that starts the game first.", true);
    addGame(f.folder, picked.value, nameInput.value);
  };
  return h("div", { class: "panel found" },
    h("h2", {}, "New game found"),
    h("p", { class: "muted path" }, f.folder),
    h("label", {}, "Name in Steam"), nameInput,
    h("label", {}, "Game file"),
    h("p", { class: "muted small" }, options.length
      ? "The file that starts the game. SteamDrop picked the most likely one; change it if it's wrong."
      : "SteamDrop couldn't find a file that starts the game. Choose it yourself, or skip this folder."),
    list,
    h("div", { class: "row" },
      btn("Add to Steam", add, "primary"),
      btn("Choose a different file...", choose),
      btn("Not now", () => invoke("dismiss_found", { folder: f.folder }), "", { title: "SteamDrop won't ask about this folder again." })));
}

function gameCard(g) {
  const cover = h("button", { class: "cover", onclick: () => showDetails(g), "aria-label": `Details for ${g.name}` },
    SD.art(g, "portrait"));
  return h("div", { class: "card" }, cover,
    h("div", { class: "body" },
      h("div", { class: "name" }, g.name),
      g.folder_missing ? h("div", {}, h("span", { class: "badge warn" }, "Game file missing")) :
        !g.in_steam ? h("div", {}, h("span", { class: "badge warn" }, "Not in Steam")) : null,
      h("div", { class: "actions" },
        btn("Details", () => showDetails(g), "small"),
        btn("Remove", () => removeGame(g), "small danger"))));
}

function gamesView() {
  const out = [];
  if (!S.steam.path) {
    out.push(h("div", { class: "panel warn" }, h("h3", {}, "Steam wasn't found"),
      h("p", {}, "SteamDrop couldn't find Steam on this PC. If Steam is installed somewhere unusual, choose its folder in Settings."),
      btn("Open Settings", () => { view = "settings"; render(); })));
  } else if (S.steam.users === 0) {
    out.push(h("div", { class: "panel warn" }, h("h3", {}, "No Steam account found"),
      h("p", {}, "Open Steam and sign in once. After that SteamDrop can add games to your library.")));
  }
  out.push(...S.found.map(foundCard));
  if (S.settings.watched.length === 0) {
    out.push(h("div", { class: "panel note" },
      h("h3", {}, "Tell SteamDrop where your games go"),
      h("p", {}, "Choose the folder that holds your game folders (for example D:\\Games). Whenever a new game folder lands there, SteamDrop adds it to Steam."),
      btn("Choose a folder to watch", addWatchFolder, "primary")));
  }
  out.push(h("div", { class: "spread" },
    h("div", {}, h("h1", {}, "Your games"),
      h("p", { class: "muted" }, S.games.length ? `${S.games.length} game${S.games.length === 1 ? "" : "s"} added by SteamDrop` : "Games you add show up here.")),
    btn("+ Add a game", addGameFolder, "primary")));
  const missing = S.games.filter((g) => g.folder_missing);
  if (missing.length) out.splice(out.length - 1, 0, missingPanel(missing));
  if (S.games.length) {
    out.push(h("div", { class: "grid" }, S.games.filter((g) => !g.folder_missing).map(gameCard)));
  } else {
    out.push(h("div", { class: "panel empty" }, logo(),
      h("h2", {}, "No games yet"),
      h("p", { class: "muted" }, "Copy a game folder into a watched folder, or add one yourself."),
      h("div", { class: "row" }, btn("+ Add a game", addGameFolder, "primary"))));
  }
  return out;
}

async function showDetails(g) {
  const dlg = $("#dialog");
  const infoBox = h("div", {}, h("p", { class: "muted" }, "Loading game details..."));
  dlg.replaceChildren(h("div", { class: "inner" },
    h("div", { class: "info" },
      SD.art(g, "portrait"),
      h("div", {}, h("h2", {}, g.name), infoBox)),
    h("div", { class: "actions" },
      btn("Open game folder", () => invoke("open", { target: g.folder }).catch((e) => showStatus(String(e), true)), "small"),
      btn(g.in_steam ? "Find artwork again" : "Add to Steam again", () => refreshGame(g), "small"),
      btn("Remove from Steam", () => removeGame(g), "small danger"),
      btn("Close", () => dlg.close(), "small primary", { always: true }))));
  dlg.oncancel = null;
  dlg.showModal();
  let info = null;
  try {
    info = await invoke("get_info", { id: g.id });
  } catch (e) {
    infoBox.replaceChildren(h("p", { class: "error" }, String(e)));
    return;
  }
  if (info) {
    const row = (label, value) => value ? [h("dt", {}, label), h("dd", {}, value)] : [];
    infoBox.replaceChildren(
      info.description ? h("p", {}, info.description) : null,
      h("dl", {},
        row("Developer", info.developers.join(", ")),
        row("Publisher", info.publishers.join(", ")),
        row("Genres", info.genres.join(", ")),
        row("Released", info.release_date)),
      h("div", { class: "row" }, btn("Open store page", () => invoke("open", { target: info.store_url }), "small", { always: true })));
  } else {
    const idInput = h("input", { type: "text", inputmode: "numeric", placeholder: "For example 413150", "aria-label": "Steam App ID" });
    infoBox.replaceChildren(
      h("p", {}, "SteamDrop couldn't find this game in the Steam store."),
      h("p", { class: "muted small" }, "If the game has a Steam store page, its App ID is the number in the page address (store.steampowered.com/app/NUMBER). Enter it to get the official details and artwork."),
      idInput,
      h("div", { class: "row" }, btn("Use this App ID", () => {
        const id = parseInt(idInput.value.trim(), 10);
        if (!id || id < 1) return showStatus("The App ID is a number, like 413150.", true);
        refreshGame(g, id);
      }, "small")));
  }
}

function toggle(checked, onchange, label) {
  return h("label", { class: "switch" },
    h("input", { type: "checkbox", role: "switch", checked, "aria-label": label, onchange: (e) => onchange(e.target.checked) }),
    h("span", { class: "track" }));
}

function setting(title, text, control) {
  return h("div", { class: "setting" }, h("div", { class: "text" }, h("strong", {}, title), h("span", {}, text)), control);
}

/** Theme cards, each drawn in its own colors. Pointing at or focusing one previews it; clicking keeps it. */
function themePicker(st) {
  const saved = SD.themeId(st.theme);
  const restore = () => SD.applyTheme(S.settings.theme);
  return h("div", { class: "themes", role: "radiogroup", "aria-label": "Theme", onmouseleave: restore,
    onfocusout: (e) => { if (!e.currentTarget.contains(e.relatedTarget)) restore(); } },
    SD.THEMES.map(([id, name]) => h("button", {
      class: "theme", role: "radio", "aria-checked": String(id === saved), "data-theme": id,
      onmouseenter: () => SD.applyTheme(id), onfocus: () => SD.applyTheme(id), onclick: () => saveSettings({ theme: id }),
    }, h("span", { class: "swatches", "aria-hidden": "true" }, h("i"), h("i"), h("i"), h("i")), h("span", {}, name))));
}

/** SteamGridDB key box: dots by default, the eye button shows it only while held down. */
function keyField() {
  const input = h("input", {
    type: "password", value: sgdbKey || "", placeholder: "Paste your SteamGridDB key here",
    "aria-label": "SteamGridDB key", autocomplete: "off", spellcheck: "false",
    oninput: (e) => { sgdbKey = e.target.value; },
  });
  const show = (on) => { input.type = on ? "text" : "password"; eye.setAttribute("aria-pressed", String(on)); };
  const eye = h("button", {
    class: "btn eye", type: "button", title: "Hold to show the key", "aria-label": "Hold to show the key", "aria-pressed": "false",
    onpointerdown: (e) => { e.preventDefault(); eye.setPointerCapture(e.pointerId); show(true); },
    onpointerup: () => show(false),
    onpointercancel: () => show(false),
    onlostpointercapture: () => show(false),
    onkeydown: (e) => { if (e.key === " " || e.key === "Enter") { e.preventDefault(); if (!e.repeat) show(true); } },
    onkeyup: (e) => { if (e.key === " " || e.key === "Enter") { e.preventDefault(); show(false); } },
    onblur: () => show(false),
    onclick: (e) => e.preventDefault(),
    oncontextmenu: (e) => e.preventDefault(),
  });
  eye.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5C6.5 5 2.7 9.2 1.5 12c1.2 2.8 5 7 10.5 7s9.3-4.2 10.5-7C21.3 9.2 17.5 5 12 5Zm0 11a4 4 0 1 1 0-8 4 4 0 0 1 0 8Zm0-2.2a1.8 1.8 0 1 0 0-3.6 1.8 1.8 0 0 0 0 3.6Z" fill="currentColor"/></svg>';
  const message = keyMsg ? h("p", { class: keyMsg.ok ? "ok-msg" : "error", role: "status" }, keyMsg.text) : null;
  return h("div", { class: "key-field" },
    h("label", { class: "small" }, h("strong", {}, "SteamGridDB key (free, optional)")),
    h("div", { class: "row nowrap" }, h("div", { class: "grow" }, input), eye),
    h("div", { class: "row" },
      btn("Save key", async () => {
        try {
          await invoke("set_sgdb_key", { key: input.value });
          keyMsg = { ok: true, text: input.value.trim() ? "Key saved." : "Key removed." };
        } catch (e) { keyMsg = { ok: false, text: String(e) }; }
        render();
      }),
      btn("Test key", async () => {
        keyMsg = { ok: true, text: "Testing the key..." };
        render();
        try { keyMsg = { ok: true, text: await invoke("test_sgdb_key", { key: input.value }) }; }
        catch (e) { keyMsg = { ok: false, text: String(e) }; }
        render();
      }),
      btn("Get a free key", () => invoke("open", { target: SGDB_API_PAGE }), "", { always: true })),
    message);
}

/** Settings > Help: recent activity in plain words, and an easy way to report a problem. */
function helpPanel() {
  const list = h("ol", { class: "activity", "aria-label": "Recent activity" }, h("li", { class: "muted" }, "Loading..."));
  invoke("get_activity").then((lines) => {
    const items = lines.slice(-80).reverse().map((line) => {
      const time = line.slice(0, 19);
      const text = line.slice(21);
      return h("li", { class: text.startsWith("Problem:") ? "problem" : "" }, h("time", {}, time), h("span", {}, text));
    });
    list.replaceChildren(...(items.length ? items : [h("li", { class: "muted" }, "Nothing has happened yet.")]));
  });
  const copyReport = async () => {
    const report = await invoke("bug_report");
    try {
      await navigator.clipboard.writeText(report);
      showStatus("Bug report copied. Paste it into your report on GitHub.");
    } catch {
      // Clipboard blocked: show the report so it can be copied by hand.
      await ask("Your bug report", [h("p", {}, "Select all of this text (Ctrl+A), copy it (Ctrl+C), and paste it into your report."),
        h("textarea", { class: "report", readonly: true, rows: 12 }, report)], [{ label: "Close", value: null, kind: "primary" }]);
    }
  };
  return h("div", { class: "panel" },
    h("h2", {}, "Help"),
    h("p", { class: "muted" }, "Something not working? Press \"Copy bug report\" and paste it into a new report on GitHub. It contains SteamDrop's recent activity and settings, with your Windows user name and SteamGridDB key left out."),
    h("div", { class: "row" },
      btn("Copy bug report", copyReport, "primary", { always: true }),
      btn("Report a problem", () => invoke("open", { target: `${REPO}/issues/new?template=bug_report.yml` }), "", { always: true }),
      btn("Open log folder", () => invoke("open_logs").catch((e) => showStatus(String(e), true)), "", { always: true })),
    h("h3", { class: "activity-title" }, "Activity"),
    list);
}

const inside = (path, folder) => path.toLowerCase().startsWith(folder.toLowerCase().replace(/[\\/]+$/, "") + "\\");

/** One watched folder: add the games already there, exclude subfolders, stop watching. */
function watchedFolder(st, w) {
  // An exclusion belongs to the innermost watched folder that holds it.
  const owner = (x) => st.watched.filter((v) => inside(x, v)).sort((a, b) => b.length - a.length)[0];
  const excluded = st.excluded.filter((x) => owner(x) === w);
  const exclude = async () => {
    const p = await invoke("pick_folder", { start: w });
    if (!p) return;
    if (!inside(p, w)) return showStatus(`Choose a folder inside ${w}.`, true);
    if (!st.excluded.some((x) => x.toLowerCase() === p.toLowerCase())) saveSettings({ excluded: [...st.excluded, p] });
  };
  return h("li", { class: "watched" },
    h("div", { class: "spread" }, h("span", { class: "path" }, w),
      h("div", { class: "row" },
        btn("Add existing games now", () => run(() => invoke("add_existing_games", { path: w })), "small"),
        btn("Exclude a subfolder...", exclude, "small"),
        btn("Stop watching", () => saveSettings({ watched: st.watched.filter((x) => x !== w), excluded: st.excluded.filter((x) => owner(x) !== w) }), "small"))),
    excluded.length ? h("ul", { class: "excluded", "aria-label": `Excluded folders in ${w}` }, excluded.map((x) => h("li", {},
      h("span", { class: "path" }, `Excluded: ${x}`),
      btn("Remove", () => saveSettings({ excluded: st.excluded.filter((y) => y !== x) }), "small")))) : null);
}

/** Games whose folder was moved or deleted. Nothing changes in Steam until the user says so. */
function missingPanel(missing) {
  const find = async (g) => {
    const folder = await invoke("pick_folder", { start: null });
    if (!folder) return;
    const restart = await askRestart(`"${g.name}" in its new place`);
    if (restart === null) return;
    await run(() => invoke("find_game", { id: g.id, folder, restart }));
  };
  return h("div", { class: "panel warn" },
    h("h2", {}, "Missing games"),
    h("p", {}, "These games' folders were moved or deleted. SteamDrop hasn't changed anything in Steam. Find each one in its new place, or remove it from Steam."),
    h("ul", { class: "folder-list" }, missing.map((g) => h("li", {},
      h("span", {}, h("strong", {}, g.name), h("br", {}), h("span", { class: "path muted small" }, g.folder)),
      h("div", { class: "row" }, btn("Find it", () => find(g), "small"), btn("Remove from Steam", () => removeGame(g), "small danger"))))));
}

/** "Add games to": every Steam account on this PC, or a checklist of chosen ones. */
function accountsPicker(st) {
  const accounts = S.steam.accounts || [];
  if (!accounts.length) return h("p", { class: "muted" }, "No Steam accounts found yet. Sign in to Steam once on this PC.");
  const chosen = new Set(st.accounts);
  const toggle = (id, on) => {
    const next = new Set(chosen);
    if (on) next.add(id); else next.delete(id);
    saveSettings({ accounts: [...next] });
  };
  return h("div", { class: "accounts" },
    setting("Add games to", accounts.length === 1 ? "There is one Steam account on this PC." : `There are ${accounts.length} Steam accounts on this PC.`,
      h("div", { class: "seg", role: "group", "aria-label": "Add games to" },
        h("button", { "aria-pressed": String(st.all_accounts), onclick: () => saveSettings({ all_accounts: true }) }, "All Steam accounts"),
        h("button", { "aria-pressed": String(!st.all_accounts), onclick: () => saveSettings({ all_accounts: false, accounts: st.accounts.length ? st.accounts : accounts.map((a) => a.id) }) }, "Only these accounts"))),
    st.all_accounts ? null : h("div", { class: "exe-list", role: "group", "aria-label": "Steam accounts" },
      accounts.map((a) => h("label", { class: "exe-option" },
        h("input", { type: "checkbox", checked: chosen.has(a.id), onchange: (e) => toggle(a.id, e.target.checked) }),
        h("span", {}, h("strong", {}, a.persona), a.login ? h("span", { class: "muted small" }, `  ${a.login}`) : null),
        h("span", { class: "muted small" }, `#${a.id}`)))),
    !st.all_accounts && !chosen.size ? h("p", { class: "error" }, "Choose at least one account, or SteamDrop can't add games.") : null,
    h("p", { class: "muted small" }, "Removing a game always removes it from every account it was added to."));
}

function settingsView() {
  const st = S.settings;
  const subs = [st.auto_add, st.auto_restart, st.notify];
  const mode = subs.every(Boolean) ? "On" : subs.some(Boolean) ? "Custom" : "Off";
  const mainSwitch = h("label", { class: `switch ${mode === "Custom" ? "mixed" : ""}` },
    h("input", {
      type: "checkbox", role: "switch", checked: mode === "On", "aria-label": `Automatic mode: ${mode}`,
      onchange: () => { const on = mode !== "On"; saveSettings({ auto_add: on, auto_restart: on, notify: on }); },
    }),
    h("span", { class: "track" }));

  const steamNote = !S.steam.path ? "Not found. Choose Steam's folder."
    : st.steam_path ? "Chosen by you." : "Found automatically.";
  if (sgdbKey === null) {
    sgdbKey = "";
    invoke("get_sgdb_key").then((k) => { sgdbKey = k; render(); });
  }

  return [
    h("h1", {}, "Settings"),
    h("div", { class: "panel" },
      h("div", { class: "spread" },
        h("div", { class: "text" }, h("h2", {}, "Automatic mode"),
          h("p", { class: "muted" }, "On: SteamDrop adds new games by itself, restarts Steam and lets you know. Off: it asks you first.")),
        h("div", { class: "main-switch" }, h("span", { class: `mode-label ${mode.toLowerCase()}` }, mode), mainSwitch)),
      h("div", { class: "sub" },
        setting("Add games without asking", "New game folders go straight into Steam.",
          toggle(st.auto_add, (v) => saveSettings({ auto_add: v }), "Add games without asking")),
        setting("Restart Steam automatically", "Steam only shows new games after a restart. When this is off, SteamDrop asks first.",
          toggle(st.auto_restart, (v) => saveSettings({ auto_restart: v }), "Restart Steam automatically")),
        setting("Show notifications", "A small message in the corner of the screen when a game is found or added.",
          toggle(st.notify, (v) => saveSettings({ notify: v }), "Show notifications")))),

    h("div", { class: "panel" },
      h("h2", {}, "Watched folders"),
      h("p", { class: "muted" }, "SteamDrop keeps an eye on these folders. Pick the folder that holds your game folders, not a single game's folder."),
      st.watched.length
        ? h("ul", { class: "folder-list" }, st.watched.map((w) => watchedFolder(st, w)))
        : h("p", {}, "No folders yet."),
      h("p", { class: "muted small" }, "Each folder directly inside a watched folder is one game. A watched folder inside another one is skipped by the outer one, and excluded folders are ignored with everything in them."),
      btn("Add a folder to watch", addWatchFolder, "primary")),

    h("div", { class: "panel" },
      h("h2", {}, "Steam"),
      h("p", {}, h("strong", {}, "Steam folder: "), h("span", { class: "path" }, S.steam.path || "not found"), " ", h("span", { class: "muted" }, steamNote)),
      S.steam.path ? accountsPicker(st) : null,
      setting("When SteamDrop restarts Steam", "Keep current mode: Steam comes back in Big Picture only if Big Picture was open just before, otherwise in the normal desktop window.",
        h("div", { class: "seg", role: "group", "aria-label": "When SteamDrop restarts Steam" },
          [["keep", "Keep current mode"], ["bigpicture", "Always Big Picture"], ["desktop", "Always desktop mode"]].map(([v, label]) =>
            h("button", { "aria-pressed": String((st.steam_restart_mode || "keep") === v), onclick: () => saveSettings({ steam_restart_mode: v }) }, label)))),
      h("div", { class: "row" },
        btn("Choose Steam folder...", async () => {
          const p = await invoke("pick_folder");
          if (p) saveSettings({ steam_path: p });
        }),
        st.steam_path ? btn("Find automatically", () => saveSettings({ steam_path: "" })) : null)),

    h("div", { class: "panel" },
      h("h2", {}, "Artwork"),
      h("p", { class: "muted" }, "SteamDrop fills in all of Steam's pictures for each game: cover, wide banner, background, logo and icon. It uses official Steam art when the game is on Steam, then SteamGridDB, and otherwise makes a clean title card."),
      keyField(),
      h("details", { class: "guide" },
        h("summary", {}, "How to get a free SteamGridDB key"),
        h("ol", {},
          h("li", {}, "Go to ", h("a", { href: "#", onclick: (e) => { e.preventDefault(); invoke("open", { target: "https://www.steamgriddb.com/" }); } }, "steamgriddb.com"), " and sign in with your Steam account (top right)."),
          h("li", {}, "Open ", h("a", { href: "#", onclick: (e) => { e.preventDefault(); invoke("open", { target: "https://www.steamgriddb.com/profile/preferences/api" }); } }, "Preferences, then API"), "."),
          h("li", {}, "Press \"Generate API Key\" and copy the key."),
          h("li", {}, "Paste it above and press \"Save key\".")),
        h("p", { class: "muted small" }, "The key is kept in Windows Credential Manager on this PC and is only ever sent to SteamGridDB.")),
      setting("Also save art in the game folder", "Keeps a copy in a \"SteamDrop art\" folder inside each game, so it comes back if you move the game.",
        toggle(st.art_in_game_folder, (v) => saveSettings({ art_in_game_folder: v }), "Also save art in the game folder"))),

    h("div", { class: "panel" },
      h("h2", {}, "App"),
      setting("Run when Windows starts", "SteamDrop starts quietly in the tray, next to the clock.",
        toggle(st.run_on_startup, (v) => saveSettings({ run_on_startup: v }), "Run when Windows starts")),
      setting("Open Steam in Big Picture when Windows starts", "For a gaming PC on the TV: when you sign in to Windows, Steam opens in Big Picture, its full-screen mode for controllers. If Steam is already in Big Picture, nothing changes. This works even if SteamDrop itself doesn't start with Windows.",
        toggle(st.big_picture_at_sign_in, (v) => saveSettings({ big_picture_at_sign_in: v }), "Open Steam in Big Picture when Windows starts")),
      setting("Keep running in the tray when closed", "Closing the window keeps SteamDrop watching your folders. Quit it from the tray icon.",
        toggle(st.close_to_tray, (v) => saveSettings({ close_to_tray: v }), "Keep running in the tray when closed")),
      setting("Play a sound when a game is added", "A short chime, so you hear it even while Steam's Big Picture is on screen.",
        toggle(st.chime, (v) => saveSettings({ chime: v }), "Play a sound when a game is added")),
      st.chime ? h("div", { class: "row volume" },
        h("label", { for: "chime-volume" }, "Volume"),
        h("input", { id: "chime-volume", type: "range", min: "0", max: "100", step: "5", value: String(st.chime_volume),
          oninput: (e) => { e.target.nextSibling.textContent = `${e.target.value}%`; },
          onchange: (e) => saveSettings({ chime_volume: Number(e.target.value) }) }),
        h("span", { class: "muted" }, `${st.chime_volume}%`),
        btn("Test", () => invoke("test_chime", { volume: Number($("#chime-volume").value) }), "small", { always: true })) : null,
      h("div", { class: "row" }, btn("Quit SteamDrop", () => invoke("quit"), "", { always: true }))),

    h("div", { class: "panel" },
      h("h2", {}, "Appearance"),
      h("p", { class: "muted" }, "Point at a theme (or move to it with the keyboard or a controller) to try it; click it to keep it. Couch mode uses the same theme."),
      themePicker(st),
      setting("Interface size", "Makes everything in this window bigger or smaller. Shortcuts: Ctrl and plus, Ctrl and minus, Ctrl and 0 to reset, or Ctrl and the mouse wheel.",
        h("div", { class: "row nowrap zoom", role: "group", "aria-label": "Interface size" },
          btn("−", () => Z.step(-1), "small", { always: true, "aria-label": "Smaller", disabled: st.zoom_window <= 50 }),
          h("span", { class: "zoom-level", "aria-live": "polite" }, `${st.zoom_window}%`),
          btn("+", () => Z.step(1), "small", { always: true, "aria-label": "Bigger", disabled: st.zoom_window >= 200 }),
          btn("Reset", Z.reset, "small", { always: true, disabled: st.zoom_window === 100 })))),

    h("div", { class: "panel" },
      h("h2", {}, "Couch mode"),
      h("p", { class: "muted" }, "A full-screen view for the TV that you control with a gamepad. It opens when you start SteamDrop from your Steam library (for example in Big Picture)."),
      setting("Show SteamDrop in my Steam library", "A SteamDrop tile with its own artwork in your Steam library, so you can open it from the couch. Turning it off removes only that tile; your games stay.",
        toggle(st.steam_shortcut, (v) => run(() => invoke("set_steam_shortcut", { on: v })), "Show SteamDrop in my Steam library")),
      setting("When opened from Steam", "Auto: Couch mode in Big Picture (and on a Steam Deck), this window in normal Steam. You can switch anytime with F11.",
        h("div", { class: "seg", role: "group", "aria-label": "When opened from Steam" },
          [["auto", "Auto"], ["couch", "Always Couch mode"], ["window", "Always window"]].map(([v, label]) =>
            h("button", { "aria-pressed": String((st.steam_view || "auto") === v), onclick: () => saveSettings({ steam_view: v }) }, label)))),
      h("div", { class: "row" }, btn("Open Couch mode", () => invoke("open_couch"), "", { always: true }))),

    h("div", { class: "panel" },
      h("h2", {}, "Updates"),
      h("p", { class: "muted" }, `You have SteamDrop ${S.version}. Updates come from SteamDrop's GitHub page.`),
      setting("Check for updates on startup", "Looks for a new version each time SteamDrop starts.",
        toggle(st.update_on_startup, (v) => saveSettings({ update_on_startup: v }), "Check for updates on startup")),
      setting("Also check once a day while running", "Looks again every 24 hours while SteamDrop sits in the tray.",
        toggle(st.update_daily, (v) => saveSettings({ update_daily: v }), "Also check once a day while running")),
      setting("Install updates automatically", "Downloads and installs new versions by itself, never while you're playing a game. You'll get a notification afterward.",
        toggle(st.update_auto_install, (v) => saveSettings({ update_auto_install: v }), "Install updates automatically")),
      h("div", { class: "row" }, btn("Check now", checkNow, "primary", { always: true })),
      checkLine()),

    helpPanel(),
  ];
}

function aboutView() {
  const link = (label, url) => btn(label, () => invoke("open", { target: url }), "", { always: true });
  return [
    h("div", { class: "panel" },
      h("div", { class: "about-head" }, logo(),
        h("div", {}, h("h1", {}, "SteamDrop"), h("p", { class: "muted" }, `Version ${S.version} · Free and open source (MIT)`))),
      h("p", {}, "Drop a game folder in, play it from Steam. SteamDrop adds games stored on your PC to your Steam library, with artwork."),
      h("div", { class: "row" },
        btn("Check for updates", checkNow, "primary", { always: true }),
        link("Source code", REPO),
        link("Report a problem", `${REPO}/issues`)),
      checkLine()),
    h("div", { class: "panel" },
      h("h2", {}, "Legal"),
      h("p", { class: "legal" }, LEGAL)),
    h("div", { class: "panel" },
      h("h2", {}, "Thanks"),
      h("p", {}, "Built with Tauri. Fonts: Chakra Petch and Atkinson Hyperlegible, under the SIL Open Font License. Artwork comes from Steam and SteamGridDB and is fetched only when needed."),
      link("Third-party credits", `${REPO}/blob/main/THIRD_PARTY.md`)),
  ];
}

/* ---------- control bar (also F11, and Y / View on a controller) ---------- */
async function scanNow() {
  showStatus("Looking for new games...");
  try { showStatus(await invoke("scan_now")); } catch (e) { showStatus(String(e), true); }
}
async function quitSteamDrop() {
  const ok = await ask("Stop adding new games?", "SteamDrop will stop watching your folders until you start it again.",
    [{ label: "Cancel", value: null }, { label: "Quit SteamDrop", value: "quit", kind: "danger" }]);
  if (ok) invoke("quit");
}
const switchView = () => invoke("switch_view", { from: "main" });
$("#c-scan").addEventListener("click", scanNow);
$("#c-view").addEventListener("click", switchView);
$("#c-hide").addEventListener("click", () => invoke("hide_window", { label: "main" }));
$("#c-close").addEventListener("click", () => invoke("close_window", { label: "main" }));
$("#c-quit").addEventListener("click", quitSteamDrop);
document.addEventListener("keydown", (e) => { if (e.key === "F11") { e.preventDefault(); switchView(); } });

/* ---------- controller (same code as Couch mode, in shared.js) ----------
   D-pad / stick move focus, A selects, B closes a dialog (or goes back to Games), LB/RB switch tabs,
   Y = Scan now, View = back to Couch mode, LT/RT = interface size. */
const TABS = ["games", "settings", "about"];
function focusables() {
  const dlg = $("#dialog");
  const root = dlg.open ? dlg : document;
  return [...root.querySelectorAll("button, input, select, a[href], summary")].filter((el) => !el.disabled && el.offsetParent !== null);
}
function padPress(action) {
  if (!document.hasFocus()) return;
  document.body.classList.add("pad");
  const dlg = $("#dialog");
  const moves = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] };
  if (moves[action]) SD.move(...moves[action], focusables());
  else if (action === "a") document.activeElement?.click();
  else if (action === "b") {
    if (dlg.open) { dlg.dispatchEvent(new Event("cancel")); dlg.close(); }
    else if (view !== "games") { view = "games"; render(); }
  } else if ((action === "lb" || action === "rb") && !dlg.open) {
    view = TABS[(TABS.indexOf(view) + (action === "rb" ? 1 : TABS.length - 1)) % TABS.length];
    render();
    focusables()[0]?.focus();
  } else if (action === "y") scanNow();
  else if (action === "view") switchView();
  else if (action === "lt") Z.step(-1);
  else if (action === "rt") Z.step(1);
}
SD.startPad(padPress, () => {});
// The big focus ring is for controller use; a mouse click turns it back to normal.
document.addEventListener("mousedown", () => document.body.classList.remove("pad"));

/* ---------- start ---------- */
for (const t of document.querySelectorAll(".tab")) {
  t.addEventListener("click", () => { view = t.dataset.view; render(); window.scrollTo(0, 0); });
}
$("#theme").addEventListener("click", () => saveSettings({ theme: S.settings.theme === "light" ? "dark" : "light" }));
listen("changed", refresh);
// Games added while this window was hidden or closed show up as soon as it's back.
window.addEventListener("focus", refresh);
document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
listen("status", (e) => showStatus(e.payload));
refresh().then(() => { if (S.status) showStatus(S.status); });
