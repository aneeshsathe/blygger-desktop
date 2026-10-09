# Install and update

Burrow runs on macOS 11 (Big Sur) or later, on Apple silicon or Intel, and
on 64-bit Windows 10 or 11 (see [Windows](#windows)).

## Install (recommended: one line in Terminal)

```sh
curl -fsSL https://raw.githubusercontent.com/aneeshsathe/blygger-desktop/main/scripts/install.sh | bash
```

This downloads the latest release, checks it against the release's
`SHA256SUMS`, and puts **Burrow.app** in `/Applications`, or in
`~/Applications` if you can't write to `/Applications`. To launch it, run
`open /Applications/Burrow.app` or find it in Spotlight. If a `Blygger.app`
from before the rename is in the same folder, the installer replaces it with
`Burrow.app` (your posts, settings and sign-ins aren't in the app, so they
carry over).

Why Terminal? Releases aren't notarized by Apple yet. Files downloaded with
`curl` don't get macOS's quarantine flag, so Gatekeeper doesn't block the app.
The script also clears the flag explicitly, in case a future macOS adds it.

To read the script before running it:

```sh
curl -fsSL -o install.sh https://raw.githubusercontent.com/aneeshsathe/blygger-desktop/main/scripts/install.sh
less install.sh
bash install.sh
```

Or install by hand, verifying the checksum yourself:

```sh
cd "$(mktemp -d)"
base=https://github.com/aneeshsathe/blygger-desktop/releases/latest/download
curl -fLO "$base/Burrow-macos-universal.zip" -fLO "$base/SHA256SUMS"
grep ' Burrow-macos-universal.zip$' SHA256SUMS | shasum -a 256 -c -   # must print "OK"
ditto -x -k Burrow-macos-universal.zip /Applications
open /Applications/Burrow.app
```

`BLYGGER_VERSION=0.1.0` installs a specific version, and `BLYGGER_DEST=<dir>`
installs somewhere else.

## Downloading from the Releases page in a browser

The [Releases page](https://github.com/aneeshsathe/blygger-desktop/releases/latest)
has a `.dmg` and a `.zip`. Files downloaded in a browser are quarantined, and
because the app is only ad-hoc signed (not notarized), macOS blocks the first
launch. On macOS 15 (Sequoia) and later, you see a dialog saying **"Burrow"
Not Opened**, with the message that Apple could not verify it is free of
malware, and only **Done** and **Move to Trash** buttons. Control-click →
**Open** no longer gets around this on macOS 15 and later. To allow it:

1. Drag Burrow to Applications and try to open it once. Click **Done**.
2. Open **System Settings › Privacy & Security** and scroll down to
   **Security**. It says "Burrow" was blocked to protect your Mac. Click
   **Open Anyway**. The button only appears for about an hour after the
   blocked attempt.
3. Confirm with **Open Anyway** in the next dialog and enter your password.
   After that, Burrow opens normally.

Or skip all that with Terminal:
`xattr -dr com.apple.quarantine /Applications/Burrow.app`

On macOS 14 and earlier, Control-click → **Open** → **Open** still works.

## Updates

From 0.3.0 on, Burrow updates itself. About once a day it checks the
[Releases page](https://github.com/aneeshsathe/blygger-desktop/releases/latest),
downloads a new version in the background, and shows **Burrow X is ready ·
Restart to update** in the status bar (quitting installs it too). **Burrow ›
Check for Updates…** checks right away.

Updates are signed: the app installs a release only if its `SHA256SUMS` carries
a valid Ed25519 signature from the project's release key (built into the app),
the download matches that file, and the new app is Burrow (the same bundle
ID, `org.blygger.desktop`, as before the rename), newer, and passes
`codesign --verify`. Downloads come only from GitHub, over HTTPS. The one-line
installer checks the same signature when OpenSSL 3 is installed.

`auto-update = notify` in the config only tells you a new version is out, and
`auto-update = off` stops the automatic checks (see
[`auto-update`](config.md#auto-update)). If Burrow can't replace itself (say,
the folder it's in isn't writable), it says so and links to the release page.

**On 0.2.0 or earlier?** Those versions can't update themselves. Run the
one-line installer above once more; after that, updates are automatic.

**Burrow › About Burrow** shows the version, commit, update status and what
your blyg's server supports. Its **Copy build info** button copies all of that
(no paths, no token) for a bug report.

## The rename

The app was called Blygger (Blygger Desktop) up to 0.6.0. Only the name you
see changed: the menus (Burrow › Settings…, Burrow › Check for Updates…,
Help › Burrow Tutorial), the About window, the window title and the app's
messages say Burrow, and new installs are `Burrow.app`. The bundle ID
(`org.blygger.desktop`), the config file (`~/.config/blygger/config`), the
`BLYGGER_*` environment variables, the `blygger` command, the data folder and
database, the Keychain items and the key-binding contexts are all the same, so
nothing needs migrating.

### Blygger.app → Burrow.app

**Updating from 0.6.0 or earlier:** the in-app update works as usual. The app
replaces itself where it is, so it stays `Blygger.app` on disk (Finder and
Spotlight show that name) while being Burrow inside. That's expected. To get
`Burrow.app`:

- run the [one-line installer](#install-recommended-one-line-in-terminal),
  which replaces a `Blygger.app` in the same folder (posts, settings and
  sign-ins carry over), or
- rename the app yourself while it isn't running.

A Dock shortcut to the old name may need re-adding.

For a transition period, each release also publishes
`Blygger-<version>-macos-universal.zip` and `Blygger-macos-universal.zip` (the
same signed app in a folder called `Blygger.app`), so 0.6.0 and earlier can
still update. The signed `SHA256SUMS` covers them too.

## Windows

The Windows build is the same app, from the same code, with a few
Windows-specific touches. The port was contributed by Patrick Atwater
([@patwater](https://github.com/patwater)).

### Install

Download `Burrow-windows-x64.zip` (or `Burrow-<version>-windows-x64.zip`)
from the [Releases page](https://github.com/aneeshsathe/blygger-desktop/releases/latest),
check it against `SHA256SUMS` if you like, unzip it anywhere, and run
`Burrow\blygger.exe`. In PowerShell:

```powershell
$base = "https://github.com/aneeshsathe/blygger-desktop/releases/latest/download"
Invoke-WebRequest "$base/Burrow-windows-x64.zip" -OutFile Burrow-windows-x64.zip
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
(Get-FileHash Burrow-windows-x64.zip -Algorithm SHA256).Hash.ToLower()  # compare with its line in SHA256SUMS
Expand-Archive Burrow-windows-x64.zip -DestinationPath "$env:LOCALAPPDATA\Programs"
& "$env:LOCALAPPDATA\Programs\Burrow\blygger.exe"
```

The build isn't code-signed yet, so the first launch shows Microsoft Defender
SmartScreen's **Windows protected your PC**. Choose **More info**, then **Run
anyway**; once per version.

Posts and the full editor need the Microsoft Edge **WebView2 Runtime**.
Windows 11 includes it, and most Windows 10 machines have it through Edge. If
posts say "The preview couldn't start", install the runtime from Microsoft's
WebView2 download page.

### What's different from the Mac

- **Keys.** Every ⌘ shortcut is **Ctrl** on Windows: Publish is Ctrl+Enter,
  search is Ctrl+L. The one exception is Versions, Ctrl+Shift+Y rather than
  Ctrl+Y, which is Redo on Windows. Tooltips, the menu, the tour and the app's
  own messages show the Windows keys. The [shortcut tables](keys.md) are
  written the Mac way.
- **Menu.** Windows has no menu bar, so a **Menu** button at the top left of
  the window lists every menu item, including the ones without a key
  (Subscribe…, Site Settings…, Open Config File).
- **Title bar.** The window keeps the normal Windows title bar.
- **Sheets.** Their keys (⏎, esc, 1, 2) are buttons too, and Settings › AI
  has **Save** and **Done** buttons.
- **Where things live.** The config file is
  `%USERPROFILE%\.config\blygger\config`, as on the Mac (Menu › Open Config
  File creates it and opens it in Notepad); `%APPDATA%\Blygger\config` is
  read after it, in place of the Mac's `~/Library/Application Support` one,
  and user themes go in `%USERPROFILE%\.config\blygger\themes\`. The local database, caches and media are in
  `%LOCALAPPDATA%\Blygger\`. Sign-ins and AI keys are in **Windows Credential
  Manager** under `org.blygger.desktop`, wherever the Mac app says Keychain.
- **A refused token.** When your blyg starts refusing your sign-in (401), the
  app says so once, rather than only "sync error".
- **Fonts.** The macOS system fonts become their nearest Windows ones: New
  York → Georgia, Charter → Cambria, SF Pro → Segoe UI, Menlo → Consolas.
  The bundled fonts, including the defaults, are the same everywhere.
- **The browser pane** is macOS only for now: links open in your default
  browser.
- **Updates.** The Windows build doesn't update itself yet. Download new
  versions from the Releases page; your posts, settings and sign-ins aren't in
  the folder, so they carry over.
- **AI helpers.** Local Claude Code and Codex are found on your `PATH`, in
  `%USERPROFILE%\.local\bin`, and in npm's global folder (`%APPDATA%\npm`).

### If it crashes

A crash shows a message and writes what happened to
`%LOCALAPPDATA%\Blygger\crash.log` (the latest crash only). Please attach it
when you report the bug.

### Known gaps

No installer, auto-update or code signing yet. Keyboard focus between the
preview and the editor is simpler than on the Mac: when the preview hides or
the window regains focus, the keyboard goes back to the editor.

### Uninstall on Windows

Delete the `Burrow` folder. To remove everything else: `%LOCALAPPDATA%\Blygger\`
(the local copy), `%USERPROFILE%\.config\blygger\` (and
`%APPDATA%\Blygger\` if you made one), and the `org.blygger.desktop`
entries in Credential Manager (Control Panel › Credential Manager › Windows
Credentials).

## Uninstall

Quit Burrow and move `Burrow.app` (or `Blygger.app`) to the Trash. Your posts
live on your blyg; the local copy, settings and sign-ins stay on your Mac
until you remove them:

- the data folder, `~/Library/Application Support/org.blygger.desktop/`
  (**Burrow › Disconnect…** can also delete the local copy);
- the config file, `~/.config/blygger/config`;
- the Keychain items, under the service `org.blygger.desktop` (search
  Keychain Access for `blygger`).
