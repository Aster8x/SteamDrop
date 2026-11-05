<div align="center">

<img src="src-tauri/icons/128x128@2x.png" width="112" alt="SteamDrop icon">

# SteamDrop

**Drop a game folder in, play it from Steam.**

SteamDrop adds the games on your PC to your Steam library automatically, with proper artwork.

<a href="https://github.com/Aster8x/SteamDrop/releases/latest/download/SteamDrop-Setup.exe"><img src="https://img.shields.io/badge/Download%20for%20Windows-5CC8FF?style=for-the-badge&logo=windows&logoColor=052131&labelColor=5CC8FF&color=5CC8FF" height="56" alt="Download for Windows"></a>

<sub>Free and open source · Windows 10 and 11 (64-bit) · <a href="https://github.com/Aster8x/SteamDrop/releases">All releases</a></sub>

</div>

---

## What it is

SteamDrop is a small app that lives next to your clock. You tell it which folder your games go into. When a new game folder shows up there, SteamDrop waits until copying has finished, finds the file that starts the game, and adds it to Steam as a non-Steam game, complete with cover art, a banner, a background, a logo and an icon. Then you just open Steam and play.

It works with any game folder on your own PC, including DRM-free games (for example from GOG or itch.io), games you made yourself, and games that use Steam emulators.

## What it does

1. **Watches your game folders.** It uses Windows' own change notifications, so it uses no CPU while nothing happens.
2. **Waits for copying to finish.** It looks at a folder only once nothing in it has changed for a while and no file is still being written.
3. **Finds the right .exe.** It skips uninstallers, crash reporters, setup files and other helpers, and picks the file whose name best matches the game. You can always change the pick.
4. **Adds it to Steam** for every Steam account on the PC, at the top of your "Recent" games.
5. **Fills in all the artwork:** official Steam art when the game is on Steam, otherwise art from SteamGridDB, otherwise a clean title card.
6. **Restarts Steam** (into Big Picture) so the game shows up right away, if you allow it.

## Who it's for

- People who keep games outside Steam and want them all in one place.
- Anyone who plays from the couch in Big Picture mode or on a TV.
- Older or less technical players: big buttons, plain words, and nothing to configure if you don't want to.

## Features

- **Automatic mode** (off by default). Turn it on and SteamDrop adds games, restarts Steam and tells you, all by itself. Or keep it off and confirm each game with one click.
- **Complete folders only.** If a folder contains only an installer, SteamDrop skips it and tells you to install the game first.
- **Full artwork** in every slot Steam uses: portrait cover, wide banner, hero background, logo and icon. Only real artwork from Steam or SteamGridDB, or a clean text title card.
- **Game details** (description, developer, genres) from the Steam store, with a link to the store page.
- **Safe.** SteamDrop backs up Steam's shortcuts file before every change. "Remove from Steam" removes only the shortcut, never your game files.
- **Light on your PC.** The window is fully closed while SteamDrop sits in the tray. The installer is only a few megabytes.
- **Dark and light looks**, large readable text (Atkinson Hyperlegible).
- **Stays up to date:** checks for new versions on startup and once a day, shows what changed, and can install updates by itself (never while you're playing).

## Couch mode

Play from the sofa? SteamDrop can add **itself** to your Steam library, with its own artwork. Start it from Steam in Big Picture (or on a Steam Deck) and it opens **Couch mode**: a full-screen view you control with an Xbox or PlayStation controller. Started from normal desktop Steam, it opens its usual window instead. Settings → Couch mode → "When opened from Steam" can make it always one or the other, and **F11** switches between the two views anytime.

- **Library:** all the games SteamDrop added, as big cover tiles.
- **Game details:** find the artwork again, change which file starts the game, remove it from Steam, or delete the game.
- **Settings** and **Activity**, readable from across the room.

| Button | Does |
|---|---|
| D-pad / left stick | Move |
| A (Cross) | Select |
| B (Circle) | Back |
| LB / RB (L1 / R1) | Switch between Library, Settings and Activity |
| Y (Triangle) | Scan now: look for new games right away |
| View (Share) | Switch to the desktop window |
| Menu (Options) | Back to Steam (SteamDrop keeps running) |

Both views have the same control bar: **Scan now**, **Hide** ("Back to Steam" in Couch mode), **Close** (closes the window, SteamDrop keeps watching from the tray) and **Quit SteamDrop** (stops watching completely, after asking).

**Deleting a game** needs you to hold A for 2 seconds. The game folder goes to the **Recycle Bin**, never deleted for good (on drives without a Recycle Bin, SteamDrop refuses). You can also set a 4-digit PIN in Couch mode's Settings so nobody deletes games by accident.

The installer has an **Add SteamDrop to Steam** box (on by default). You can change it later in Settings → Couch mode, where there's also an **Open Couch mode** button. Changes made in Couch mode show in Steam's library after Steam restarts. When you leave Couch mode, SteamDrop offers to restart Steam for you and brings you back to Big Picture.

## Screenshots

> _Screenshots coming soon._

## Install

1. Download **[SteamDrop-Setup.exe](https://github.com/Aster8x/SteamDrop/releases/latest/download/SteamDrop-Setup.exe)**.
2. Run it. No administrator rights are needed.
3. In SteamDrop, press **Choose a folder to watch** and pick the folder that holds your games (for example `D:\Games`).
4. That's it. Copy a game folder in and SteamDrop takes care of the rest.

Windows SmartScreen may warn you because the installer isn't code-signed yet. Press **More info**, then **Run anyway**.

**To uninstall**, use Windows' "Installed apps" settings. The uninstaller asks whether to also remove SteamDrop's games from Steam, and whether to delete its settings and cache. Both are off unless you tick them.

## FAQ

**Does SteamDrop download games?**
No. It only adds games that are already on your PC to your Steam library.

**Will it touch my game files?**
No. It only reads them to find the right .exe. The one exception is if you turn on "Also save art in the game folder", which adds a `SteamDrop art` folder with a copy of the pictures.

**Steam has to restart? Why?**
Steam only reads its list of non-Steam games when it starts. SteamDrop closes Steam for a moment, adds the game and opens Steam again. If Steam is open and you'd rather not restart, SteamDrop can add the game anyway and it shows up the next time Steam starts.

**The artwork is a plain title card. Can I get real art?**
Real art comes from Steam (when the game is on Steam) or from SteamGridDB. For SteamGridDB you need a free key: in SteamDrop go to **Settings → Artwork** and press **Get a free key**. Sign in on that page with your Steam account, press *Generate API Key*, copy the key, paste it into SteamDrop, and press **Test key**, then **Save key**. The key is stored in Windows Credential Manager, not in a settings file. Then open the game's details and press **Find artwork again**. If SteamDrop picked the wrong game, enter its Steam App ID in the details window.

**It picked the wrong .exe.**
With Automatic mode off, you confirm the pick before anything is added. Otherwise, remove the game in SteamDrop and add its folder again with **+ Add a game**, then choose the right file.

**I have more than one Steam account on this PC.**
SteamDrop adds games to every account that has signed in on this PC.

**Something went wrong. How do I report it?**
Go to **Settings → Help** and press **Copy bug report**. Then press **Report a problem** (or open a [new issue](https://github.com/Aster8x/SteamDrop/issues/new?template=bug_report.yml)), describe what happened, and paste the report into the last box. The report holds SteamDrop's version, your Windows version, whether Steam was found, your settings, and the last 200 lines of SteamDrop's activity. Your Windows user name is replaced with `<user>`, and your SteamGridDB key is never included. The **Activity** list on the same page shows what SteamDrop did recently, in plain words.

**Where does SteamDrop keep its files?**
In `%APPDATA%\SteamDrop`: settings, the artwork cache (`art`), store details (`info`), backups of Steam's shortcuts file (`backups`), and the activity log (`logs`, the last 5 files of up to 1 MB each). The SteamGridDB key is kept in Windows Credential Manager.

**How do I undo a change?**
Every version of Steam's shortcuts file is backed up in `%APPDATA%\SteamDrop\backups\<account number>`. Close Steam, copy the backup you want back to `Steam\userdata\<account number>\config\shortcuts.vdf`, and start Steam.

## Building from source

You need [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for Windows.

```bash
npm install
```

```bash
npm run dev
```

```bash
npx tauri build --config src-tauri/tauri.no-updater.conf.json
```

Run the tests with `cargo test --manifest-path src-tauri/Cargo.toml`. For releases, see [docs/RELEASING.md](docs/RELEASING.md). What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

## Credits

- Built with [Tauri](https://tauri.app/).
- Fonts: [Chakra Petch](https://github.com/m4rc1e/Chakra-Petch) and [Atkinson Hyperlegible](https://www.brailleinstitute.org/freefont/), under the SIL Open Font License.
- Artwork from the Steam store and [SteamGridDB](https://www.steamgriddb.com/), fetched when needed.

See [THIRD_PARTY.md](THIRD_PARTY.md) for details.

## Donate

> _Donation link coming soon. If SteamDrop is useful to you, starring the repository helps too._

## License

[MIT](LICENSE).

## Legal

SteamDrop is an independent, open source tool for adding games stored on your own PC to your Steam library. It does not provide, host, or link to any games, and does not encourage or enable illegal access to copyrighted material. Only use it with games you have the right to use. SteamDrop is not affiliated with any cracking scene groups, nor with Valve Corporation or any game publisher. Steam is a trademark of Valve Corporation. All artwork and game information belong to their respective owners and are fetched at runtime from public sources. Provided as-is, with no warranty of any kind; use at your own risk.

## How it was made

SteamDrop is a passion project by Aster8x. AI tools helped with development and testing, and every release is tested by hand on real hardware.
