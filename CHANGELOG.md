# Changelog

All notable changes to SteamDrop are listed here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and SteamDrop uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-05

First release.

### Fixed during testing

- Settings, watched folders, exclusions and the games list could be lost after SteamDrop restarted: one failed read of the data file made SteamDrop start from defaults and save them over the real data, and two SteamDrop processes saving at once could break the file. Saves are now atomic per process, keep a `data.json.bak`, and an unreadable file is kept and recovered from the backup. Settings changes only update what changed.

- SteamDrop's own Steam artwork shows just the logo and "SteamDrop".
- Game art loads reliably in both views (served by SteamDrop itself instead of through a folder permission check), and a missing or broken picture shows a title card instead of a broken-image icon.
- "Check now" with no published release says "You're on the latest version." Only real connection problems mention the internet.
- After adding a game from desktop Steam, Steam no longer reopens in Big Picture.
- "Window view" in Couch mode opened a blank white window; switching views now really switches, and windows have a dark background while loading.
- Both views always list every game SteamDrop added, even games added while the view was closed or by another SteamDrop process.
- Steam's "Resume" for SteamDrop did nothing, because the process Steam tracks had no window. It now has a tiny invisible one that brings SteamDrop to the front (Couch mode while Big Picture is open, switching the window instead of opening a second view). SteamDrop opened from Steam also switches to Couch mode by itself when Steam goes into Big Picture.
- Couch mode's control bar no longer has "Close" and "Quit SteamDrop" (both showed A). B on the Library asks "Back to Steam", "Quit SteamDrop" or "Cancel"; on other screens B goes back to the Library.
- Settings fit small and scaled windows (800x600, 125% and 150%): a control moves under its description when there isn't room, and the window never opens bigger than the screen. Couch mode fits small screens too.
- Faster Steam restarts: no fixed waits, Steam is checked every quarter second with a fast process list, the changes are written and Steam is started again right away (even if writing failed), it's force-closed only after a minute, and the time of each step is logged.
- New games and deleted game folders that arrive close together are handled together: one question with a tick per game ("3 new games found. Add them to Steam?", "2 game folders were deleted. Remove them from Steam?") and one Steam restart. Changes made during a restart wait for one follow-up restart.
- A game that shows up in Steam as its store version (with a "Purchase" button) is explained in the README: the game itself tells Steam its App ID; SteamDrop never does.

### Added

- Watches the folders you choose and notices new game folders as soon as they have finished copying, without polling.
- Finds the file that starts the game, skipping uninstallers, crash reporters, redistributables, setup and helper files. In manual mode you confirm or change the pick.
- Adds games to Steam as non-Steam shortcuts for every Steam account on the PC, first in "Recent", and restarts Steam into Big Picture if it was running.
- Skips folders that only contain an installer and tells you why.
- Fills every Steam artwork slot (cover, wide banner, background, logo, icon): official Steam art, then SteamGridDB (with your free key), then a clean title card. Art is cached in `%APPDATA%\SteamDrop\art`, with an option to also keep it in the game folder.
- Game details from the Steam store (description, developer, genres) with an "Open store page" button.
- Automatic mode (off by default) with its own switches for adding without asking, restarting Steam and notifications.
- Backs up Steam's shortcuts file before every change. "Remove from Steam" never touches game files.
- Tray icon, run on startup, keep running in the tray when closed, Steam folder detection with manual override, dark and light looks.
- Uninstaller options to remove SteamDrop's games from Steam and to delete its settings and cache (both off by default).
- Automatic updates from GitHub releases.
- SteamGridDB key box in Settings → Artwork: shows dots, a hold-to-show eye button (mouse, touch, Space/Enter), "Get a free key" and "Test key" with clear messages. The key is stored in Windows Credential Manager, never in a settings file or the logs.
- Activity log in plain words, in `%APPDATA%\SteamDrop\logs` (last 5 files, up to 1 MB each).
- Settings → Help: recent activity, "Copy bug report" (version, Windows version, Steam status, settings and the last 200 log lines, with the Windows user name and secrets removed), "Report a problem" and "Open log folder".
- Bug report form on GitHub.
- Settings → Updates: check on startup (on), check once a day (on), install automatically (off), and "Check now".
- Update window with "Current → New", the changes in every version in between (newest first), and "Update now", "Remind me later" and "Skip this version".
- Automatic updates install quietly, but never while a game started from Steam is running.
- A one-time "What's new" screen after each update.
- Couch mode: SteamDrop adds itself to Steam with its own artwork and, started from Steam, opens a full-screen view for gamepads (Xbox and PlayStation) with Library, game details, Settings and Activity.
- Couch mode can find art again, change a game's file, remove a game from Steam, or delete it (hold A for 2 seconds; the folder goes to the Recycle Bin). An optional 4-digit PIN protects deleting.
- Installer option "Show SteamDrop in my Steam library" (on by default) and a matching setting. Turning it off removes only SteamDrop's own tile and its art, from every Steam account, while Steam is closed.
- Opened from Steam, SteamDrop picks its view: Couch mode in Big Picture or on a Steam Deck, the desktop window in normal Steam. A setting ("Auto", "Always Couch mode", "Always window"), a switch button and F11 change it anytime. Steam shows SteamDrop as running while either view is open.
- A control bar in both views: Scan now (Y on a controller), Hide / Back to Steam, Close (keeps watching from the tray) and Quit SteamDrop (asks first), with controller button hints in Couch mode.
- "Add games to": all Steam accounts on the PC (the default) or only the ones you tick, shown with their Steam names. Removing a game, and the uninstaller, cover every account.
- A short chime when a game is added, heard even over Big Picture. On by default, with a volume slider and a "Test" button. The sound is synthesized by SteamDrop itself.
- "Open Steam in Big Picture when Windows starts" (off by default): at sign-in, Steam opens in Big Picture unless it's already there.
- Starting fresh: adding a watched folder that already has games asks "What about games already in this folder?" ("Only new games from now on" by default), with "Add existing games now" for later.
- Folder rules: each direct subfolder of a watched folder is one game; a watched folder inside another is skipped by the outer one; per-folder "Exclude a subfolder" ignores a folder and everything in it; a game whose .exe already has a Steam shortcut is never added again.
- Folders moved or renamed into a watched folder are added like copies. Games moved out are never removed from Steam automatically: they show under "Missing games" with "Find it" and "Remove from Steam".
- "When SteamDrop restarts Steam": "Keep current mode" (default; Big Picture only if it was open right before), "Always Big Picture" or "Always desktop mode". Every restart uses it and logs which mode it chose and why.
- Full controller support in the desktop window too (same controls as Couch mode, with a clear focus ring; View goes back to Couch mode).
- Leaving Couch mode after changes asks "Restart Steam to apply changes?". SteamDrop then restarts Steam once Couch mode has closed (back into Big Picture) and writes the changes while Steam is closed, so Steam can't bring back a removed game.

[Unreleased]: https://github.com/Aster8x/SteamDrop/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Aster8x/SteamDrop/releases/tag/v0.1.0
