# Third-party credits

SteamDrop is MIT licensed. It is built on, bundles, or talks to the following.

## Software

- **[Tauri](https://tauri.app/)** (MIT or Apache-2.0): the app framework, plus the official plugins for dialogs, notifications, single instance and updates. The uninstaller script (`src-tauri/windows/installer.nsi`) is based on Tauri's NSIS template.
- **[notify](https://github.com/notify-rs/notify)** (CC0-1.0 / Artistic-2.0): folder change notifications.
- **[reqwest](https://github.com/seanmonstar/reqwest)** (MIT or Apache-2.0): downloads.
- **[ab_glyph](https://github.com/alexheretic/ab-glyph)** (Apache-2.0): drawing text on title cards.
- **[keyring](https://github.com/hwchen/keyring-rs)** (MIT or Apache-2.0): keeps the SteamGridDB key in Windows Credential Manager.
- **[time](https://github.com/time-rs/time)** (MIT or Apache-2.0): local timestamps in the activity log.
- **[png](https://github.com/image-rs/image-png)**, **[crc32fast](https://github.com/srijs/rust-crc32fast)**, **[serde](https://serde.rs/)**, **[winreg](https://github.com/gentoo90/winreg-rs)** (MIT or Apache-2.0).
- **[NSIS](https://nsis.sourceforge.io/)** (zlib license): the Windows installer.

The full list of Rust crates and their licenses is in `src-tauri/Cargo.lock`.

## Fonts (bundled)

- **Chakra Petch**, Copyright 2018 The Chakra Petch Project Authors. SIL Open Font License 1.1, see `ui/fonts/OFL-ChakraPetch.txt`.
- **Atkinson Hyperlegible**, Copyright 2020 Braille Institute of America, Inc. SIL Open Font License 1.1, see `ui/fonts/OFL-AtkinsonHyperlegible.txt`.

## Online services (used at runtime, nothing is bundled)

- **Steam store and CDN** (Valve Corporation): official game artwork and store details, fetched from public URLs when a game is added or its details are opened.
- **[SteamGridDB](https://www.steamgriddb.com/)**: community artwork, used only if you enter your own free API key. Artwork belongs to its creators and the game's owners.

Steam is a trademark of Valve Corporation. SteamDrop is not affiliated with Valve, SteamGridDB, or any game publisher.
