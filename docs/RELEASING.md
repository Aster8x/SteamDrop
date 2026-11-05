# Releasing SteamDrop

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when you push a tag such as `v0.2.0`. The workflow builds the installer, signs it for the in-app updater, and publishes a GitHub release containing:

- `SteamDrop-Setup.exe`: the installer the README's download button points to
- `SteamDrop-Setup.exe.sig`: the updater signature
- `latest.json`: what installed copies check for updates

## One-time setup: the updater signing key

The updater only installs updates signed with your private key. You make the key once and keep it safe. If you lose it, existing installs can't update any more.

1. On your PC, in the project folder, run:

   ```bash
   npx tauri signer generate -w ~/.tauri/steamdrop.key
   ```

   Choose a password when asked (or leave it empty). This creates two files:
   - `~/.tauri/steamdrop.key`: the **private** key. Never commit or share it.
   - `~/.tauri/steamdrop.key.pub`: the **public** key.

2. Open `steamdrop.key.pub`, copy its whole content (one long line), and paste it into `src-tauri/tauri.conf.json` in place of `REPLACE_WITH_YOUR_UPDATER_PUBLIC_KEY` (under `plugins` → `updater` → `pubkey`). Commit that change. The public key is meant to be public.

3. On GitHub, open the repository's **Settings → Secrets and variables → Actions → New repository secret** and add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the whole content of `steamdrop.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password you chose (skip it if you left the password empty)

The release workflow stops with a clear message if either the public key or the secret is missing.

## Making a release

1. Change `version` in `src-tauri/Cargo.toml` (for example to `0.2.0`), commit, and push to `main`.
2. Tag that commit and push the tag:

   ```bash
   git tag v0.2.0
   ```

   ```bash
   git push origin v0.2.0
   ```

3. Wait for the **Release** workflow to finish, then edit the release notes on GitHub if you like.

The tag must match the version in `Cargo.toml`, or the workflow stops.

## Local builds

`npx tauri build` also needs the signing key, because the config asks for updater files. To build without it:

```bash
npx tauri build --config "{\"bundle\":{\"createUpdaterArtifacts\":false}}"
```

## The uninstaller template

`src-tauri/windows/installer.nsi` is a copy of Tauri's NSIS template (tauri-bundler 2.10.4) with the uninstall page changed to show SteamDrop's two checkboxes. After a major Tauri upgrade, compare it with the new upstream template and carry the changes over. They are marked with `SteamDrop:` comments.
