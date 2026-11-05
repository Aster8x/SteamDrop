# Status

Version 0.1.0, first complete build. Everything in the spec is implemented. It has not yet run on a real Windows PC with Steam. The checklist below covers what needs testing there.

## Works and checked

- **Unit tests** pass on Linux (18) and as real Windows binaries (19, cross-compiled and run under Wine; one test is Windows-only). They cover:
  - reading and writing `shortcuts.vdf`, including keeping other tools' shortcuts byte-for-byte, refusing broken files, and the backups;
  - exe picking: junk exclusion, Unreal games, initials such as `GTA5`, cleaning folder names;
  - spotting folders that hold only an installer, and reading `steam_appid.txt`;
  - title-card rendering and installing art into a grid folder and removing it again;
  - detecting files that are still open, using the Windows share-mode check;
  - loading old or partial settings files.
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
4. **Manual add (Automatic mode off):**
   - Copy a game folder in. It should be a known DRM-free game whose name matches a Steam game, for example a GOG copy.
   - About 15 seconds after the copy *finishes*, the SteamDrop window should pop up with "New game found".
   - Check that the right game file is preselected and the name is clean.
   - Press "Add to Steam". With Steam open, you should be asked to restart Steam. Choose "Restart Steam now".
   - Steam should close, reopen in Big Picture, and show the game first in Recent, with cover, banner, background, logo and icon.
5. **Large copy:** copy a big game (10 GB or more). Nothing should happen until the copy has completely finished.
6. **Game with `steam_appid.txt`:** official Steam art should appear.
7. **Game not on Steam:** with no SteamGridDB key, you should get clean title cards. Add a SteamGridDB key in Settings, press "Check key" (it should say the key works), then the game's Details → "Find artwork again".
8. **Installer only:** copy a folder containing only `setup_something.exe` (plus `.bin` files). You should get an "Only an installer found" card or notification, and nothing should be added to Steam.
9. **Wrong exe:** use "Choose a different file..." on a new-game card and check that Steam launches the file you chose.
10. **Details:** open a game's Details. You should see a description, developer and genres, and "Open store page" should open the browser.
11. **Remove:** "Remove" on a game removes it from Steam after the restart. The game's folder must be untouched.
12. **Automatic mode:** turn it on (it should show "On"), then switch one sub-setting off (it should show "Custom"). With all three on, copying a game in should add it, restart Steam and show a notification without any questions.
13. **Tray:** close the window. SteamDrop should stay in the tray, and Task Manager should show no WebView2 processes for it. Click the tray icon and the window should come back. Turn "Keep running in the tray" off, and closing should quit.
14. **Run on startup:** turn it on, sign out and back in. SteamDrop should start in the tray with no window.
15. **While closed:** quit SteamDrop, copy a game into the watched folder, and start SteamDrop. It should find the game.
16. **Several Steam accounts** (if you have them): the game should show up for each.
17. **Uninstall,** three times:
    - with both boxes unchecked: games stay in Steam;
    - with "Remove SteamDrop's games from Steam" checked: they're gone from Steam after it restarts;
    - with "Delete SteamDrop settings and cache" checked: `%APPDATA%\SteamDrop` is gone.

    Also check that the two checkboxes' text fits and isn't cut off.
18. **Release and update:** after setting up the signing key (see `docs/RELEASING.md`), tag `v0.1.0`, check that the release has `SteamDrop-Setup.exe` and the README button downloads it. Then release `v0.1.1` and use About → "Check for updates" → "Install update".

If something goes wrong, the backups are in `%APPDATA%\SteamDrop\backups`.

## Known limits

- If you decline a Steam restart and then play another non-Steam game before Steam restarts, Steam may overwrite the change. The game then shows "Not in Steam" in SteamDrop, with "Add to Steam again" in its details.
- Some newer Steam games store their library art under hashed URLs that SteamDrop can't guess. Those slots fall back to SteamGridDB or a title card.
- Exe picking is a best guess. Games with unusual layouts may need "Choose a different file..." (with Automatic mode off).
