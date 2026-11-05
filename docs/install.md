# Installing Islet

Download the file for your computer from the [latest release](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest). Each release lists SHA-256 checksums in `SHA256SUMS` if you want to verify a download:

```bash
sha256sum -c SHA256SUMS --ignore-missing     # Linux
shasum -a 256 -c SHA256SUMS --ignore-missing  # macOS
```

After installing, open **Islet** from your apps. Settings opens with a short welcome: choose **Start now and at login**, and the island appears at the top of your screen.

## Windows 10 and 11

- **Installer:** `Islet_<version>_x64-setup.exe` (or the `.msi`).
- **Portable:** `Islet_<version>_x64_portable.zip`. Unzip anywhere and run `islet-settings.exe`.

- **MSIX package:** `Islet_<version>_x64.msix`. Choose this if you want **notifications on the island**: Windows lets only packaged apps read other apps' notifications. It also starts at login through Windows' own startup list (Task Manager → Startup apps).

### Installing the MSIX

Until Islet has a real code-signing certificate, the MSIX is signed with a test certificate, `Islet-test-certificate.cer`, which comes with it. Windows installs a package only if it trusts the signer, so trust the certificate once (this needs an administrator):

1. Double-click `Islet-test-certificate.cer` → **Install Certificate** → **Local Machine** → **Place all certificates in the following store** → **Browse** → **Trusted People** → **OK** → **Next** → **Finish**.
2. Double-click `Islet_<version>_x64.msix` → **Install**.

Or in an administrator PowerShell, from the download folder:

```powershell
Import-Certificate -FilePath .\Islet-test-certificate.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage .\Islet_<version>_x64.msix
```

Then open **Islet** (the island) or **Islet Settings** from the Start menu. To use notifications, turn on **Settings → Notifications → Show notifications on the island**; Windows asks once for permission. A packaged app keeps its settings in its own folder, so the MSIX doesn't share settings with an `.msi`/`.exe` install, and new versions come as a new MSIX (Settings opens the download page). Uninstall it like any app (Settings → Apps), and remove the certificate with `certlm.msc` → Trusted People if you like.

Islet isn't code-signed yet, so **SmartScreen** may say "Windows protected your PC". Click **More info**, then **Run anyway**. Windows 11 may ask for **Location** permission the first time you open the Wi-Fi list, because Windows requires it before apps can see network names.

## macOS 11 and later (Apple silicon and Intel)

Open `Islet_<version>_universal.dmg` and drag **Islet** to **Applications**.

Islet isn't notarized yet. On macOS 15 and later, right-click → Open no longer gets past Gatekeeper. Instead:

1. Open Islet once. macOS says it can't verify the developer. Click **Done**.
2. Open **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next to the Islet message.
3. Open Islet again and confirm.

If macOS says Islet "is damaged and can't be opened", that's the quarantine flag on an unsigned download. Clear it:

```bash
xattr -dr com.apple.quarantine /Applications/Islet.app
```

macOS will ask once for **Bluetooth** (battery levels and connecting devices), and for **Location** the first time you open the Wi-Fi list (macOS 14+ shows network names only to apps with Location access). Islet never reads your location.

## Linux

| Distro | File | Install |
|---|---|---|
| Debian, Ubuntu, Mint, Pop!_OS | `.deb` | `sudo apt install ./Islet_<version>_amd64.deb` |
| Fedora, openSUSE | `.rpm` | `sudo dnf install ./Islet-<version>-1.x86_64.rpm` |
| Anything else | `.AppImage` | `chmod +x Islet_<version>_amd64.AppImage` and run it |

- The AppImage includes its own WebKitGTK for the Settings window, so it's much larger than the packages (the island itself is the same small binary).
- Saving a Google Routes key needs `secret-tool` (package `libsecret-tools` on Debian/Ubuntu, `libsecret` on Fedora/Arch) and a running keyring (GNOME Keyring or KWallet).
- Wi-Fi uses NetworkManager and Bluetooth uses BlueZ, which most desktops already run.
- On GNOME with Wayland, Islet runs through XWayland. It works without anything else, but GNOME hides global keys and the Caps Lock state from XWayland apps. The **GNOME Shell extension** fills that in (see below).

### GNOME Shell extension (GNOME 45–50)

`islet@jimchi-cmd.github.io.shell-extension.zip` adds, on GNOME:

- a shortcut (default **Ctrl+Alt+I**) that opens and closes the island;
- the Caps Lock pop-up;
- an option to hide the top bar's clock, which the island sits over.

Install it and log out and back in (GNOME on Wayland loads new extensions at login):

```bash
gnome-extensions install --force islet@jimchi-cmd.github.io.shell-extension.zip
# log out and back in, then:
gnome-extensions enable islet@jimchi-cmd.github.io
gnome-extensions prefs islet@jimchi-cmd.github.io   # shortcut, Caps Lock, clock
```

It runs Islet's command line (`islet toggle`, `islet caps-lock on|off`), so the `islet` program must be installed (the `.deb`/`.rpm` put it in `/usr/bin`; for an AppImage or a build, set its path in the extension's settings). It makes no network requests.

## Updating

**Settings → Updates** checks GitHub for a new version. Automatic checks are off by default. Builds signed with an update key install updates in place (Windows, macOS and the AppImage). Otherwise Settings opens the download page. `.deb` and `.rpm` installs update by installing the new package.

## Uninstalling

- **Windows:** Settings → Apps → Islet → Uninstall.
- **macOS:** quit Islet (right-click the island), then drag Islet from Applications to the Bin.
- **Linux:** `sudo apt remove islet` or `sudo dnf remove islet`, or delete the AppImage.

Your settings live in `%APPDATA%\Islet`, `~/Library/Application Support/Islet` or `~/.config/islet`. Delete that folder to remove them, and turn off **Start at login** first (or run `islet autostart off`). The Google key stays in your keychain until you remove it (`islet key routes --clear`).
