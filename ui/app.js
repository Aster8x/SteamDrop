"use strict";
const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const REPO = "https://github.com/Aster8x/SteamDrop";
const LEGAL = "SteamDrop is an independent, open source tool for adding games stored on your own PC to your Steam library. It does not provide, host, or link to any games, and does not encourage or enable illegal access to copyrighted material. Only use it with games you have the right to use. SteamDrop is not affiliated with any cracking scene groups, nor with Valve Corporation or any game publisher. Steam is a trademark of Valve Corporation. All artwork and game information belong to their respective owners and are fetched at runtime from public sources. Provided as-is, with no warranty of any kind; use at your own risk.";

let S = null;          // last state from the app
let view = "games";
let busy = false;
let update = null;     // { checking, version, error, installing }

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
  document.documentElement.dataset.theme = S.settings.theme === "light" ? "light" : "dark";
  render();
}

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

async function saveSettings(patch) {
  S.settings = { ...S.settings, ...patch };
  try {
    await invoke("save_settings", { settings: S.settings });
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
    const choice = await ask("Add the games already there?",
      `This folder already has ${count} folder${count === 1 ? "" : "s"} in it. Should SteamDrop add those games to Steam too, or only games that arrive from now on?`,
      [
        { label: "Only new games", value: "new" },
        { label: "Add them too", value: "all", kind: "primary" },
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
  target.replaceChildren(...({ games: gamesView, settings: settingsView, about: aboutView })[view]());
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
    g.portrait ? h("img", { src: `${convertFileSrc(g.portrait)}?v=${g.added}`, alt: "" }) : null);
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
  if (S.games.length) {
    out.push(h("div", { class: "grid" }, S.games.map(gameCard)));
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
      g.portrait ? h("img", { src: `${convertFileSrc(g.portrait)}?v=${g.added}`, alt: "" }) : h("div", {}),
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
  const keyInput = h("input", { type: "password", value: st.sgdb_key, placeholder: "Paste your SteamGridDB key here", "aria-label": "SteamGridDB key", autocomplete: "off" });
  const keyResult = h("span", { class: "muted small" });

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
        ? h("ul", { class: "folder-list" }, st.watched.map((w) => h("li", {},
          h("span", { class: "path" }, w),
          btn("Stop watching", () => saveSettings({ watched: st.watched.filter((x) => x !== w) }), "small"))))
        : h("p", {}, "No folders yet."),
      btn("Add a folder to watch", addWatchFolder, "primary")),

    h("div", { class: "panel" },
      h("h2", {}, "Steam"),
      h("p", {}, h("strong", {}, "Steam folder: "), h("span", { class: "path" }, S.steam.path || "not found"), " ", h("span", { class: "muted" }, steamNote)),
      S.steam.path ? h("p", { class: "muted" }, `Steam accounts on this PC: ${S.steam.users}. Games are added to all of them.`) : null,
      h("div", { class: "row" },
        btn("Choose Steam folder...", async () => {
          const p = await invoke("pick_folder");
          if (p) saveSettings({ steam_path: p });
        }),
        st.steam_path ? btn("Find automatically", () => saveSettings({ steam_path: "" })) : null)),

    h("div", { class: "panel" },
      h("h2", {}, "Artwork"),
      h("p", { class: "muted" }, "SteamDrop fills in all of Steam's pictures for each game: cover, wide banner, background, logo and icon. It uses official Steam art when the game is on Steam, then SteamGridDB, and otherwise makes a clean title card."),
      h("label", { class: "small" }, h("strong", {}, "SteamGridDB key (free, optional)")),
      h("div", { class: "row" }, h("div", { class: "grow" }, keyInput),
        btn("Save key", async () => {
          await saveSettings({ sgdb_key: keyInput.value.trim() });
          showStatus("Key saved.");
        }),
        btn("Check key", async () => {
          keyResult.textContent = "Checking...";
          const ok = await invoke("test_sgdb_key", { key: keyInput.value });
          keyResult.textContent = ok ? "The key works." : "That key didn't work. Check it and try again.";
        })),
      keyResult,
      h("details", { class: "guide" },
        h("summary", {}, "How to get a free SteamGridDB key"),
        h("ol", {},
          h("li", {}, "Go to ", h("a", { href: "#", onclick: (e) => { e.preventDefault(); invoke("open", { target: "https://www.steamgriddb.com/" }); } }, "steamgriddb.com"), " and sign in with your Steam account (top right)."),
          h("li", {}, "Open ", h("a", { href: "#", onclick: (e) => { e.preventDefault(); invoke("open", { target: "https://www.steamgriddb.com/profile/preferences/api" }); } }, "Preferences, then API"), "."),
          h("li", {}, "Press \"Generate API Key\" and copy the key."),
          h("li", {}, "Paste it above and press \"Save key\".")),
        h("p", { class: "muted small" }, "The key stays on this PC and is only sent to SteamGridDB.")),
      setting("Also save art in the game folder", "Keeps a copy in a \"SteamDrop art\" folder inside each game, so it comes back if you move the game.",
        toggle(st.art_in_game_folder, (v) => saveSettings({ art_in_game_folder: v }), "Also save art in the game folder"))),

    h("div", { class: "panel" },
      h("h2", {}, "App"),
      setting("Run when Windows starts", "SteamDrop starts quietly in the tray, next to the clock.",
        toggle(st.run_on_startup, (v) => saveSettings({ run_on_startup: v }), "Run when Windows starts")),
      setting("Keep running in the tray when closed", "Closing the window keeps SteamDrop watching your folders. Quit it from the tray icon.",
        toggle(st.close_to_tray, (v) => saveSettings({ close_to_tray: v }), "Keep running in the tray when closed")),
      setting("Look", "Dark or light colors.",
        h("div", { class: "seg", role: "group", "aria-label": "Look" },
          h("button", { "aria-pressed": String(st.theme !== "light"), onclick: () => saveSettings({ theme: "dark" }) }, "Dark"),
          h("button", { "aria-pressed": String(st.theme === "light"), onclick: () => saveSettings({ theme: "light" }) }, "Light"))),
      h("div", { class: "row" }, btn("Quit SteamDrop", () => invoke("quit"), "", { always: true }))),
  ];
}

async function checkUpdate() {
  update = { checking: true };
  render();
  try {
    const version = await invoke("check_update");
    update = { version };
  } catch (e) {
    update = { error: String(e) };
  }
  render();
}

function aboutView() {
  let updateLine = null;
  if (update?.checking) updateLine = h("p", { class: "muted" }, "Checking for updates...");
  else if (update?.installing) updateLine = h("p", { class: "muted" }, "Downloading the update. SteamDrop will restart by itself.");
  else if (update?.error) updateLine = h("p", { class: "error" }, "Couldn't check for updates right now. Try again later.");
  else if (update && update.version) updateLine = h("div", { class: "row" },
    h("p", {}, `Version ${update.version} is available.`),
    btn("Install update", async () => {
      update = { installing: true };
      render();
      try { await invoke("install_update"); } catch (e) { update = { error: String(e) }; render(); }
    }, "primary"));
  else if (update) updateLine = h("p", { class: "muted" }, "You have the latest version.");

  const link = (label, url) => btn(label, () => invoke("open", { target: url }), "", { always: true });
  return [
    h("div", { class: "panel" },
      h("div", { class: "about-head" }, logo(),
        h("div", {}, h("h1", {}, "SteamDrop"), h("p", { class: "muted" }, `Version ${S.version} · Free and open source (MIT)`))),
      h("p", {}, "Drop a game folder in, play it from Steam. SteamDrop adds games stored on your PC to your Steam library, with artwork."),
      h("div", { class: "row" },
        btn("Check for updates", checkUpdate, "primary", { always: true }),
        link("Source code", REPO),
        link("Report a problem", `${REPO}/issues`)),
      updateLine),
    h("div", { class: "panel" },
      h("h2", {}, "Legal"),
      h("p", { class: "legal" }, LEGAL)),
    h("div", { class: "panel" },
      h("h2", {}, "Thanks"),
      h("p", {}, "Built with Tauri. Fonts: Chakra Petch and Atkinson Hyperlegible, under the SIL Open Font License. Artwork comes from Steam and SteamGridDB and is fetched only when needed."),
      link("Third-party credits", `${REPO}/blob/main/THIRD_PARTY.md`)),
  ];
}

/* ---------- start ---------- */
for (const t of document.querySelectorAll(".tab")) {
  t.addEventListener("click", () => { view = t.dataset.view; render(); window.scrollTo(0, 0); });
}
$("#theme").addEventListener("click", () => saveSettings({ theme: S.settings.theme === "light" ? "dark" : "light" }));
listen("changed", refresh);
listen("status", (e) => showStatus(e.payload));
refresh().then(() => { if (S.status) showStatus(S.status); });
