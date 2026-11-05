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
18. **Version shown:** About says "Version 0.1.0" and hovering the tray icon says "SteamDrop 0.1.0".
19. **Release and update:** publish `v0.1.0` as described in "How to publish a new version" below. Check that the release page shows the 0.1.0 notes from CHANGELOG.md and has `SteamDrop-Setup.exe`, and that the README button downloads it. Then publish `v0.1.1`, and in the installed 0.1.0 use About → "Check for updates" → "Install update".

20. **SteamGridDB key box:**
    - The key shows as dots.
    - Holding the eye button (mouse, touch, or Space/Enter on the keyboard) shows it, and letting go hides it again.
    - "Get a free key" opens the SteamGridDB API page.
    - "Test key" says clearly whether the key works. Try a wrong key too.
    - After "Save key", the key is in Windows Credential Manager (Control Panel → Credential Manager → Windows Credentials, look for "SteamDrop") and not in `%APPDATA%\SteamDrop\data.json`.
21. **Help and bug report:**
    - Settings → Help shows recent activity in plain words (for example "Added "X" to Steam").
    - "Open log folder" opens `%APPDATA%\SteamDrop\logs`.
    - "Copy bug report" copies one block of text. Paste it into Notepad and check it shows `C:\Users\<user>` instead of your name and contains no key.
    - "Report a problem" opens GitHub's bug report form.
22. **Uninstall with "Delete SteamDrop settings and cache"** also removes the saved key from Credential Manager.
23. **Updates** (needs two published releases, for example 0.1.0 installed and 0.1.1 released):
    - Settings → Updates shows your version. "Check for updates on startup" and "Also check once a day" are on, "Install updates automatically" is off.
    - Start SteamDrop: after about 20 seconds you get a Windows notification and the update window, showing "Current: 0.1.0 → New: 0.1.1" and the changes for every version in between.
    - "Remind me later" closes it; it comes back on the next check. "Skip this version" stops it for that version (but "Check now" still shows it).
    - "Update now" downloads, installs and reopens SteamDrop, which then shows "What's new in 0.1.1" once.
    - Turn on "Install updates automatically", start a game from Steam, and trigger a check. The update must wait until the game is closed, then install, restart SteamDrop and show a notification.
    - Unplug the network and press "Check now": you get a short message, and the Activity list says the check failed. No error pop-ups.
24. **Couch mode setup:**
    - On the installer's last page, "Add SteamDrop to Steam (for Couch mode)" is ticked.
    - After installing and restarting Steam, SteamDrop is in the library with its own cover, banner, background, logo and icon.
    - Settings → Couch mode can turn this off and on again.
25. **Couch mode with a controller** (try an Xbox and a PlayStation controller if you have both):
    - Start SteamDrop from Steam in Big Picture. It opens full screen, and Steam shows SteamDrop as running.
    - D-pad and left stick move between tiles, A opens a game, B goes back, and LB/RB switch Library, Settings and Activity.
    - B on the main screen asks to leave; leaving closes Couch mode, and Steam shows SteamDrop as stopped.
    - Start Couch mode while SteamDrop is already in the tray: it still opens, and you don't get two SteamDrops.
26. **Game details in Couch mode:** "Find artwork again", "Change game file" (pick another .exe), and "Remove from Steam" all work; Steam's library changes after a Steam restart.
27. **Delete game:**
    - Use a test game you don't need. Holding A for under 2 seconds does nothing; holding it for 2 seconds moves the folder to the Recycle Bin (check that you can restore it) and removes the game.
    - Set a PIN in Couch mode's Settings: deleting now asks for it, and a wrong PIN refuses.
    - On a USB stick or network drive without a Recycle Bin, SteamDrop must refuse and leave the folder alone.
28. **Uninstall** removes SteamDrop's own shortcut from Steam (always), and the PIN with "Delete SteamDrop settings and cache".

If something goes wrong, the backups are in `%APPDATA%\SteamDrop\backups`. To report a problem, use **Settings → Help → Copy bug report** and paste it into a [bug report on GitHub](https://github.com/Aster8x/SteamDrop/issues/new?template=bug_report.yml).

## How to publish a new version

SteamDrop uses [Semantic Versioning](https://semver.org/): `MAJOR.MINOR.PATCH`. Bump PATCH for fixes (0.1.0 → 0.1.1), MINOR for new features (0.1.1 → 0.2.0), and MAJOR only for big breaking changes. The version lives in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (plus their lock files). The bump script keeps all of them in sync, so never edit them by hand.

One-time setup before the very first release: the updater signing key (see `docs/RELEASING.md`, "One-time setup").

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
- Changes made from Couch mode (remove, delete, change game file) are written while Steam is running, so they show after Steam restarts. Removals are repeated automatically the next time SteamDrop starts while Steam is closed, in case Steam wrote its old list back.
- Exe picking is a best guess. Games with unusual layouts may need "Choose a different file..." (with Automatic mode off).
