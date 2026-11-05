# Changelog

All notable changes to SteamDrop are listed here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and SteamDrop uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-05

First release.

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
- Installer option "Add SteamDrop to Steam" (on by default) and a matching setting.
- Opened from Steam, SteamDrop picks its view: Couch mode in Big Picture or on a Steam Deck, the desktop window in normal Steam. A setting ("Auto", "Always Couch mode", "Always window"), a switch button and F11 change it anytime. Steam shows SteamDrop as running while either view is open.
- A control bar in both views: Scan now (Y on a controller), Hide / Back to Steam, Close (keeps watching from the tray) and Quit SteamDrop (asks first), with controller button hints in Couch mode.
- "Add games to": all Steam accounts on the PC (the default) or only the ones you tick, shown with their Steam names. Removing a game, and the uninstaller, cover every account.
- A short chime when a game is added, heard even over Big Picture. On by default, with a volume slider and a "Test" button. The sound is synthesized by SteamDrop itself.
- Leaving Couch mode after changes asks "Restart Steam to apply changes?". SteamDrop then restarts Steam once Couch mode has closed (back into Big Picture) and writes the changes while Steam is closed, so Steam can't bring back a removed game.

[Unreleased]: https://github.com/Aster8x/SteamDrop/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Aster8x/SteamDrop/releases/tag/v0.1.0
