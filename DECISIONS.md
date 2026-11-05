# Decisions

Why things are the way they are. Newest decisions go at the bottom of each section.

## Project and code

- **Coding approach:** the smallest change that fully works, no speculative options, standard library first, and a small test for every non-trivial piece of logic.
- **Commit identity:** commits use the repository owner's GitHub identity (the same one as the initial commit).
- **Stack:** Tauri 2 with a Rust backend and a plain HTML/CSS/JS front end loaded straight from `ui/` (`withGlobalTauri`, no bundler, no framework). Dev tooling is only `@tauri-apps/cli`.
- **Dependencies kept minimal:** `notify` (folder events), `reqwest` (HTTP, the same version the updater already pulls in), `ab_glyph` + `png` (title cards), `crc32fast` (shortcut ids), `winreg` (Steam path, run on startup), plus Tauri's dialog, notification, single-instance and updater plugins. `png` and `reqwest` match the versions Tauri already uses, so nothing gets compiled twice.
- **TLS uses Windows' own stack** (`native-tls` / Schannel) instead of rustls. That means no C code to compile (`ring`) and a smaller binary.
- **The binary is named `SteamDrop.exe`** (`mainBinaryName`), so Task Manager and the uninstaller show a recognizable name.
- **Strict CSP and minimal permissions:** the window only has `core:default`. Folder and file pickers, opening links and folders all go through SteamDrop's own commands, which check their input (`open` accepts only `https://` links or existing folders).

## Steam

- **Own binary VDF reader/writer** (`vdf.rs`, ~100 lines) instead of a crate. It keeps every entry and field in order, including ones it doesn't know, so other tools' shortcuts survive untouched. If it finds a field type it doesn't understand, it refuses to write rather than guess.
- **Shortcut id** = `crc32(quoted exe + name) | 0x80000000`, stored in the shortcut's `appid` field. The same number names the grid art files.
- **Every Steam account on the PC** gets the shortcut ("per Steam user"). Accounts are the numeric folders under `userdata` that have a `config` folder.
- **Steam must be closed while `shortcuts.vdf` changes**, or Steam may overwrite it later. SteamDrop asks Steam to close (`steam.exe -shutdown`), waits until it has exited (checked every half second, up to a minute, and only during a restart), writes, then starts Steam again: in Big Picture (`-tenfoot`) after adding games, as before after removing them.
- **If Steam is open and the user declines a restart,** SteamDrop still writes the change and says "Restart Steam to see it". If Steam later overwrites the file, the game shows a **Not in Steam** badge with an **Add to Steam again** button. Waiting for Steam to close by itself would have needed polling.
- **Artwork is fetched before Steam closes,** so Steam is only down for the few seconds the write takes. Games found together are added with a single restart.
- **Backups:** before every write, `shortcuts.vdf` is copied to `%APPDATA%\SteamDrop\backups\<account>\shortcuts-<unix time>.vdf`. The newest 30 per account are kept. Writes go to a temporary file and are then renamed, so a crash can't leave a half-written file.
- **`LastPlayTime` is set to now** so new games appear first under Recent.
- **Re-adding a folder** under a different name or exe removes the old shortcut in the same step, so there are no duplicates.

## Watching folders

- **Event-based only:** `notify` uses Windows' `ReadDirectoryChangesW`, one recursive watch per watched folder. Timers exist only while a folder has recent changes; nothing is scanned periodically.
- **"Copying has finished"** means no change in that game folder for 12 seconds, and no recently changed file in it is still open (an exclusive open succeeds). If a file is still open, SteamDrop checks again 12 seconds later.
- **Folders that appeared while SteamDrop was closed** are picked up by a one-time scan when watching starts. SteamDrop remembers the folders it has handled ("known"), so this never repeats work.
- **Adding a watched folder that already has folders in it** asks whether to add those games too or only new ones. Otherwise a big existing library would either be ignored silently or added all at once without asking.
- **Empty new folders are ignored** until files arrive (for example a "New folder" that is about to be renamed).
- **Manual mode:** the SteamDrop window opens with a "New game found" card where the user can change the name and the exe. **Not now** means "don't ask about this folder again". An "Only an installer found" card appears in the same place.

## Picking the exe

- **Score** = name similarity to the cleaned folder name (0 to 50 points: exact match, initials like `GTA5` for "Grand Theft Auto V", containment, then bigram overlap), plus file size (log scale, -10 to +16), minus 12 points per folder level, minus 6 for "launcher"/"config". Deeper and tinier files lose. For Unreal games this picks the small top-level starter exe, which is what Steam does too.
- **Excluded by name:** uninstallers, setup/installers, crash handlers and reporters, redistributables (VC++, DirectX, .NET, PhysX, OpenAL), anti-cheat installers, updaters, web helpers, and similar. **Excluded folders:** `_CommonRedist`, `redist`, `__Installer`, `DirectX`, `Support`, `tools` and similar. The scan goes 4 levels deep and stops after 20,000 entries.
- Plain words like "crash" are not excluded on their own, so a game named "Crash ..." isn't skipped.
- **Folder name cleaning** drops `[tags]`, `(tags)`, version numbers (`v1.2.3`, `build 1234`) and underscores. Dots are kept unless a number follows, so "S.T.A.L.K.E.R." survives.
- **Installer only:** a `setup*.exe`, `*install*.exe` or `.msi` near the top of the folder, and no game exe candidates at all.

## Artwork

- **Order per slot:** SteamDrop's cache → the game folder's `SteamDrop art` folder (if present) → official Steam art → SteamGridDB → title card. Each slot falls back on its own, so one missing official image doesn't throw away the others.
- **Finding the Steam App ID:** `steam_appid.txt` anywhere in the top 3 levels of the folder (emulators often keep it in `steam_settings`). **Added:** if there is none, an *exact* title match in Steam's public store search. Many DRM-free games have a Steam page but no `steam_appid.txt`, and official art is the cleanest. Only exact matches count, to avoid wrong art. The user can enter or correct the App ID in the details window, which re-fetches the art.
- **Steam CDN files:** `library_600x900_2x.jpg` (falling back to `library_600x900.jpg`), `header.jpg` for the wide slot (the same shape as 920×430), `library_hero.jpg`, `logo.png`. Both `shared.steamstatic.com/store_item_assets` and `cdn.cloudflare.steamstatic.com` are tried. Files under 1 KB or that aren't PNG/JPEG are rejected (placeholders).
- **SteamGridDB:** static images only, with NSFW, humor and epilepsy-warning images filtered out. Searching by title only counts when the best match's name is close (similarity ≥ 0.6). With a Steam App ID, SteamDrop asks for that game directly.
- **Icon fallback:** when no real icon is found, the shortcut's icon points at the game's exe, so Steam shows the game's own icon. That looks better than a text square.
- **Title cards** are drawn in Rust with the bundled Chakra Petch font: a dark gradient and a thin accent stripe, with the title word-wrapped at the largest size that fits. The hero gets no text (Steam lays the logo over it), and the logo is text on a transparent background with a soft shadow, so it reads well over any background.
- **Grid files** are `<id>p`, `<id>`, `<id>_hero`, `<id>_logo` and `<id>_icon`, with the image's real extension. Old files with other extensions are removed first.
- **No game artwork is in the repository.** The app icon is an original drop-and-arrow design made for SteamDrop, with no Steam logo.

## App behavior

- **Data lives in `%APPDATA%\SteamDrop`** (`data.json`, `art`, `info`, `backups`), as specified, rather than Tauri's per-identifier folder. Only the WebView's own cache uses the identifier folder.
- **Automatic mode** isn't stored. It's derived from its three sub-settings: all on shows On, all off shows Off, a mix shows Custom. Pressing it while it shows Custom turns everything on. "Show notifications" is part of it, so it is off by default too. In manual mode the window opens itself instead.
- **Tray:** closing the window destroys it, freeing the WebView's memory, and the app keeps running. It's recreated from the tray or when a new game needs confirming. With "Keep running in the tray" off, closing quits.
- **Run on startup** writes `HKCU\...\Run\SteamDrop = "SteamDrop.exe" --tray`. That is the value Tauri's uninstaller already removes, so no autostart plugin is needed.
- **Single instance:** starting SteamDrop again just shows the existing window.
- **Store details** (description, developer, publisher, genres, release date) come from `store.steampowered.com/api/appdetails`, fetched on first open and cached in `info/<appid>.json`.
- **Wording** is aimed at non-technical users: "Game file" instead of "executable", "Not now" instead of "Ignore", and every Steam restart is explained before it happens.
- **Look:** dark by default, with a light option in Settings and the sun/moon button in the top bar. Base text is 18 px and buttons are at least 44 to 52 px tall.

## Installer, uninstaller, releases

- **Custom NSIS template** (`src-tauri/windows/installer.nsi`, copied from tauri-bundler 2.10.4). Tauri's hooks can't add uninstaller checkboxes, so the confirm page was changed to show two, both unchecked:
  1. "Remove SteamDrop's games from Steam (and their art)", with the note that Steam then hides their playtime. This runs `SteamDrop.exe --uninstall-cleanup` *before* the program files are deleted, since SteamDrop is what knows which shortcuts it added. The cleanup closes and restarts Steam if it's running, and backs up `shortcuts.vdf` first, like every change.
  2. "Delete SteamDrop settings and cache" deletes `%APPDATA%\SteamDrop` plus Tauri's own data folders.
- **Per-user install** (no admin prompt), English only, WebView2 bootstrapper downloaded if missing.
- **Release asset is renamed to `SteamDrop-Setup.exe`** and `latest.json` is written by the workflow, so the README's download button can point at the fixed URL `releases/latest/download/SteamDrop-Setup.exe`. The updater installs in passive mode (a progress bar, no questions).
- **The updater public key is a placeholder** until the owner makes a key (see `docs/RELEASING.md`). The release workflow refuses to run with the placeholder, without the secret, or when the tag doesn't match the version.
- **A build check runs on every push**, producing an unsigned installer as a workflow artifact for testing. Builds without the signing key use `src-tauri/tauri.no-updater.conf.json`, which turns off the updater files. It's a file rather than inline JSON on the command line because PowerShell mangles the quotes (that broke the first CI build).
- **Semantic Versioning, starting at 0.1.0.** The version is written in `package.json`, `tauri.conf.json` and `Cargo.toml` (the user asked for all three), and `scripts/version.mjs` keeps them and both lock files in sync. A small Node script with no dependencies is less error-prone than editing five files by hand. Every build checks that they agree.
- **Release notes come from `CHANGELOG.md`** (Keep a Changelog format). The release workflow refuses a tag whose version has no changelog section, and uses that section both on the GitHub release page and in `latest.json`. `bump` turns "Unreleased" into the new version's section and updates the compare links at the bottom.
- **The app reads its version from Tauri's package info**, which comes from `tauri.conf.json`. It's shown in About and in the tray tooltip ("SteamDrop 0.1.0").

## Secrets, logs and bug reports

- **SteamGridDB key in Windows Credential Manager** via the `keyring` crate (Windows backend only, with no extra C code). Using a well-tested crate is safer than hand-written Credential Manager calls. The key never goes into `data.json`, the logs or bug reports. The bug report also scrubs the key's text, in case it ever ended up in a log line. No migration from the settings file was written, because no released version ever stored the key there.
- **The key box loads the saved key** so it can show dots and reveal it while the eye is held. Reveal works only while held: pointer capture keeps it shown while the mouse or finger moves off the button, and Space/Enter key-down shows it until key-up. Losing focus always hides it.
- **The uninstaller's "Delete SteamDrop settings and cache" also forgets the key** (`SteamDrop.exe --forget-secrets`), since Credential Manager isn't a folder it can delete.
- **Own small logger** (`log.rs`) instead of a logging framework. It writes one plain-language line per event with a local timestamp (the `time` crate is already used by Tauri), rotates at 1 MB, and keeps 5 files. The log records what users and helpers care about: folders found or skipped and why, the exe and App ID chosen, where each picture came from, Steam restarts, settings changes, and problems marked "Problem:".
- **Bug report** is Markdown so it renders nicely in a GitHub issue. Every `\Users\<name>` (any drive, either slash) becomes `\Users\<user>`. Settings are listed with any key, token or password fields dropped. It's copied with the browser clipboard; if that's blocked, the report is shown in a box to copy by hand.

## Left out on purpose

- Removing a shortcut automatically when its game folder is deleted. The game shows "Game file missing" instead, because deleting things from Steam unasked felt wrong.
- Launch options, Steam collections/tags, and per-game controller settings. Steam already handles these for non-Steam games.
- Translations. Can be added later; the text lives in `ui/app.js` and the installer template.
