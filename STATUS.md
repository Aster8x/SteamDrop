# Status

Version 0.1.0, first complete build. Everything in the spec is implemented. It has not yet run on a real Windows PC with Steam. The checklist below covers what needs testing there.

## Works and checked

- **Unit tests** pass on Linux (23) and as real Windows binaries (26, cross-compiled and run under Wine; three tests are Windows-only). They run on every push in GitHub Actions on Windows too. They cover:
  - reading and writing `shortcuts.vdf`, including keeping other tools' shortcuts byte-for-byte, refusing broken files, and the backups;
  - exe picking: junk exclusion, Unreal games, initials such as `GTA5`, cleaning folder names;
  - spotting folders that hold only an installer, and reading `steam_appid.txt`;
  - title-card rendering and installing art into a grid folder and removing it again;
  - detecting files that are still open, using the Windows share-mode check;
  - loading old or partial settings files;
  - log rotation (5 files of 1 MB);
  - hiding user names in bug reports;
  - saving the SteamGridDB key in Credential Manager and keeping it out of the bug report (run under Wine's Credential Manager);
  - reading the changelog and picking the versions between two releases;
  - sending a folder to the Recycle Bin, or refusing without touching it (Wine has no Recycle Bin, so it refused; GitHub's Windows runner exercises the real path).
- **SteamDrop's own Steam shortcut:** `--add-to-steam` and `--remove-from-steam` were run as a Windows binary under Wine against a fake Steam folder. They added and removed the shortcut (launch options `--couch`) and all five artwork files.
- **The process Steam starts** (`--couch`) exits by itself when Couch mode can't open; it doesn't hang.
- **Couch mode** was driven in Chromium with a simulated gamepad and the keyboard: moving around, A, B, LB/RB, holding A (1 second does nothing, 2 seconds deletes), and the PIN pad.
- **Windows installer builds:** a cross-compiled build produced a ~2 MB `SteamDrop_0.1.0_x64-setup.exe` with the custom uninstaller template, so the NSIS script compiles.
- **`SteamDrop.exe --uninstall-cleanup`** was run as a Windows binary under Wine against a fake Steam folder. It removed SteamDrop's shortcut and its grid art, kept another tool's shortcut, made a backup, and cleared the art cache and the game list.
- **The UI** was rendered in Chromium against a mocked backend (games list, new-game card, installer card, details window, settings, about, dark and light). The layout looks right.
- **Title cards** (portrait, wide, hero, logo) were rendered and checked by eye.

## Not tested yet (couldn't be, from the build machine)

- Anything with a **real Steam install**: detecting Steam, closing and restarting it, Big Picture start, Steam picking up the shortcut and art, Recent ordering, the icon.
- **Network:** the Steam store, Steam CDN and SteamGridDB were unreachable from the build machine, so live artwork downloads, store search and store details are untested. The code paths fall back to title cards when downloads fail.
- **The real app window** (WebView2), the tray icon and menu, notifications, single instance, and run on startup.
- **Folder watching** on a real NTFS drive with Explorer copying a large game.
- **The NSIS installer and uninstaller UI**, including the two checkboxes' layout at different display scales.
- **The official MSVC build** (the build checked here used the GNU toolchain). The GitHub Actions build check covers the MSVC build.
- **The updater** (needs a signing key and a published release).

## What to test on Windows

Before you start, back up `C:\Program Files (x86)\Steam\userdata\<your number>\config\shortcuts.vdf`. SteamDrop backs it up too, but a manual copy is safest for a first run.

1. **Install:** run the installer from the Build workflow artifact (or a release). Check that it installs without admin rights, that a Start menu entry exists, and that SteamDrop opens.
2. **Steam found:** Settings → Steam should show your Steam folder and "Steam accounts on this PC: 1" (or more).
3. **Watch a folder:** Games → "Choose a folder to watch" → pick a test folder, such as an empty `D:\TestGames`.
4. **Starting fresh:** watch a folder that already holds 2–3 game folders. You're asked "What about games already in this folder?" with "Only new games from now on" selected; pressing Enter keeps them out of Steam. Then Settings → Watched folders → "Add existing games now" offers them.
5. **Folder rules** (the README example): watch `D:\Games` and `D:\Games\Torrent`, and exclude `D:\Games\Torrent\X`.
    - Copy a game into `D:\Games\Torrent\X\Downloads`: nothing happens.
    - Move it (drag within the same drive) to `D:\Games\Torrent\Celeste`: it's found about 15 seconds later, just like a copy.
    - `D:\Games\Torrent` itself is never offered as a game.
    - A game whose .exe is already a Steam shortcut (for example one you added by hand in Steam) is skipped; the Activity list says why.
6. **Moved out:** move an added game's folder out of the watched folders. It appears under "Missing games", and nothing changes in Steam. "Find it" with the new folder updates the Steam shortcut; "Remove from Steam" removes it.
7. **Manual add (Automatic mode off):**
   - Copy a game folder in. It should be a known DRM-free game whose name matches a Steam game, for example a GOG copy.
   - About 15 seconds after the copy *finishes*, the SteamDrop window should pop up with "New game found".
   - Check that the right game file is preselected and the name is clean.
   - Press "Add to Steam". With Steam open, you should be asked to restart Steam. Choose "Restart Steam now".
   - Steam should close, reopen in Big Picture, and show the game first in Recent, with cover, banner, background, logo and icon.
8. **Large copy:** copy a big game (10 GB or more). Nothing should happen until the copy has completely finished.
9. **Game with `steam_appid.txt`:** official Steam art should appear.
10. **Game not on Steam:** with no SteamGridDB key, you should get clean title cards. Add a SteamGridDB key in Settings, press "Check key" (it should say the key works), then the game's Details → "Find artwork again".
11. **Installer only:** copy a folder containing only `setup_something.exe` (plus `.bin` files). You should get an "Only an installer found" card or notification, and nothing should be added to Steam.
12. **Wrong exe:** use "Choose a different file..." on a new-game card and check that Steam launches the file you chose.
13. **Details:** open a game's Details. You should see a description, developer and genres, and "Open store page" should open the browser.
14. **Remove:** "Remove" on a game removes it from Steam after the restart. The game's folder must be untouched.
15. **Automatic mode:** turn it on (it should show "On"), then switch one sub-setting off (it should show "Custom"). With all three on, copying a game in should add it, restart Steam and show a notification without any questions.
16. **Tray:** close the window. SteamDrop should stay in the tray, and Task Manager should show no WebView2 processes for it. Click the tray icon and the window should come back. Turn "Keep running in the tray" off, and closing should quit.
17. **Run on startup:** turn it on, sign out and back in. SteamDrop should start in the tray with no window.
18. **While closed:** quit SteamDrop, copy a game into the watched folder, and start SteamDrop. It should find the game.
19. **Several Steam accounts** (if you have them): by default a new game shows up for each. In Settings → Steam, "Add games to" lists every account by its Steam name; choose "Only these accounts", untick one, add a game, and check it only appears for the ticked accounts. Removing it must remove it from all of them.
20. **Uninstall,** three times:
    - with both boxes unchecked: games stay in Steam;
    - with "Remove SteamDrop's games from Steam" checked: they're gone from Steam after it restarts;
    - with "Delete SteamDrop settings and cache" checked: `%APPDATA%\SteamDrop` is gone.

    Also check that the two checkboxes' text fits and isn't cut off.
21. **Version shown:** About says "Version 0.1.0" and hovering the tray icon says "SteamDrop 0.1.0".
22. **Release and update:** publish the first release as described in "Your first release: v0.1.0" below (just a tag, no bump). Check that the release page shows the 0.1.0 notes from CHANGELOG.md and has `SteamDrop-Setup.exe`, and that the README button downloads it. Then publish `v0.1.1`, and in the installed 0.1.0 use About → "Check for updates" → "Install update".

23. **SteamGridDB key box:**
    - The key shows as dots.
    - Holding the eye button (mouse, touch, or Space/Enter on the keyboard) shows it, and letting go hides it again.
    - "Get a free key" opens the SteamGridDB API page.
    - "Test key" says clearly whether the key works. Try a wrong key too.
    - After "Save key", the key is in Windows Credential Manager (Control Panel → Credential Manager → Windows Credentials, look for "SteamDrop") and not in `%APPDATA%\SteamDrop\data.json`.
24. **Help and bug report:**
    - Settings → Help shows recent activity in plain words (for example "Added "X" to Steam").
    - "Open log folder" opens `%APPDATA%\SteamDrop\logs`.
    - "Copy bug report" copies one block of text. Paste it into Notepad and check it shows `C:\Users\<user>` instead of your name and contains no key.
    - "Report a problem" opens GitHub's bug report form.
25. **Uninstall with "Delete SteamDrop settings and cache"** also removes the saved key from Credential Manager.
26. **Updates** (needs two published releases, for example 0.1.0 installed and 0.1.1 released):
    - Settings → Updates shows your version. "Check for updates on startup" and "Also check once a day" are on, "Install updates automatically" is off.
    - Start SteamDrop: after about 20 seconds you get a Windows notification and the update window, showing "Current: 0.1.0 → New: 0.1.1" and the changes for every version in between.
    - "Remind me later" closes it; it comes back on the next check. "Skip this version" stops it for that version (but "Check now" still shows it).
    - "Update now" downloads, installs and reopens SteamDrop, which then shows "What's new in 0.1.1" once.
    - Turn on "Install updates automatically", start a game from Steam, and trigger a check. The update must wait until the game is closed, then install, restart SteamDrop and show a notification.
    - Unplug the network and press "Check now": you get a short message, and the Activity list says the check failed. No error pop-ups.
27. **Couch mode setup:**
    - On the installer's last page, "Show SteamDrop in my Steam library" is ticked.
    - After installing and restarting Steam, SteamDrop is in the library with its own cover, banner, background, logo and icon.
    - Settings → Couch mode can turn this off and on again.
28. **Couch mode with a controller** (try an Xbox and a PlayStation controller if you have both):
    - Start SteamDrop from Steam in Big Picture. It opens full screen, and Steam shows SteamDrop as running.
    - D-pad and left stick move between tiles, A opens a game, B goes back, and LB/RB switch Library, Settings and Activity.
    - B on the main screen asks to leave; leaving closes Couch mode, and Steam shows SteamDrop as stopped.
    - Start Couch mode while SteamDrop is already in the tray: it still opens, and you don't get two SteamDrops.
29. **Game details in Couch mode:** "Find artwork again", "Change game file" (pick another .exe), and "Remove from Steam" all work; Steam's library changes after a Steam restart.
30. **Leaving Couch mode after a change:** remove a game in Couch mode, then leave. You're asked "Restart Steam to apply changes?". "Restart now" closes Couch mode, restarts Steam back into Big Picture, and the game is gone for good: it must not come back after another Steam restart. "Later" just leaves.
31. **Opening from Steam:**
    - Start SteamDrop from desktop Steam: the normal window opens. Start it from Big Picture (or a Steam Deck): Couch mode opens.
    - Change "When opened from Steam" to "Always Couch mode" or "Always window" and check it's respected.
    - F11 or the switch button changes views; Steam keeps showing SteamDrop as running the whole time, and as stopped only after the last window closes.
32. **Control bar:** Scan now finds a game you copied while SteamDrop was paused (Y on a controller). In the desktop window, Hide sends the window to the tray, Close closes it but the tray icon stays, and Quit SteamDrop asks "Stop adding new games?" and then exits. Couch mode's bar has only Scan now (Y), Window view (View) and Back to Steam (Menu); leaving and quitting are behind B (see test 38).
33. **Big Picture at sign-in:** turn on Settings → App → "Open Steam in Big Picture when Windows starts", sign out and back in: Steam opens in Big Picture. With Steam already set to start in Big Picture, nothing extra happens. Turn it off and sign in again: Steam starts as usual. Uninstalling removes it.
34. **Chime:** Settings → App → "Play a sound when a game is added" is on. "Test" plays a short three-note chime at the slider's volume. Add a game while Big Picture is open: you hear the chime. Turn it off: no sound.
35. **Delete game:**
    - Use a test game you don't need. Holding A for under 2 seconds does nothing; holding it for 2 seconds moves the folder to the Recycle Bin (check that you can restore it) and removes the game.
    - Set a PIN in Couch mode's Settings: deleting now asks for it, and a wrong PIN refuses.
    - On a USB stick or network drive without a Recycle Bin, SteamDrop must refuse and leave the folder alone.
36. **Uninstall** removes SteamDrop's own shortcut from Steam (always), and the PIN with "Delete SteamDrop settings and cache".

37. **Fixes from the first laptop test** (install the new build over the old one; your games and settings stay):
    - **Settings survive restarts:** set up watched folders, an exclusion, Automatic mode and a few other settings, and add a game. Then quit and restart SteamDrop, restart Windows, open SteamDrop from Steam (both views), let SteamDrop restart Steam, and install the new build over the old one. After each, everything is still there. `%APPDATA%\SteamDrop` has `data.json` and `data.json.bak`.
    - SteamDrop's own Steam tile and banner show only the logo and "SteamDrop" (no "COUCH MODE").
    - Game covers show in both views, including Couch mode's Library and a game's details. A game without art shows a title card with its name, never a broken-image icon.
    - About → "Check for updates" with no release published says "You're on the latest version." With Wi-Fi off it says to check the internet connection.
    - With Steam in the normal desktop window, add a game and let SteamDrop restart Steam: Steam comes back in the desktop window. Do the same from Big Picture: it comes back in Big Picture. The Activity list says which mode it chose and why. Try "Always Big Picture" and "Always desktop mode" too.
    - In Couch mode (started from Big Picture), press "Window view": Couch mode closes and the desktop window opens with its content (no white window). F11 or View goes back. Steam keeps showing SteamDrop as running the whole time.
    - Add a game from the desktop window, then open Couch mode: the game is in its Library. Also the other way around.
    - Turn "Show SteamDrop in my Steam library" off with Steam open, then restart Steam from SteamDrop: the SteamDrop tile is gone from every account, your games and their art are still there, and SteamDrop still opens from the tray and Start menu. Turn it on again: the tile comes back with its art.
    - In the desktop window with a controller: D-pad/stick move a clearly visible focus ring, A selects, B closes dialogs, LB/RB switch tabs, Y scans, View goes back to Couch mode.

38. **Fixes from the second laptop test** (install the new build over the old one):
    - **Resume:** open SteamDrop from desktop Steam (the window opens), switch Steam to Big Picture, then press SteamDrop's "Resume" in Big Picture. SteamDrop comes to the front as full-screen Couch mode, and there is only one SteamDrop view (the desktop window was switched, not doubled). Also: just switching Steam to Big Picture while the SteamDrop window from Steam is open should switch it to Couch mode by itself after a moment.
    - **Couch mode B:** on the Library, B opens "Leave SteamDrop?" with "Back to Steam" selected. A on it goes back to Big Picture with SteamDrop still running. "Quit SteamDrop" shows the "Stop adding new games?" warning first. "Cancel" stays. On Settings or Activity, B goes back to the Library. The control bar has no Close or Quit, and no two hints share a button.
    - **Settings layout:** look through Settings in a small window and at 125% and 150% Windows scaling (Settings → System → Display → Scale), on a 1366x768 laptop if you have one. Nothing is cut off and there's no sideways scrolling; where space is tight, the switch or buttons sit under their description. The window never opens bigger than the screen.
    - **Batches:** delete two game folders (from a watched folder) a few seconds apart. One question appears: "2 game folders were deleted. Remove them from Steam?" with a tick for each. Untick one, press "Remove selected": only the ticked game goes, with one Steam restart. "Keep in Steam" leaves them under Missing games. Moving a game folder to another place inside the watched folders is not reported as deleted.
    - Copy three game folders in at once (Automatic mode off): one question "3 new games found. Add them to Steam?". Untick one and press "Add selected": two games are added with one Steam restart, and the third stays as a card on the Games page. With Automatic mode on, all three are added with one restart. The same questions show in Couch mode (A ticks, B leaves it for later).
    - **Faster Steam restarts:** add a game with Steam open and let SteamDrop restart it. The Activity list has a line like "Steam restart: closing Steam took 2.1 s, writing changes 0.1 s, starting it 0.0 s". Steam opens right after it has closed (the old fixed 2-second wait is gone).
    - **Store game in "Recent":** see the README FAQ "A game shows up in Steam as the store version". SteamDrop's own shortcut for such a game still starts it.

39. **Interface size:** in the desktop window, Ctrl and plus makes everything bigger in steps of 10%, Ctrl and minus smaller, Ctrl and 0 back to 100%; Ctrl and the mouse wheel does the same. It stops at 50% and 200%. Settings → App → "Interface size" shows the size, with −, + and Reset. In Couch mode, LT and RT change the size and Settings → "Interface size" resets it. Each view keeps its own size after closing and reopening.

If something goes wrong, the backups are in `%APPDATA%\SteamDrop\backups`. To report a problem, use **Settings → Help → Copy bug report** and paste it into a [bug report on GitHub](https://github.com/Aster8x/SteamDrop/issues/new?template=bug_report.yml).

## How to publish a new version

SteamDrop uses [Semantic Versioning](https://semver.org/): `MAJOR.MINOR.PATCH`. Bump PATCH for fixes (0.1.0 → 0.1.1), MINOR for new features (0.1.1 → 0.2.0), and MAJOR only for big breaking changes. The version lives in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (plus their lock files). The bump script keeps all of them in sync, so never edit them by hand.

One-time setup before the very first release: the updater signing key (see `docs/RELEASING.md`, "One-time setup").

### Your first release: v0.1.0

The version is already 0.1.0 everywhere, and `CHANGELOG.md` already has the `## [0.1.0]` section with everything in it. So the first release needs **no bump**, only the tag:

```bash
git checkout main
```

```bash
git pull
```

```bash
node scripts/version.mjs check v0.1.0
```

```bash
git tag v0.1.0
```

```bash
git push origin v0.1.0
```

Then wait for the "Release" workflow (step 7 below). Use the full steps below for every release after that (0.1.1, 0.2.0...).

### Every later release

Every release, from the project folder on `main`:

1. **Get up to date.**

   ```bash
   git checkout main
   ```

   ```bash
   git pull
   ```

2. **Write the changelog.** While you work, list changes in `CHANGELOG.md` under `## [Unreleased]`, in groups like `### Added`, `### Changed` and `### Fixed`. This text becomes the release notes and the update notes.

3. **Bump the version** (example: 0.2.0). This updates every version file and turns the "Unreleased" notes into a `## [0.2.0] - <today>` section:

   ```bash
   npm run bump -- 0.2.0
   ```

   Open `CHANGELOG.md` and check the new section reads well. If the script warns that the section is empty, write the notes under `## [0.2.0]` now.

4. **Check** that everything agrees and the notes exist:

   ```bash
   node scripts/version.mjs check v0.2.0
   ```

5. **Commit and push** the bump:

   ```bash
   git commit -am "Release 0.2.0"
   ```

   ```bash
   git push
   ```

6. **Tag and push the tag.** This starts the release:

   ```bash
   git tag v0.2.0
   ```

   ```bash
   git push origin v0.2.0
   ```

7. **Wait for the "Release" workflow** on GitHub (Actions tab, about 10 to 15 minutes). It publishes the release with the changelog section as its notes, plus `SteamDrop-Setup.exe`, `SteamDrop-Setup.exe.sig` and `latest.json`. Installed copies see the update the next time someone presses "Check for updates" in About.

If the workflow stops at "Check release setup", it says why: a version mismatch, missing changelog notes, or a missing signing key. To fix it, delete the tag, fix the problem, commit, and tag again:

```bash
git tag -d v0.2.0
```

```bash
git push origin :refs/tags/v0.2.0
```

## Known limits

- If you decline a Steam restart and then play another non-Steam game before Steam restarts, Steam may overwrite the change. The game then shows "Not in Steam" in SteamDrop, with "Add to Steam again" in its details.
- Some newer Steam games store their library art under hashed URLs that SteamDrop can't guess. Those slots fall back to SteamGridDB or a title card.
- Changes made while Steam is running (Couch mode, or "Add without restarting") show after a Steam restart. Leaving Couch mode offers "Restart now". If you choose "Later" and restart Steam yourself, Steam may show its old list until SteamDrop next starts while Steam is closed (or does a restart), which writes the changes again.
- Games added by SteamDrop don't appear in Steam's "Recent games" until they've been played once. SteamDrop writes Steam's own last-played time in `shortcuts.vdf`, but Steam decides the recent list from its own live data and there's no reliable file to write (see DECISIONS.md).
- Exe picking is a best guess. Games with unusual layouts may need "Choose a different file..." (with Automatic mode off).
