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

[Unreleased]: https://github.com/Aster8x/SteamDrop/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Aster8x/SteamDrop/releases/tag/v0.1.0
